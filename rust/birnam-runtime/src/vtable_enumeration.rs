//! Own the Object Model observation service without exposing Id's registry.
use birnam_abi::vtable_enumeration::{ApiV2, FactoryV2};
use core::ptr::NonNull;

pub(crate) struct Service {
    api: NonNull<ApiV2>,
    factory: Option<FactoryV2>,
    _model: Option<crate::object_model::Shared>,
    canonical: ApiV2,
}
impl Service {
    pub(crate) fn new() -> Self {
        #[cfg(test)]
        let factory = unsafe { *birnam_object_model::enumeration::vtable_enumeration_factory() };
        #[cfg(not(test))]
        let factory = unsafe { birnam_abi::vtable_enumeration::operations() };
        let floor = crate::public_api::vtable_enumeration_access();
        let api = NonNull::new(unsafe { factory.create.unwrap()(&floor) })
            .expect("vtable enumeration creation failed");
        let canonical = unsafe { *api.as_ptr() };
        Self { api, factory: Some(factory), _model: None, canonical }
    }
    pub(crate) fn from_model(owner: crate::object_model::Shared) -> Self {
        let api = NonNull::new(owner.native.enumeration).unwrap();
        Self { api, factory: None, canonical: unsafe { *api.as_ptr() }, _model: Some(owner) }
    }
    pub(crate) fn api(&self) -> *mut ApiV2 { self.api.as_ptr() }
    pub(crate) fn canonical(&self) -> ApiV2 { self.canonical }
}
impl Drop for Service {
    fn drop(&mut self) {
        if let Some(factory) = self.factory { unsafe { factory.destroy.unwrap()(self.api.as_ptr()) }; }
    }
}
