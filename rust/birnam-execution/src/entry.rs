//! Exact entry identity includes context; it is never encoded as a language Oop.
use birnam_abi::id_protocol::NativeEntry;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EntryKey(NativeEntry);
impl EntryKey {
    pub(crate) fn from_method(entry: NativeEntry) -> Self {
        Self(entry)
    }
    pub(crate) fn key(self) -> (usize, usize) {
        (
            self.0.method.map_or(0, |f| f as usize),
            self.0.context.addr(),
        )
    }
    pub(crate) fn to_method(self) -> NativeEntry {
        self.0
    }
}
