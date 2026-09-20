//! Ownership of upper collection settings; Id retains only execution facts.
use birnam_abi::gc_settings::{ApiV1, FactoryV1};
use core::ptr::NonNull;

pub(crate) struct Service {
    api: NonNull<ApiV1>,
    factory: FactoryV1,
}
impl Service {
    pub(crate) fn new() -> Self {
        #[cfg(test)]
        let factory = unsafe { *birnam_gc_settings::birnam_gc_settings_factory_v1() };
        #[cfg(not(test))]
        let factory = unsafe { birnam_abi::gc_settings::operations() };
        let floor = crate::public_api::gc_settings_access();
        let api = NonNull::new(unsafe { factory.create.unwrap()(&floor) })
            .expect("GC settings creation failed");
        Self { api, factory }
    }
    pub(crate) fn api(&self) -> *mut ApiV1 { self.api.as_ptr() }
    /// Runtime has already performed owner/observation admission.
    pub(crate) fn set_auto(&self, enabled: bool) {
        unsafe { self.factory.set_auto.unwrap()(self.api.as_ptr(), i32::from(enabled)) };
    }
    /// Runtime has already performed owner/observation admission.
    pub(crate) fn set_precise(&self, enabled: bool) {
        unsafe { self.factory.set_precise.unwrap()(self.api.as_ptr(), i32::from(enabled)) };
    }
    /// Callback-free query at the existing allocation/budget decision point.
    pub(crate) fn automatic_due(&self, stack_ready: bool, allocated: usize) -> bool {
        unsafe { self.factory.automatic_due.unwrap()(self.api.as_ptr(), i32::from(stack_ready), allocated) != 0 }
    }
    /// Callback-free query, also valid during restricted storage observation.
    pub(crate) fn precise(&self) -> bool {
        unsafe { self.factory.precise.unwrap()(self.api.as_ptr()) != 0 }
    }
}
impl Drop for Service {
    fn drop(&mut self) { unsafe { self.factory.destroy.unwrap()(self.api.as_ptr()) }; }
}
