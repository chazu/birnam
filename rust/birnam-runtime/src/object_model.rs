//! Upper adoption of completed Object Model records; no production copy.
use crate::{abi::Oop, raw::AddressMap};
use birnam_abi::object_model::OwnedV1;
use std::rc::Rc;
pub(crate) struct Owner { pub(crate) native: OwnedV1 }
pub(crate) type Shared = Rc<Owner>;
impl Owner {
    pub(crate) unsafe fn adopt(pointer: *const OwnedV1) -> Shared {
        assert!(unsafe { birnam_abi::object_model::valid(pointer) });
        Rc::new(Self { native: unsafe { *pointer } })
    }
}
impl Drop for Owner { fn drop(&mut self) { unsafe { self.native.destroy.unwrap()(self.native.context) }; } }
pub(crate) enum Records {
    Local(AddressMap<Oop>),
    Native(Shared),
}
impl Records {
    pub(crate) fn new(owner: Option<Shared>) -> Self { owner.map(Self::Native).unwrap_or_else(|| Self::Local(AddressMap::default())) }
    pub(crate) fn get(&self, behavior: Oop) -> Oop {
        match self {
            Self::Local(records) => records.get(&behavior.addr()).copied().unwrap_or(core::ptr::null_mut()),
            Self::Native(owner) => unsafe { owner.native.owner.unwrap()(owner.native.context, behavior) },
        }
    }
    pub(crate) fn set(&mut self, behavior: Oop, object: Oop) {
        match self {
            Self::Local(records) => { if object.is_null() { records.remove(&behavior.addr()); } else { records.insert(behavior.addr(), object); } },
            Self::Native(owner) => unsafe { owner.native.set_owner.unwrap()(owner.native.context, behavior, object) },
        }
    }
    pub(crate) fn retire(&mut self, addresses: &[usize]) {
        match self {
            Self::Local(records) => { for address in addresses { records.remove(address); } },
            Self::Native(owner) => unsafe { owner.native.retire.unwrap()(owner.native.context, addresses.as_ptr(), addresses.len()) },
        }
    }
}
