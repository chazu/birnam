//! Exact floor attachment and lifetime of upper finalizer descriptions.
use birnam_abi::finalizers::{ApiV1, FactoryV1, SlotV1};
use crate::abi::Finalizer;
use crate::heap::Permanence;
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
        let factory = unsafe { *birnam_finalizers::finalizers_factory() };
        #[cfg(not(test))]
        let factory = unsafe { birnam_abi::finalizers::operations() };
        let floor = crate::public_api::finalizers_access();
        let api = NonNull::new(unsafe { factory.create.unwrap()(&floor) })
            .expect("finalizers capability creation failed");
        Self { api, factory, native: None, records: Default::default() }
    }
    pub(crate) fn from_lifetime(owner: crate::lifetime::Shared) -> Self {
        Self { api: NonNull::new(owner.0.finalizers).unwrap(), factory: unsafe { *owner.0.finalizer_factory }, native: Some(owner), records: Default::default() }
    }
    pub(crate) fn get(&self, object: crate::abi::Oop) -> SlotV1 {
        if let Some(owner) = &self.native { unsafe { (owner.0.finalizer)(owner.0.context, object) } }
        else { self.records.get(&object.addr()).copied().unwrap_or(SlotV1::EMPTY) }
    }
    pub(crate) fn replace(&mut self, object: crate::abi::Oop, slot: SlotV1) {
        if let Some(owner) = &self.native { unsafe { (owner.0.replace_finalizer)(owner.0.context, object, slot) }; }
        else { self.records.insert(object.addr(), slot); }
    }
    pub(crate) fn detach(&mut self, address: usize) -> SlotV1 {
        if let Some(owner) = &self.native {
            let result = unsafe { (owner.0.finalizer)(owner.0.context, core::ptr::without_provenance_mut(address)) };
            unsafe { (owner.0.replace_finalizer)(owner.0.context, core::ptr::without_provenance_mut(address), SlotV1::EMPTY) };
            result
        } else { self.records.remove(&address).unwrap_or(SlotV1::EMPTY) }
    }
    pub(crate) fn api(&self) -> *mut ApiV1 { self.api.as_ptr() }
    pub(crate) fn slot(&self, permanence: Permanence, finalizer: Finalizer) -> SlotV1 {
        unsafe { self.factory.slot.unwrap()(i32::from(permanence == Permanence::Permanent), finalizer) }
    }
}
impl Drop for Service {
    fn drop(&mut self) { if self.native.is_none() { unsafe { self.factory.destroy.unwrap()(self.api.as_ptr()) }; } }
}
