//! Exact floor attachment and lifetime of upper trace-hook descriptions.
use birnam_abi::trace_hooks::{ApiV1, FactoryV1, SlotV1, SourceV1};
use crate::abi::{Oop, TraceHook};
use core::ptr::NonNull;

pub(crate) struct Service {
    api: NonNull<ApiV1>,
    factory: FactoryV1,
    native: Option<crate::lifetime::Shared>,
    records: crate::raw::AddressMap<SlotV1>,
}
impl Service {
    pub(crate) fn new() -> Self {
        #[cfg(test)]
        let factory = unsafe { *birnam_trace_hooks::trace_hooks_factory() };
        #[cfg(not(test))]
        let factory = unsafe { birnam_abi::trace_hooks::operations() };
        let floor = crate::public_api::trace_hooks_access();
        let api = NonNull::new(unsafe { factory.create.unwrap()(&floor) })
            .expect("trace-hooks capability creation failed");
        Self { api, factory, native: None, records: Default::default() }
    }
    pub(crate) fn from_lifetime(owner: crate::lifetime::Shared) -> Self {
        Self { api: NonNull::new(owner.0.hooks).unwrap(), factory: unsafe { *owner.0.hook_factory }, native: Some(owner), records: Default::default() }
    }
    pub(crate) fn get(&self, object: Oop) -> SlotV1 {
        if let Some(owner) = &self.native { unsafe { (owner.0.hook)(owner.0.context, object) } }
        else { self.records.get(&object.addr()).copied().unwrap_or(SlotV1::EMPTY) }
    }
    pub(crate) fn replace(&mut self, object: Oop, slot: SlotV1) {
        if let Some(owner) = &self.native { unsafe { (owner.0.replace_hook)(owner.0.context, object, slot) }; }
        else { self.records.insert(object.addr(), slot); }
    }
    pub(crate) fn retire_local(&mut self, addresses: &[usize]) {
        for address in addresses { self.records.remove(address); }
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
    fn drop(&mut self) { if self.native.is_none() { unsafe { self.factory.destroy.unwrap()(self.api.as_ptr()) }; } }
}
