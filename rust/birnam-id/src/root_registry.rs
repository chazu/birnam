//! Ownership of the upper ordered root registry; no provider list remains in Id.
use birnam_abi::root_registry::{ApiV1, CAPACITY, FactoryV1};
use crate::abi::RootProvider;
use crate::boundary;
use core::ptr::NonNull;

pub(crate) struct Service {
    api: NonNull<ApiV1>,
    factory: FactoryV1,
}
impl Service {
    pub(crate) fn new() -> Self {
        #[cfg(test)]
        let factory = unsafe { *birnam_root_registry::birnam_root_registry_factory_v1() };
        #[cfg(not(test))]
        let factory = unsafe { birnam_abi::root_registry::operations() };
        let floor = crate::public_api::root_registry_access();
        let api = NonNull::new(unsafe { factory.create.unwrap()(&floor) })
            .expect("root registry creation failed");
        Self { api, factory }
    }
    pub(crate) fn api(&self) -> *mut ApiV1 { self.api.as_ptr() }
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
        snapshot.entries[..snapshot.count].iter().map(|entry| {
            entry.unwrap_or_else(|| boundary::fatal_invariant("null root registry provider"))
        }).collect()
    }
}
impl Drop for Service {
    fn drop(&mut self) { unsafe { self.factory.destroy.unwrap()(self.api.as_ptr()) }; }
}
