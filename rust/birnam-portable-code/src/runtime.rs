use birnam_abi::{Finalizer, IdApi, MarkFn, Method, Oop, TraceHook};
use core::cell::RefCell;
use core::ffi::{CStr, c_char};
use std::sync::{Once, OnceLock};

use crate::heap::Permanence;

#[derive(Clone, Copy)]
pub(crate) struct CanonicalObjects {
    pub nil: Oop,
}

#[derive(Clone, Copy)]
pub(crate) struct Globals {
    pub object_vt: Oop,
}

pub(crate) struct Runtime {
    api: usize,
    imports: usize,
    exports: usize,
}

unsafe impl Send for Runtime {}
unsafe impl Sync for Runtime {}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static ROOTS_REGISTERED: Once = Once::new();

thread_local! {
    static TEMPORARY_ROOTS: RefCell<Vec<Oop>> = const { RefCell::new(Vec::new()) };
}

pub(crate) struct TemporaryRootScope {
    start: usize,
}

impl Drop for TemporaryRootScope {
    fn drop(&mut self) {
        TEMPORARY_ROOTS.with(|roots| roots.borrow_mut().truncate(self.start));
    }
}

unsafe extern "C" fn mark_roots(mark: MarkFn) {
    TEMPORARY_ROOTS.with(|roots| {
        for root in roots.borrow().iter().copied() {
            unsafe { mark(root) };
        }
    });
}

pub(crate) fn install(api: *mut IdApi, imports: *const birnam_abi::namespace_import::ApiV1, exports: *const birnam_abi::namespace_export::ApiV1) -> &'static Runtime {
    let runtime = RUNTIME.get_or_init(|| Runtime { api: api as usize, imports: imports as usize, exports: exports as usize });
    ROOTS_REGISTERED.call_once(|| unsafe {
        (runtime.api().gcRegisterRootProvider)(mark_roots);
    });
    runtime
}

pub(crate) fn with_active_if_present<T>(body: impl FnOnce(&Runtime) -> T) -> Option<T> {
    RUNTIME.get().map(body)
}

