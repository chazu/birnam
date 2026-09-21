//! Lifetime attachment of upper descriptions to exact floor-owned records.
use birnam_abi::payload_layout::{ApiV1, FactoryV1, OwnedV1, ReleaseV1, ViewV1};
use core::ptr::NonNull;

#[cfg(test)]
thread_local! {
    pub(crate) static TEST_FACTORY: core::cell::Cell<Option<FactoryV1>> = const { core::cell::Cell::new(None) };
}

pub(crate) struct Owned {
    carrier: OwnedV1,
    release: ReleaseV1,
}
impl Owned {
    pub(crate) fn view(&self) -> ViewV1 { self.carrier.view }
    pub(crate) fn into_carrier(mut self) -> OwnedV1 { core::mem::replace(&mut self.carrier, OwnedV1::UNSET) }
}
impl Drop for Owned {
    fn drop(&mut self) {
        let owned = core::mem::replace(&mut self.carrier, OwnedV1::UNSET);
        unsafe { (self.release)(owned) };
    }
}

pub(crate) struct Service {
    api: NonNull<ApiV1>,
    factory: FactoryV1,
    native: Option<crate::lifetime::Shared>,
    records: crate::raw::AddressMap<Owned>,
}
impl Service {
    pub(crate) fn new() -> Self {
        #[cfg(test)]
        let factory = TEST_FACTORY.get().unwrap_or_else(|| unsafe {
            *birnam_payload_layout::birnam_payload_layout_factory_v1()
        });
        #[cfg(not(test))]
        let factory = unsafe { birnam_abi::payload_layout::operations() };
        let floor = crate::public_api::payload_layout_access();
        let api = NonNull::new(unsafe { factory.create.unwrap()(&floor) })
            .expect("payload-layout capability creation failed");
        Self { api, factory, native: None, records: Default::default() }
    }
    pub(crate) fn from_lifetime(owner: crate::lifetime::Shared) -> Self {
        Self { api: NonNull::new(owner.0.layouts).unwrap(), factory: unsafe { *owner.0.layout_factory }, native: Some(owner), records: Default::default() }
    }
    pub(crate) fn native_owner(&self) -> Option<birnam_abi::lifetime::OwnedV2> { self.native.as_ref().map(|owner| owner.0) }
    pub(crate) fn view(&self, object: crate::abi::Oop) -> ViewV1 {
        if let Some(owner) = &self.native { unsafe { (owner.0.layout)(owner.0.context, object) } }
        else { self.records.get(&object.addr()).map_or(ViewV1::UNSET, Owned::view) }
    }
    pub(crate) fn replace(&mut self, object: crate::abi::Oop, owned: OwnedV1) {
        if let Some(owner) = &self.native { unsafe { (owner.0.replace_layout)(owner.0.context, object, owned) }; }
        else { let owned = self.adopt(owned); self.records.insert(object.addr(), owned); }
    }
    pub(crate) fn retire(&mut self, addresses: &[usize]) {
        if let Some(owner) = &self.native { unsafe { (owner.0.retire)(owner.0.context, addresses.as_ptr(), addresses.len()) }; }
        else { for address in addresses { self.records.remove(address); } }
    }
    pub(crate) fn api(&self) -> *mut ApiV1 { self.api.as_ptr() }
    pub(crate) fn adopt(&self, carrier: OwnedV1) -> Owned {
        Owned { carrier, release: self.factory.release.unwrap() }
    }
    pub(crate) fn copy(&self, kinds: *const u8, count: usize, rest: u8) -> Owned {
        self.adopt(unsafe { self.factory.copy.unwrap()(kinds, count, rest) })
    }
    pub(crate) fn slot_kind(&self, view: ViewV1, word: usize) -> u8 {
        unsafe { self.factory.slot_kind.unwrap()(view, word) }
    }
    pub(crate) fn has_layout(&self, view: ViewV1) -> bool {
        unsafe { self.factory.has_layout.unwrap()(view) != 0 }
    }
    pub(crate) fn all_oop(&self, view: ViewV1, words: usize) -> bool {
        unsafe { self.factory.all_oop.unwrap()(view, words) != 0 }
    }
}
impl Drop for Service {
    fn drop(&mut self) { self.records.clear(); if self.native.is_none() { unsafe { self.factory.destroy.unwrap()(self.api.as_ptr()) }; } }
}
