//! Ownership of the upper ordered root registry; no provider list remains in Id.
use crate::abi::RootProvider;
use crate::boundary;
use birnam_abi::root_registry::{ApiV1, CAPACITY, FactoryV1};
use core::ptr::NonNull;

pub(crate) struct Service {
    api: NonNull<ApiV1>,
    factory: FactoryV1,
    native: Option<birnam_abi::root_registry::OwnedV1>,
}
impl Service {
    pub(crate) fn new() -> Self {
        #[cfg(test)]
        let factory = unsafe { *birnam_root_registry::root_registry_factory() };
        #[cfg(not(test))]
        let factory = unsafe { birnam_abi::root_registry::operations() };
        let floor = crate::public_api::root_registry_access();
        let api = NonNull::new(unsafe { factory.create.unwrap()(&floor) })
            .expect("root registry creation failed");
        Self {
            api,
            factory,
            native: None,
        }
    }
    /// Adopt the complete owner initialized through Id, without preparing or
    /// replacing any callback. Its native admission already checks the thread.
    pub(crate) unsafe fn adopt(owned: *const birnam_abi::root_registry::OwnedV1) -> Self {
        assert!(unsafe { birnam_abi::root_registry::valid_owned(owned) });
        let owned = unsafe { *owned };
        Self {
            api: NonNull::new(owned.api).unwrap(),
            factory: unsafe { *owned.factory },
            native: Some(owned),
        }
    }
    pub(crate) fn retained_roots(&self) -> Vec<crate::abi::Oop> {
        let Some(owned) = self.native else {
            return Vec::new();
        };
        let view = unsafe { owned.roots.unwrap()(owned.context) };
        unsafe { crate::abi::borrowed_slice(view.entries, view.count) }
            .expect("invalid retained roots")
            .to_vec()
    }
    pub(crate) fn native_owner(&self) -> Option<birnam_abi::root_registry::OwnedV1> { self.native }
    pub(crate) fn api(&self) -> *mut ApiV1 {
        self.api.as_ptr()
    }
    /// Owner admission is performed by the Runtime, including independent
    /// bootstrap values. This immutable operation invokes no callbacks.
    pub(crate) fn register(&self, provider: RootProvider) {
        boundary::require_invariant(
            unsafe { self.factory.append.unwrap()(self.api.as_ptr(), provider) } == 1,
            "gc: too many root providers",
        );
    }
    /// Canonical callback-free snapshot, including inside storage observation.
    pub(crate) fn snapshot(&self) -> Vec<RootProvider> {
        let snapshot = unsafe { self.factory.snapshot.unwrap()(self.api.as_ptr()) };
        boundary::require_invariant(snapshot.count <= CAPACITY, "invalid root registry snapshot");
        snapshot.entries[..snapshot.count]
            .iter()
            .map(|entry| {
                entry.unwrap_or_else(|| boundary::fatal_invariant("null root registry provider"))
            })
            .collect()
    }
}

pub(crate) fn install_admission(owned: birnam_abi::root_registry::OwnedV1, runtime: &crate::runtime::Runtime) {
    unsafe extern "C" fn admit(context: *mut core::ffi::c_void) {
        unsafe { &*context.cast::<crate::runtime::Runtime>() }.admit_root_registration();
    }
    let mut floor = crate::public_api::root_registry_access();
    floor.context = core::ptr::from_ref(runtime).cast_mut().cast();
    floor.admit = Some(admit);
    boundary::require_invariant(unsafe { owned.set_admission.unwrap()(owned.context, &floor) } == 1, "root admission replacement failed");
}
impl Drop for Service {
    fn drop(&mut self) {
        unsafe { self.factory.destroy.unwrap()(self.api.as_ptr()) };
    }
}

/// Temporary roots for the aggregate Runtime constructor. These are ordinary
/// retained roots, released after the complete collector extension is installed.
pub(crate) struct Retained {
    owner: birnam_abi::root_registry::OwnedV1,
    tokens: Vec<usize>,
}
impl Retained {
    pub(crate) fn new(owner: birnam_abi::root_registry::OwnedV1, objects: &[crate::abi::Oop]) -> Self {
        let mut retained = Self { owner, tokens: Vec::new() };
        for object in objects { retained.retain(*object); }
        retained
    }
    pub(crate) fn retain(&mut self, object: crate::abi::Oop) {
        if !object.is_null() {
            let token = unsafe { self.owner.retain.unwrap()(self.owner.context, object) };
            assert_ne!(token, 0);
            self.tokens.push(token);
        }
    }
}
impl Drop for Retained {
    fn drop(&mut self) {
        for token in &self.tokens {
            assert_eq!(unsafe { self.owner.release.unwrap()(self.owner.context, *token) }, 1);
        }
    }
}
