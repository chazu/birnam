//! Production marks live in Collect. The local map supports legacy initialization.
use super::{BlockLifetime, Permanence};
use crate::{abi::Oop, boundary, collection_state::Shared, raw::AddressMap};
use birnam_abi::collection_state::{MISSING, RecordsV1};

pub(super) enum Records {
    Legacy(AddressMap<BlockLifetime>),
    Native(Shared),
}
impl Records {
    pub(super) fn new(owner: Option<Shared>) -> Self {
        owner
            .map(Self::Native)
            .unwrap_or_else(|| Self::Legacy(Default::default()))
    }
    fn api(owner: &Shared) -> RecordsV1 {
        unsafe { *owner.0.records }
    }
    pub(super) fn get(&self, address: &usize) -> Option<BlockLifetime> {
        match self {
            Self::Legacy(records) => records.get(address).copied(),
            Self::Native(owner) => {
                let api = Self::api(owner);
                let flags = unsafe { (api.get)(api.context, *address) };
                (flags != MISSING).then_some(BlockLifetime {
                    metadata: u64::from(flags),
                })
            }
        }
    }
    pub(super) fn insert(&mut self, object: Oop, record: BlockLifetime) {
        let inserted = match self {
            Self::Legacy(records) => records.insert(object.addr(), record).is_none(),
            Self::Native(owner) => {
                let api = Self::api(owner);
                unsafe { (api.insert)(api.context, object, record.metadata as u32) == 1 }
            }
        };
        boundary::require_invariant(inserted, "allocator reused a live payload address");
    }
    pub(super) fn contains_key(&self, address: &usize) -> bool {
        self.get(address).is_some()
    }
    pub(super) fn retain(&mut self, address: usize) {
        match self {
            Self::Legacy(records) => {
                records
                    .get_mut(&address)
                    .expect("initial object is not allocated")
                    .metadata |= 2
            }
            Self::Native(owner) => {
                let api = Self::api(owner);
                boundary::require_invariant(
                    unsafe { (api.retain)(api.context, address) } == 1,
                    "initial object is not allocated",
                );
            }
        }
    }
    pub(super) fn mark(&mut self, address: usize) -> bool {
        match self {
            Self::Legacy(records) => {
                let Some(record) = records.get_mut(&address) else {
                    return false;
                };
                if record.permanence() == Permanence::Permanent || record.marked() {
                    return false;
                }
                record.set_marked(true);
                true
            }
            Self::Native(owner) => {
                let api = Self::api(owner);
                unsafe { (api.mark)(api.context, address) == 1 }
            }
        }
    }
    pub(super) fn clear(&mut self) {
        match self {
            Self::Legacy(records) => {
                for record in records.values_mut() {
                    record.set_marked(false);
                }
            }
            Self::Native(owner) => {
                let api = Self::api(owner);
                unsafe { (api.clear)(api.context) };
            }
        }
    }
    pub(super) fn remove_dead(&mut self, address: usize) -> bool {
        match self {
            Self::Legacy(records) => {
                if !records.get(&address).is_some_and(|record| {
                    record.permanence() == Permanence::Collectable && !record.marked()
                }) {
                    return false;
                }
                records.remove(&address);
                true
            }
            Self::Native(owner) => {
                let api = Self::api(owner);
                unsafe { (api.remove_dead)(api.context, address) == 1 }
            }
        }
    }
    pub(super) fn iter(&self) -> std::vec::IntoIter<(usize, BlockLifetime)> {
        let entries: Vec<_> = match self {
            Self::Legacy(records) => records
                .iter()
                .map(|(address, record)| (*address, *record))
                .collect(),
            Self::Native(owner) => {
                let api = Self::api(owner);
                let view = unsafe { (api.snapshot)(api.context) };
                unsafe { crate::abi::borrowed_slice(view.entries, view.count) }
                    .expect("invalid collection record snapshot")
                    .iter()
                    .map(|record| {
                        (
                            record.object.addr(),
                            BlockLifetime {
                                metadata: u64::from(record.flags),
                            },
                        )
                    })
                    .collect()
            }
        };
        entries.into_iter()
    }
    #[cfg(test)]
    pub(super) fn keys(&self) -> impl Iterator<Item = usize> {
        self.iter().map(|(address, _)| address)
    }
}
