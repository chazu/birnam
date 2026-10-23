//! The native image owner. Descriptor data and code residency stay here;
//! generic Id roots and dispatch do not read module records.
use super::native_id::State;
use crate::{abi::{self, Oop, id_protocol as native}, raw::AddressMap};
use core::{cell::RefCell, ffi::c_void};
struct ModuleRecord {
    _handle: *mut c_void,
    declarations: &'static [native::MethodSpec],
    literals: Vec<Oop>,
    symbols: Vec<Oop>,
    methods: Vec<(Oop, Oop)>,
    result: Oop,
}

pub(super) struct NativeModules {
    records: RefCell<AddressMap<ModuleRecord>>,
    pub registry: Oop,
    pub registry_behavior: Oop,
}
impl NativeModules {
    pub fn new(registry: Oop, registry_behavior: Oop) -> Self {
        Self { records: RefCell::new(AddressMap::default()), registry, registry_behavior }
    }
    pub fn entry(&self, owner: Oop, index: usize) -> Option<native::Method> {
        self.records.borrow().get(&owner.addr())?.declarations.get(index)?.entry
    }
    pub fn literal(&self, owner: Oop, index: usize) -> Option<Oop> {
        self.records.borrow().get(&owner.addr())?.literals.get(index).copied()
    }
    pub fn method(&self, owner: Oop, name: Oop) -> Option<Oop> {
        self.records.borrow().get(&owner.addr())?.methods.iter()
            .find(|(key, _)| *key == name).map(|(_, value)| *value)
    }
    pub fn references(&self, object: Oop) -> Option<Vec<Oop>> {
        if object != self.registry { return None; }
        let mut values = Vec::new();
        for module in self.records.borrow().values() {
            values.extend(&module.literals);
            values.extend(&module.symbols);
            for (key, value) in &module.methods { values.extend([*key, *value]); }
            values.push(module.result);
        }
        Some(values)
    }
}
impl State {
    pub(super) unsafe fn load_module(&self, path: Oop, args: &[Oop]) -> Result<Oop, Oop> {
        let (p, n) = self.name_data(path)?;
        let path_bytes = unsafe { core::slice::from_raw_parts(p, n) };
        let Some(handle) = (unsafe { abi::native_module::open(path_bytes) }) else {
            return Err(self.make_fault(self.selector(b"mappingFailed"), path));
        };
        let descriptor = unsafe { abi::native_module::symbol(handle, c"birnam_native_module_v2") }
            .cast::<native::Module>();
        if descriptor.is_null() {
            unsafe { abi::native_module::close(handle) };
            return Err(self.make_fault(self.selector(b"incompatibleModule"), path));
        }
        if unsafe { (*descriptor).version } != native::VERSION
            || unsafe { (*descriptor).byte_size } as usize != size_of::<native::Module>()
        {
            unsafe { abi::native_module::close(handle) };
            return Err(self.make_fault(self.selector(b"incompatibleModule"), path));
        }
        let d = unsafe { &*descriptor };
        let Some(initialize) = d.initialize else {
            unsafe { abi::native_module::close(handle) };
            return Err(self.make_fault(self.selector(b"incompatibleModule"), path));
        };
        if d.literal_count > 65536 || d.method_count > 65536 {
            unsafe { abi::native_module::close(handle) };
            return Err(self.invalid());
        }
        let literals =
            unsafe { abi::borrowed_slice(d.literals, d.literal_count) }.ok_or(self.invalid())?;
        let methods =
            unsafe { abi::borrowed_slice(d.methods, d.method_count) }.ok_or(self.invalid())?;
        let image = self.allocate(self.module, 0)?;
        self.modules.records.borrow_mut().insert(
            image.addr(),
            ModuleRecord {
                _handle: handle,
                declarations: methods,
                literals: Vec::new(),
                symbols: Vec::new(),
                methods: Vec::new(),
                result: core::ptr::null_mut(),
            },
        );
        // Native images, including partly initialized ones, stay resident. Publication
        // cannot be rolled back. The bounded slice has no native image-unload policy.
        self.permanent.borrow_mut().push(image);
        for literal in literals {
            if literal.length > 16 * 1024 * 1024 {
                return Err(self.invalid());
            }
            let bytes = unsafe { abi::borrowed_slice(literal.bytes, literal.length) }
                .ok_or(self.invalid())?;
            let values: Vec<_> = bytes.iter().map(|&b| abi::size(b as usize).unwrap()).collect();
            let name = unsafe { self.send(self.root, b"nameWithBytes", &values) }?;
            self.modules.records
                .borrow_mut()
                .get_mut(&image.addr())
                .unwrap()
                .literals
                .push(name);
        }
        let names = self
            .modules.records
            .borrow()
            .get(&image.addr())
            .unwrap()
            .literals
            .clone();
        for name in names {
            let symbol = unsafe { self.send(name, b"intern", &[]) }?;
            self.modules.records
                .borrow_mut()
                .get_mut(&image.addr())
                .unwrap()
                .symbols
                .push(symbol);
        }
        for (index, method) in methods.iter().enumerate() {
            let selector = *self
                .modules.records
                .borrow()
                .get(&image.addr())
                .unwrap()
                .symbols
                .get(method.name)
                .ok_or(self.invalid())?;
            let method = unsafe {
                self.send(image, b"newMethodAt:", &[abi::size(index).unwrap()])
            }?;
            self.modules.records
                .borrow_mut()
                .get_mut(&image.addr())
                .unwrap()
                .methods
                .push((selector, method));
        }
        let names = self
            .modules.records
            .borrow()
            .get(&image.addr())
            .unwrap()
            .symbols
            .clone();
        let result = unsafe {
            initialize(
                &self.protocol,
                image,
                names.as_ptr(),
                names.len(),
                args.as_ptr(),
                args.len(),
            )
        };
        self.modules.records
            .borrow_mut()
            .get_mut(&image.addr())
            .unwrap()
            .result = result;
        self.behavior(result)?;
        if abi::is_fault(result) {
            Err(result)
        } else {
            Ok(result)
        }
    }
}
