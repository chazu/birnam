//! Owned payload descriptions release their storage on drop.
use birnam_abi::payload_layout::{OwnedV1, ReleaseV1, ViewV1};

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