impl Runtime {
    pub(crate) fn imports(&self) -> *const birnam_abi::namespace_import::ApiV1 { self.imports as *const birnam_abi::namespace_import::ApiV1 }
    pub(crate) fn api(&self) -> &'static IdApi {
        unsafe { &*(self.api as *const IdApi) }
    }

    fn c_name(bytes: &[u8]) -> Result<Vec<u8>, ()> {
        if bytes.contains(&0) {
            return Err(());
        }
        let mut name = Vec::with_capacity(bytes.len() + 1);
        name.extend_from_slice(bytes);
        name.push(0);
        Ok(name)
    }

    pub(crate) fn canonical(&self) -> CanonicalObjects {
        CanonicalObjects {
            nil: self.api().nilObj,
        }
    }

    pub(crate) fn globals(&self) -> Globals {
        Globals {
            object_vt: self.api().object_vt,
        }
    }

    pub(crate) fn intern(&self, bytes: &[u8]) -> Result<Oop, ()> {
        let name = Self::c_name(bytes)?;
        Ok(unsafe { (self.api().intern)(name.as_ptr().cast::<c_char>()) })
    }

    pub(crate) fn import(&self, bytes: &[u8]) -> Result<Oop, ()> {
        let name = Self::c_name(bytes)?;
        Ok(unsafe { birnam_abi::namespace_import::import(self.imports(), name.as_ptr().cast::<c_char>()) })
    }

    pub(crate) fn export(&self, bytes: &[u8], value: Oop) -> Result<Oop, ()> {
        let name = Self::c_name(bytes)?;
        Ok(unsafe { birnam_abi::namespace_export::export(self.exports as *const birnam_abi::namespace_export::ApiV1, name.as_ptr().cast::<c_char>(), value) })
    }

    pub(crate) fn create_vtable(&self, parent: Oop) -> Oop {
        unsafe { (self.api().proto)(parent) }
    }

    pub(crate) fn allocate_empty(&self, vtable: Oop, _permanence: Permanence) -> Oop {
        unsafe { (self.api().alloc)(vtable, 0) }
    }

    pub(crate) unsafe fn allocate_instance_active(&self, vtable: Oop, bytes: usize) -> Oop {
        unsafe { (self.api().alloc)(vtable, bytes) }
    }

    pub(crate) fn set_vtable_owner(&self, vtable: Oop, owner: Oop) {
        unsafe { birnam_abi::set_vtable_owner(self.api(), vtable, owner) };
    }

    pub(crate) fn set_layout(&self, vtable: Oop, layout: *const u8, count: usize, rest: u8) {
        unsafe { (self.api().setLayout)(vtable, layout, count, rest) };
    }

    pub(crate) fn set_trace_hook(&self, vtable: Oop, hook: TraceHook) {
        unsafe { (self.api().setTraceHook)(vtable, hook) };
    }

    pub(crate) fn set_finalizer(&self, object: Oop, finalizer: Finalizer) {
        unsafe { (self.api().setFinalizer)(object, finalizer) };
    }

    pub(crate) fn add_method(&self, vtable: Oop, selector: Oop, method: Method) -> bool {
        unsafe { (self.api().method)(vtable, selector, method) };
        true
    }

    pub(crate) fn payload_size(&self, object: Oop) -> Option<usize> {
        (!object.is_null()).then(|| unsafe { (self.api().objectPayloadSize)(object) })
    }

    pub(crate) unsafe fn checked_vtable_of_active(&self, object: Oop) -> Oop {
        unsafe { (self.api().vtableOf)(object) }
    }

    pub(crate) fn vtable_of(&self, object: Oop) -> Oop {
        unsafe { (self.api().vtableOf)(object) }
    }

    pub(crate) fn vtable_parent(&self, vtable: Oop) -> Oop {
        unsafe { birnam_abi::vtable_parent(self.api(), vtable) }
    }

    pub(crate) fn vtable_owner(&self, vtable: Oop) -> Oop {
        unsafe { birnam_abi::vtable_owner(self.api(), vtable) }
    }

    pub(crate) fn vtable_selector_at(&self, vtable: Oop, index: usize) -> Oop {
        unsafe { (self.api().vtableSelectorAt)(vtable, index) }
    }

    pub(crate) fn is_integer(&self, object: Oop) -> bool {
        unsafe { (birnam_abi::integer::is_integer)(object) != 0 }
    }

    pub(crate) fn integer_value(&self, object: Oop) -> i64 {
        unsafe { (birnam_abi::integer::value)(object) }
    }

    pub(crate) fn make_integer(&self, value: i64) -> Oop {
        unsafe { (birnam_abi::integer::make)(value) }
    }

    pub(crate) fn string_bytes(&self, object: Oop) -> Option<&'static [u8]> {
        if object.is_null() || unsafe { (self.api().vtableOf)(object) } != unsafe { birnam_abi::string::behavior() } {
            return None;
        }
        let mut length = 0usize;
        let bytes = unsafe { (birnam_abi::string::bytes)(object, &mut length) };
        if length == 0 {
            return Some(&[]);
        }
        if bytes.is_null() {
            return None;
        }
        Some(unsafe { core::slice::from_raw_parts(bytes, length) })
    }

    pub(crate) fn symbol_bytes(&self, object: Oop) -> Option<&'static [u8]> {
        if object.is_null() || unsafe { (self.api().vtableOf)(object) } != self.api().symbol_vt {
            return None;
        }
        let name = unsafe { (self.api().symbolName)(object) };
        (!name.is_null()).then(|| unsafe { CStr::from_ptr(name) }.to_bytes())
    }

    pub(crate) fn send_slice(&self, receiver: Oop, selector: Oop, arguments: &[Oop]) -> Oop {
        unsafe { (self.api().send)(receiver, selector, arguments.as_ptr(), arguments.len()) }
    }

    pub(crate) unsafe fn make_string_with_finalizer_active(
        &self,
        bytes: &[u8],
        _finalizer: Finalizer,
    ) -> Oop {
        unsafe { (birnam_abi::string::make)(bytes.as_ptr(), bytes.len()) }
    }

    pub(crate) fn make_string(&self, bytes: &[u8]) -> Oop {
        unsafe { (birnam_abi::string::make)(bytes.as_ptr(), bytes.len()) }
    }

    pub(crate) fn temporary_root_scope(&self) -> TemporaryRootScope {
        let start = TEMPORARY_ROOTS.with(|roots| roots.borrow().len());
        TemporaryRootScope { start }
    }

    pub(crate) fn push_temporary_root(&self, root: Oop) {
        if !root.is_null() {
            TEMPORARY_ROOTS.with(|roots| roots.borrow_mut().push(root));
        }
    }
}
