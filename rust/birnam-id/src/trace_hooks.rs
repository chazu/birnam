//! Exact floor attachment and lifetime of upper trace-hook descriptions.
use birnam_abi::trace_hooks::{ApiV1, FactoryV1, SlotV1, SourceV1};
use crate::abi::{Oop, TraceHook};
use core::ptr::NonNull;

pub(crate) struct Service {
    api: NonNull<ApiV1>,
    factory: FactoryV1,
}
impl Service {
    pub(crate) fn new() -> Self {
        #[cfg(test)]
        let factory = unsafe { *birnam_trace_hooks::birnam_trace_hooks_factory_v1() };
        #[cfg(not(test))]
        let factory = unsafe { birnam_abi::trace_hooks::operations() };
        let floor = crate::public_api::trace_hooks_access();
        let api = NonNull::new(unsafe { factory.create.unwrap()(&floor) })
            .expect("trace-hooks capability creation failed");
        Self { api, factory }
    }
    pub(crate) fn api(&self) -> *mut ApiV1 { self.api.as_ptr() }
    pub(crate) fn slot(&self, hook: TraceHook) -> SlotV1 {
        unsafe { self.factory.slot.unwrap()(hook) }
    }
    pub(crate) fn resolve(&self, source: &SourceV1, vtable: Oop) -> Option<TraceHook> {
        unsafe { self.factory.resolve.unwrap()(source, vtable) }
    }
}
impl Drop for Service {
    fn drop(&mut self) { unsafe { self.factory.destroy.unwrap()(self.api.as_ptr()) }; }
}
