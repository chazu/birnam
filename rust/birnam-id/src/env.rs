//! Per-runtime ownership of the explicitly prepared Namespace storage.
use crate::abi::Oop;
use birnam_abi::namespace::ApiV2;
use core::ffi::c_void;
use core::ptr::NonNull;

pub(crate) struct Environment {
    api: ApiV2,
    context: NonNull<c_void>,
    names: NonNull<birnam_abi::namespace_names::ApiV1>,
    canonical_names: birnam_abi::namespace_names::ApiV1,
}
impl Environment {
    pub(crate) fn new() -> Self {
        // Tests select the same implementation for each independent Runtime;
        // production only accepts the host's explicitly attached descriptor.
        #[cfg(test)]
        let api = unsafe { *birnam_namespace::birnam_namespace_provider_v2() };
        #[cfg(not(test))]
        let api = unsafe { birnam_abi::namespace::operations() };
        let context =
            NonNull::new(unsafe { api.create.unwrap()() }).expect("Namespace create returned null");
        let names = NonNull::new(unsafe {
            api.create_names.unwrap()(
                context.as_ptr(),
                api.reverse,
                &crate::public_api::namespace_name_access(),
            )
        })
        .expect("Namespace names creation failed");
        let canonical_names = unsafe { *names.as_ptr() };
        Self {
            api,
            context,
            names,
            canonical_names,
        }
    }
    pub(crate) fn names(&self) -> *mut birnam_abi::namespace_names::ApiV1 {
        self.names.as_ptr()
    }
    pub(crate) fn canonical_names(&self) -> birnam_abi::namespace_names::ApiV1 {
        self.canonical_names
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
        unsafe { self.api.destroy_names.unwrap()(self.names.as_ptr()) };
        unsafe { self.api.destroy.unwrap()(self.context.as_ptr()) };
    }
}
