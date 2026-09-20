//! Exact floor attachment and lifetime of upper finalizer descriptions.
use birnam_abi::finalizers::{ApiV1, FactoryV1, SlotV1};
use crate::abi::Finalizer;
use crate::heap::Permanence;
use core::ptr::NonNull;

pub(crate) struct Service {
    api: NonNull<ApiV1>,
    factory: FactoryV1,
}
impl Service {
    pub(crate) fn new() -> Self {
        #[cfg(test)]
        let factory = unsafe { *birnam_finalizers::birnam_finalizers_factory_v1() };
        #[cfg(not(test))]
        let factory = unsafe { birnam_abi::finalizers::operations() };
        let floor = crate::public_api::finalizers_access();
        let api = NonNull::new(unsafe { factory.create.unwrap()(&floor) })
            .expect("finalizers capability creation failed");
        Self { api, factory }
    }
    pub(crate) fn api(&self) -> *mut ApiV1 { self.api.as_ptr() }
    pub(crate) fn slot(&self, permanence: Permanence, finalizer: Finalizer) -> SlotV1 {
        unsafe { self.factory.slot.unwrap()(i32::from(permanence == Permanence::Permanent), finalizer) }
    }
}
impl Drop for Service {
    fn drop(&mut self) { unsafe { self.factory.destroy.unwrap()(self.api.as_ptr()) }; }
}
