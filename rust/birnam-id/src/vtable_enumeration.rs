//! Own the Object Model observation service without exposing Id's registry.
use birnam_abi::vtable_enumeration::{ApiV2, FactoryV2};
use core::ptr::NonNull;

pub(crate) struct Service {
    api: NonNull<ApiV2>,
    factory: FactoryV2,
    canonical: ApiV2,
}
impl Service {
    pub(crate) fn new() -> Self {
        #[cfg(test)]
        let factory = unsafe { *birnam_object_model::enumeration::birnam_vtable_enumeration_factory_v2() };
        #[cfg(not(test))]
        let factory = unsafe { birnam_abi::vtable_enumeration::operations() };
        let floor = crate::public_api::vtable_enumeration_access();
        let api = NonNull::new(unsafe { factory.create.unwrap()(&floor) })
            .expect("vtable enumeration creation failed");
        let canonical = unsafe { *api.as_ptr() };
        Self { api, factory, canonical }
    }
    pub(crate) fn api(&self) -> *mut ApiV2 { self.api.as_ptr() }
    pub(crate) fn canonical(&self) -> ApiV2 { self.canonical }
}
impl Drop for Service {
    fn drop(&mut self) {
        unsafe { self.factory.destroy.unwrap()(self.api.as_ptr()) };
    }
}
