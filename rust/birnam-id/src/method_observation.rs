//! Ownership of the upper method observer; no callback state remains in Id.
use birnam_abi::method_observation::{ApiV1, FactoryV1, NotifyV1};
use core::ptr::NonNull;
pub(crate) struct Service {
    api: NonNull<ApiV1>,
    factory: FactoryV1,
}
impl Service {
    pub(crate) fn new() -> Self {
        #[cfg(test)]
        let factory = unsafe { *birnam_method_observation::birnam_method_observation_factory_v1() };
        #[cfg(not(test))]
        let factory = unsafe { birnam_abi::method_observation::operations() };
        let floor = crate::public_api::method_observation_access();
        let api = NonNull::new(unsafe { factory.create.unwrap()(&floor) })
            .expect("method observation creation failed");
        Self { api, factory }
    }
    pub(crate) fn api(&self) -> *mut ApiV1 { self.api.as_ptr() }
    pub(crate) fn notification(&self) -> (NotifyV1, *mut ApiV1) {
        (self.factory.notify.unwrap(), self.api.as_ptr())
    }
}
impl Drop for Service {
    fn drop(&mut self) { unsafe { self.factory.destroy.unwrap()(self.api.as_ptr()) }; }
}
