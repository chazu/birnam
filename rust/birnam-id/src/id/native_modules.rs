//! The native image owner. Descriptor data and code residency stay here;
//! generic Id roots and dispatch do not read module records.
use super::native_id::State;
use crate::{abi::{self, Oop, id_protocol as native}, raw::AddressMap};
use core::{cell::RefCell, ffi::c_void};
enum Initializer {
    Synchronous(native::Initialize),
    Continuation(native::ContinuationInitialize),
}
enum Declarations {
    Synchronous(&'static [native::MethodSpec]),
    Mixed(&'static [native::ContinuationMethodSpec]),
}
impl Declarations {
    fn len(&self)->usize { match self { Self::Synchronous(v)=>v.len(), Self::Mixed(v)=>v.len() } }
    fn name(&self,index:usize)->usize { match self { Self::Synchronous(v)=>v[index].name,Self::Mixed(v)=>v[index].name } }
    fn entry(&self,index:usize)->Option<(native::Method,bool)> {
        match self {
            Self::Synchronous(v)=>Some((v.get(index)?.entry?,false)),
            Self::Mixed(v)=>{let d=v.get(index)?; Some((d.entry?,d.convention==native::CONTINUATION_ENTRY))}
        }
    }
}
struct ModuleRecord {
    _handle: *mut c_void,
    declarations: Declarations,
    literals: Vec<Oop>,
    symbols: Vec<Oop>,
    methods: Vec<(Oop, Oop)>,
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
    pub fn entry(&self, owner: Oop, index: usize) -> Option<(native::Method,bool)> {
        let records=self.records.borrow(); let record=records.get(&owner.addr())?;
        record.declarations.entry(index)
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
        let continuing = unsafe { abi::native_module::symbol(handle, c"birnam_native_module_v3") }.cast::<native::Module>();
        let (descriptor, version) = if !continuing.is_null() { (continuing,native::CONTINUATION_MODULE_VERSION) }
            else { (unsafe { abi::native_module::symbol(handle, c"birnam_native_module_v2") }.cast::<native::Module>(),native::VERSION) };
        if descriptor.is_null() || unsafe { (*descriptor).version } != version
            || unsafe { (*descriptor).byte_size } as usize != size_of::<native::Module>() {
            unsafe { abi::native_module::close(handle) };
            return Err(self.make_fault(self.selector(b"incompatibleModule"), path));
        }
        let (literals, literal_count, methods, method_count, initialize) = if version == native::CONTINUATION_MODULE_VERSION {
            let d=unsafe { &*descriptor.cast::<native::ContinuationModule>() };
            (d.literals,d.literal_count,d.methods.cast::<native::MethodSpec>(),d.method_count,d.initialize.map(Initializer::Continuation))
        } else {
            let d=unsafe { &*descriptor };
            (d.literals,d.literal_count,d.methods,d.method_count,d.initialize.map(Initializer::Synchronous))
        };
        let Some(initialize) = initialize else {
            unsafe { abi::native_module::close(handle) };
            return Err(self.make_fault(self.selector(b"incompatibleModule"), path));
        };
        if literal_count > 65536 || method_count > 65536 {
            unsafe { abi::native_module::close(handle) };
            return Err(self.invalid());
        }
        let literals=unsafe { abi::borrowed_slice(literals,literal_count) }.ok_or(self.invalid())?;
        let methods=if version == native::CONTINUATION_MODULE_VERSION {
            let values=unsafe { abi::borrowed_slice(methods.cast::<native::ContinuationMethodSpec>(),method_count) }.ok_or(self.invalid())?;
            if values.iter().any(|v|!matches!(v.convention,native::SYNCHRONOUS_ENTRY|native::CONTINUATION_ENTRY)) {
                unsafe { abi::native_module::close(handle) };
                return Err(self.make_fault(self.selector(b"incompatibleModule"),path));
            }
            Declarations::Mixed(values)
        } else { Declarations::Synchronous(unsafe { abi::borrowed_slice(methods,method_count) }.ok_or(self.invalid())?) };
        let method_names:Vec<_>=(0..methods.len()).map(|i|methods.name(i)).collect();
        let image = self.allocate(self.module, 0)?;
        self.modules.records.borrow_mut().insert(
            image.addr(),
            ModuleRecord {
                _handle: handle,
                declarations: methods,
                literals: Vec::new(),
                symbols: Vec::new(),
                methods: Vec::new(),
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
        for (index, name) in method_names.into_iter().enumerate() {
            let selector = *self
                .modules.records
                .borrow()
                .get(&image.addr())
                .unwrap()
                .symbols
                .get(name)
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
            match initialize {
                Initializer::Synchronous(initialize)=>initialize(&self.protocol,image,names.as_ptr(),names.len(),args.as_ptr(),args.len()),
                Initializer::Continuation(initialize)=>{
                    let protocol=self.continuation_protocol.get_or_init(||{
                        let mut common=self.protocol;
                        common.version=native::CONTINUATION_MODULE_VERSION;
                        common.byte_size=size_of::<native::ContinuationProtocol>() as u32;
                        native::ContinuationProtocol{common,control:super::continuation::control}
                    });
                    initialize(protocol,image,names.as_ptr(),names.len(),args.as_ptr(),args.len())
                }
            }
        };
        // The loader's caller owns the initializer result. Keeping code resident
        // does not retain that result or its object graph after explicit release.
        self.behavior(result)?;
        if abi::is_fault(result) {
            Err(abi::object_reference(result))
        } else {
            Ok(result)
        }
    }
}
