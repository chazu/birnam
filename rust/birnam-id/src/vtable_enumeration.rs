//! Own the Object Model observation service without exposing Id's registry.
use birnam_abi::vtable_enumeration::{ApiV1, FactoryV1};
use core::ptr::NonNull;

pub(crate) struct Service {
    api: NonNull<ApiV1>,
    factory: FactoryV1,
}
impl Service {
    pub(crate) fn new() -> Self {
        #[cfg(test)]
        let factory = unsafe { *birnam_object_model::enumeration::birnam_vtable_enumeration_factory_v1() };
        #[cfg(not(test))]
        let factory = unsafe { birnam_abi::vtable_enumeration::operations() };
        let floor = crate::public_api::vtable_enumeration_access();
        let api = NonNull::new(unsafe { factory.create.unwrap()(&floor) })
            .expect("vtable enumeration creation failed");
        Self { api, factory }
    }
    pub(crate) fn api(&self) -> *mut ApiV1 { self.api.as_ptr() }
}
impl Drop for Service {
    fn drop(&mut self) {
        unsafe { self.factory.destroy.unwrap()(self.api.as_ptr()) };
    }
}
