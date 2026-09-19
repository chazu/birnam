//! Per-runtime ownership of the explicitly prepared Namespace storage.
use crate::abi::Oop;
use birnam_abi::namespace::ApiV1;
use core::ffi::c_void;
use core::ptr::NonNull;

pub(crate) struct Environment {
    api: ApiV1,
    context: NonNull<c_void>,
}
impl Environment {
    pub(crate) fn new() -> Self {
        // Tests select the same implementation for each independent Runtime;
        // production only accepts the host's explicitly attached descriptor.
        #[cfg(test)]
        let api = unsafe { *birnam_namespace::birnam_namespace_provider_v1() };
        #[cfg(not(test))]
        let api = unsafe { birnam_abi::namespace::operations() };
        let context =
            NonNull::new(unsafe { api.create.unwrap()() }).expect("Namespace create returned null");
        Self { api, context }
    }
    pub(crate) fn export(&mut self, name: Oop, value: Oop) -> Oop {
        unsafe { self.api.bind.unwrap()(self.context.as_ptr(), name, value) }
    }
    pub(crate) fn import(&self, name: Oop) -> Oop {
        unsafe { self.api.find.unwrap()(self.context.as_ptr(), name) }
    }
    pub(crate) fn exported_name_of(&self, value: Oop) -> Oop {
        unsafe { self.api.reverse.unwrap()(self.context.as_ptr(), value) }
    }
    pub(crate) fn roots(&self) -> Vec<Oop> {
        // The restricted storage operations cannot reenter or collect. Copy both
        // halves before any mutation or callback can invalidate the borrowed view.
        let view = unsafe { self.api.bindings.unwrap()(self.context.as_ptr()) };
        let bindings = unsafe { birnam_abi::borrowed_slice(view.bindings, view.count) }
            .expect("invalid Namespace view");
        bindings
            .iter()
            .flat_map(|entry| [entry.name, entry.value])
            .collect()
    }
    pub(crate) fn count(&self) -> usize {
        unsafe { self.api.count.unwrap()(self.context.as_ptr()) }
    }
    pub(crate) fn name_at(&self, index: usize) -> Oop {
        unsafe { self.api.name_at.unwrap()(self.context.as_ptr(), index) }
    }
}
impl Drop for Environment {
    fn drop(&mut self) {
        unsafe { self.api.destroy.unwrap()(self.context.as_ptr()) };
    }
}
