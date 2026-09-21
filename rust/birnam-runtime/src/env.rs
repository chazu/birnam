//! Per-runtime ownership of the explicitly prepared Namespace storage.
use crate::abi::Oop;
use birnam_abi::namespace::ApiV8;
use core::ffi::c_void;
use core::ptr::NonNull;

pub(crate) struct Environment {
    api: ApiV8,
    native_owner: Option<birnam_abi::namespace_owner::OwnedV1>,
    context: NonNull<c_void>,
    enumeration: NonNull<birnam_abi::namespace_enumeration::ApiV1>,
    names: NonNull<birnam_abi::namespace_names::ApiV1>,
    presence: NonNull<birnam_abi::namespace_presence::ApiV1>,
    imports: NonNull<birnam_abi::namespace_import::ApiV1>,
    canonical_imports: birnam_abi::namespace_import::ApiV1,
    exports: NonNull<birnam_abi::namespace_export::ApiV1>,
    canonical_exports: birnam_abi::namespace_export::ApiV1,
    canonical_presence: birnam_abi::namespace_presence::ApiV1,
    canonical_names: birnam_abi::namespace_names::ApiV1,
}
impl Environment {
    pub(crate) fn new() -> Self {
        // Tests select the same implementation for each independent Runtime;
        // production only accepts the host's explicitly attached descriptor.
        #[cfg(test)]
        let api = unsafe { *birnam_namespace::namespace_provider() };
        #[cfg(not(test))]
        let api = unsafe { birnam_abi::namespace::operations() };
        let context =
            NonNull::new(unsafe { api.create.unwrap()() }).expect("Namespace create returned null");
        Self::with_storage(api, context, None)
    }
    /// Transfer the completed native namespace, preserving all early bindings.
    pub(crate) unsafe fn adopt(pointer: *const birnam_abi::namespace_owner::OwnedV1) -> Self {
        assert!(unsafe { birnam_abi::namespace_owner::valid(pointer) });
        let owner = unsafe { *pointer };
        Self::with_storage(unsafe { *owner.api }, NonNull::new(owner.storage).unwrap(), Some(owner))
    }
    fn with_storage(api: ApiV8, context: NonNull<c_void>, native_owner: Option<birnam_abi::namespace_owner::OwnedV1>) -> Self {
        let names = NonNull::new(unsafe {
            api.create_names.unwrap()(
                context.as_ptr(),
                api.reverse,
                &crate::public_api::namespace_name_access(),
            )
        })
        .expect("Namespace names creation failed");
        let canonical_names = unsafe { *names.as_ptr() };
        let enumeration = NonNull::new(unsafe {
            api.create_enumeration.unwrap()(context.as_ptr(), api.count, api.name_at, &crate::public_api::namespace_enumeration_access())
        }).expect("Namespace enumeration creation failed");
        let presence = NonNull::new(unsafe { api.create_presence.unwrap()(context.as_ptr(), api.find, api.lookup, &crate::public_api::namespace_presence_access()) }).expect("Namespace presence creation failed");
        let canonical_presence = unsafe { *presence.as_ptr() };
        let imports = NonNull::new(unsafe { api.create_import.unwrap()(context.as_ptr(), api.find, api.lookup, &crate::public_api::namespace_import_access()) }).expect("Namespace import creation failed");
        let canonical_imports = unsafe { *imports.as_ptr() };
        let exports = NonNull::new(unsafe { api.create_export.unwrap()(context.as_ptr(), api.bind, api.publish, &crate::public_api::namespace_export_access()) }).expect("Namespace export creation failed");
        let canonical_exports = unsafe { *exports.as_ptr() };
        Self {
            native_owner,
            exports, canonical_exports,
            imports, canonical_imports,
            presence, canonical_presence,
            api,
            context,
            names,
            canonical_names,
            enumeration,
        }
    }
    pub(crate) fn native_receiver(&self) -> Option<Oop> { self.native_owner.map(|owner| owner.receiver) }
    pub(crate) fn native_owner(&self) -> Option<birnam_abi::namespace_owner::OwnedV1> { self.native_owner }
    pub(crate) fn exports(&self) -> *mut birnam_abi::namespace_export::ApiV1 { self.exports.as_ptr() }
    pub(crate) fn canonical_exports(&self) -> birnam_abi::namespace_export::ApiV1 { self.canonical_exports }
    pub(crate) fn imports(&self) -> *mut birnam_abi::namespace_import::ApiV1 { self.imports.as_ptr() }
    pub(crate) fn canonical_imports(&self) -> birnam_abi::namespace_import::ApiV1 { self.canonical_imports }
    pub(crate) fn presence(&self) -> *mut birnam_abi::namespace_presence::ApiV1 { self.presence.as_ptr() }
    pub(crate) fn canonical_presence(&self) -> birnam_abi::namespace_presence::ApiV1 { self.canonical_presence }
    pub(crate) fn enumeration(&self) -> *const birnam_abi::namespace_enumeration::ApiV1 { self.enumeration.as_ptr() }
    pub(crate) fn names(&self) -> *mut birnam_abi::namespace_names::ApiV1 {
        self.names.as_ptr()
    }
    pub(crate) fn canonical_names(&self) -> birnam_abi::namespace_names::ApiV1 {
        self.canonical_names
    }
    /// Copy selection before the primitives mutably borrow RuntimeState.
    pub(crate) fn publication(&self) -> (birnam_abi::namespace::PublishV1, *mut c_void, birnam_abi::namespace::BindV1) {
        (self.api.publish.unwrap(), self.context.as_ptr(), self.api.bind.unwrap())
    }
    /// Copy selection before the interner mutably borrows RuntimeState.
    pub(crate) fn lookup(&self) -> (birnam_abi::namespace::LookupV1, *const c_void, birnam_abi::namespace::FindV1) {
        (self.api.lookup.unwrap(), self.context.as_ptr(), self.api.find.unwrap())
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
    #[cfg(test)]
    pub(crate) fn count(&self) -> usize {
        unsafe { self.api.count.unwrap()(self.context.as_ptr()) }
    }
    pub(crate) fn name_at(&self, index: usize) -> Oop {
        unsafe { self.api.name_at.unwrap()(self.context.as_ptr(), index) }
    }
}
impl Drop for Environment {
    fn drop(&mut self) {
        unsafe { self.api.destroy_export.unwrap()(self.exports.as_ptr()) };
        unsafe { self.api.destroy_import.unwrap()(self.imports.as_ptr()) };
        unsafe { self.api.destroy_presence.unwrap()(self.presence.as_ptr()) };
        unsafe { self.api.destroy_enumeration.unwrap()(self.enumeration.as_ptr()) };
        unsafe { self.api.destroy_names.unwrap()(self.names.as_ptr()) };
        if let Some(owner) = self.native_owner {
            unsafe { owner.destroy.unwrap()(owner.context) };
        } else {
            unsafe { self.api.destroy.unwrap()(self.context.as_ptr()) };
        }
    }
}
