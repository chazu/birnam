//! Ownership of the upper native stack anchor; physical capture stays in Id.
use birnam_abi::native_stack::{ApiV1, FactoryV3};
use core::ptr::NonNull;

pub(crate) struct Service {
    api: NonNull<ApiV1>,
    factory: FactoryV3,
    native: Option<crate::collection_state::Shared>,
}
impl Service {
    pub(crate) fn from_collection(owner: crate::collection_state::Shared) -> Self {
        Self { api: NonNull::new(owner.0.stack).unwrap(), factory: unsafe { *owner.0.stack_factory }, native: Some(owner) }
    }
    pub(crate) fn new() -> Self {
        #[cfg(test)]
        let factory = unsafe { *birnam_native_stack::birnam_native_stack_factory_v3() };
        #[cfg(not(test))]
        let factory = unsafe { birnam_abi::native_stack::operations() };
        let floor = crate::public_api::native_stack_access();
        let api = NonNull::new(unsafe { factory.create.unwrap()(&floor) })
            .expect("native stack creation failed");
        Self { api, factory, native: None }
    }
    pub(crate) fn capture(&self) -> birnam_abi::native_stack::CaptureV1 { self.factory.capture.unwrap() }
    pub(crate) fn api(&self) -> *mut ApiV1 { self.api.as_ptr() }
    /// Runtime has already performed owner/observation admission.
    pub(crate) fn set(&self, address: usize) {
        unsafe { self.factory.set.unwrap()(self.api.as_ptr(), address) };
    }
    /// Callback-free scalar read at the existing readiness/capture point.
    pub(crate) fn get(&self) -> usize {
        unsafe { self.factory.get.unwrap()(self.api.as_ptr()) }
    }
}
impl Drop for Service {
    fn drop(&mut self) { if self.native.is_none() { unsafe { self.factory.destroy.unwrap()(self.api.as_ptr()) }; } }
}
