//! Remaining legacy compiler ownership bridge, removed with its TCC producers
//! in batch 2. Language Method values, installation, lookup adaptation and
//! invocation now belong solely to birnam-execution.
use super::*;
use crate::abi::id_protocol::NativeEntry;
use core::ffi::c_void;

#[derive(Default)]
pub(super) struct Methods {
    owners: crate::raw::AddressMap<Oop>,
    turn_owners: crate::raw::AddressMap<Oop>,
}
impl Runtime {
    pub(crate) fn register_method_owner(&self, function: *const c_void, owner: Oop) -> bool {
        self.assert_owner();
        if function.is_null()
            || self.payload_size(owner).is_none()
            || unsafe { (&*self.state.get()).vtables.lifetime_owner() }.is_none()
        {
            return false;
        }
        let mut methods = self.methods.borrow_mut();
        if methods
            .owners
            .get(&function.addr())
            .is_some_and(|previous| *previous != owner)
        {
            return false;
        }
        methods.owners.insert(function.addr(), owner);
        true
    }
    pub(crate) fn forget_method_owner(&self, owner: Oop) {
        self.assert_owner();
        self.methods
            .borrow_mut()
            .owners
            .retain(|_, value| *value != owner);
    }
    fn method_owner(&self, entry: NativeEntry) -> Oop {
        crate::legacy_method::direct_method(entry)
            .and_then(|method| {
                self.methods
                    .borrow()
                    .owners
                    .get(&method.as_oop().addr())
                    .copied()
            })
            .unwrap_or(core::ptr::null_mut())
    }
    pub(crate) fn update_method_owner_edge(
        &self,
        behavior: Oop,
        selector: Oop,
        previous: Option<NativeEntry>,
        current: Option<NativeEntry>,
    ) {
        let old = previous.map_or(core::ptr::null_mut(), |entry| self.method_owner(entry));
        let new = current.map_or(core::ptr::null_mut(), |entry| self.method_owner(entry));
        if old == new {
            return;
        }
        if !old.is_null() && unsafe { (&*self.state.get()).execution.has_managed_turn() } {
            self.methods
                .borrow_mut()
                .turn_owners
                .insert(old.addr(), old);
        }
        let owner = unsafe { (&*self.state.get()).vtables.lifetime_owner() }
            .expect("generated native methods require Lifetime");
        unsafe { (owner.set_edge)(owner.context, behavior, selector.addr(), new) };
    }
    pub(crate) fn trace_method_turn_owners(&self, mark: MarkFn) {
        let owners: Vec<_> = self
            .methods
            .borrow()
            .turn_owners
            .values()
            .copied()
            .collect();
        for owner in owners {
            unsafe { mark(owner) };
        }
    }
    pub(crate) fn release_method_turn_owners(&self) {
        self.methods.borrow_mut().turn_owners.clear();
    }
}
