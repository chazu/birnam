//! A private registration key retains the Method reference and its allocation
//! identity. An address reused before checkpoint must not select an old entry.
use birnam_abi::Oop;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EntryKey {
    method: Oop,
    identity: usize,
}
impl EntryKey {
    // The message-facing owner obtains identity from Id while the Method is live.
    pub(crate) const fn new(method: Oop, identity: usize) -> Self {
        Self { method, identity }
    }
    pub(crate) fn key(self) -> usize { self.identity }
    pub(crate) fn to_method(self) -> Oop { self.method }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reused_address_does_not_reuse_registration_identity() {
        let address=core::ptr::without_provenance_mut(0x1000);
        let old=EntryKey::new(address,17);
        let new=EntryKey::new(address,18);
        assert_eq!(old.to_method(),new.to_method());
        assert_ne!(old.key(),new.key());
    }
}
