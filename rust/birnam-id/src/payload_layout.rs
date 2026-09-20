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
        Self { api, factory }
    }
    pub(crate) fn api(&self) -> *mut ApiV1 { self.api.as_ptr() }
    pub(crate) fn adopt(&self, carrier: OwnedV1) -> Owned {
        Owned { carrier, release: self.factory.release.unwrap() }
    }
    pub(crate) fn empty(&self) -> Owned { self.adopt(OwnedV1::UNSET) }
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
    fn drop(&mut self) { unsafe { self.factory.destroy.unwrap()(self.api.as_ptr()) }; }
}
