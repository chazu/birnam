//! Pending Execution integration: retain numeric admission and deferred batch
//! algorithms below. Production Integer has no table/mounting interface now.
//! Explicit Integer dependency of Machine and its representation consumers.
//! Id's bootstrap, Values and Heap do not own this family.
use super::*;
use birnam_abi::integer::{IntegerApiV1, IntegerStorageV1};

#[derive(Clone, Copy)]
pub(super) struct Binding {
    pointer: *mut IntegerApiV1,
    admitted: IntegerApiV1,
    storage: IntegerStorageV1,
    native: Option<birnam_abi::integer_native::ConstructionV1>,
    namespace_lookup: Option<NativeEntry>,
}

impl Runtime {
    pub(crate) unsafe fn attach_integer(&self, pointer: *mut IntegerApiV1) -> bool {
        self.assert_owner();
        if pointer.is_null() {
            return false;
        }
        if let Some(binding) = self.integer.get() {
            return binding.pointer == pointer;
        }
        let ops = unsafe { pointer.read() };
        if ops.version != 1
            || ops.byte_size != size_of::<IntegerApiV1>()
            || ops.context.is_null()
            || ops.behavior.is_null()
            || ops.prototype.is_null()
            || ops.storage.is_null()
        {
            return false;
        }
        let storage = unsafe { ops.storage.read() };
        if storage.version != 1
            || storage.byte_size != size_of::<IntegerStorageV1>()
            || storage.payload_size == 0
            || !storage.payload_alignment.is_power_of_two()
            || storage.payload_alignment > align_of::<Oop>()
        {
            return false;
        }
        if self.checked_vtable_of(ops.behavior) != self.globals().vtable_vt
            || self.checked_vtable_of(ops.prototype) != ops.behavior
            || !self
                .payload_size(ops.prototype)
                .is_some_and(|size| size >= storage.payload_size)
        {
            return false;
        }
        let native = crate::loader::integer_native_construction(&ops);
        let namespace_lookup = native.and_then(|facts| {
            let (owner, entry) = {
                let state = unsafe { &*self.state.get() };
                let owner = state.environment.native_owner()?;
                if facts.namespace != owner.receiver { return None; }
                let behavior = state.heap.vtable_of(owner.receiver)?;
                (owner, state.vtables.native_lookup(behavior, facts.binding)?)
            };
            crate::loader::namespace_lookup_declared(entry, &owner, &facts.id).then_some(entry)
        });
        self.integer.set(Some(Binding {
            pointer,
            admitted: ops,
            storage,
            native,
            namespace_lookup,
        }));
        true
    }

    pub(crate) fn integer_dependency_attached(&self) -> bool {
        self.assert_owner();
        self.integer.get().is_some()
    }

    fn integer_binding(&self) -> Binding {
        self.assert_owner();
        self.integer.get().expect("Integer dependency not mounted")
    }

    pub(crate) fn integer_behavior(&self) -> Oop {
        self.integer_binding().admitted.behavior
    }
    #[cfg(test)]
    pub(crate) fn integer_prototype(&self) -> Oop {
        self.integer_binding().admitted.prototype
    }
    pub(crate) fn integer_storage(&self) -> IntegerStorageV1 {
        self.integer_binding().storage
    }
    pub(crate) fn integer_operations(&self) -> IntegerApiV1 {
        self.integer_binding().admitted
    }

    pub(crate) fn integer_context_current(&self) -> bool {
        self.assert_owner();
        let Some(binding) = self.integer.get() else {
            return false;
        };
        let current = unsafe { binding.pointer.read() };
        if !current.same_contract(&binding.admitted) { return false; }
        let Some(facts) = binding.native else {
            return unsafe { (current.allocation_context)(current.context) }
                == crate::public_api::canonical_allocator();
        };
        if !self.guarded_default_lookup_eligible(current.behavior) { return false; }
        let state = unsafe { &*self.state.get() };
        let Some(id) = state.id_protocol.as_ref() else { return false; };
        let Some(roots) = state.root_registry.native_owner() else { return false; };
        let Some(primitives) = state.vtables.primitives() else { return false; };
        facts.id.same_contract(id)
            && facts.roots.context == roots.context
            && facts.roots.retain.map(|f| f as usize) == roots.retain.map(|f| f as usize)
            && facts.roots.release.map(|f| f as usize) == roots.release.map(|f| f as usize)
            && facts.allocation_size == primitives.allocation_size
            && facts.behavior_of == primitives.behavior_of
            && state.symbols.existing(b"allocate") == Some(facts.allocate)
            && state.vtables.native_lookup(state.globals.vtable_vt, facts.allocate)
                == Some(NativeEntry::new(super::id_protocol::allocate, core::ptr::from_ref(self).cast_mut().cast()))
    }

    pub(crate) fn integer_native_numeric_current(&self) -> bool {
        if !self.integer_context_current() { return false; }
        let binding = self.integer_binding();
        let Some(facts) = binding.native else { return false; };
        let Some(expected) = binding.namespace_lookup else { return false; };
        let state = unsafe { &*self.state.get() };
        let Some(behavior) = state.heap.vtable_of(facts.namespace) else { return false; };
        if !self.guarded_default_lookup_eligible(behavior) { return false; }
        let Some(symbol_bytes) = state.symbols.existing(b"symbolBytesOf:") else { return false; };
        let Some(primitives) = state.vtables.primitives() else { return false; };
        state.environment.native_receiver() == Some(facts.namespace)
            && state.symbols.existing(b"bindingAt:") == Some(facts.binding)
            && state.symbols.existing(b"ErrorVt") == Some(facts.error_behavior_name)
            && state.vtables.native_lookup(behavior, facts.binding) == Some(expected)
            && state.vtables.native_lookup(facts.id.receiver, facts.id.intern)
                == Some(NativeEntry::new(super::id_protocol::intern, core::ptr::from_ref(self).cast_mut().cast()))
            && state.vtables.native_lookup(facts.id.receiver, symbol_bytes)
                == Some(primitives.symbol_bytes)
    }
    #[cfg(feature = "ffi-test-hooks")]
    pub(crate) fn integer_live_pointer(&self) -> *mut IntegerApiV1 {
        self.integer_binding().pointer
    }

    pub(crate) fn make_integer(&self, value: i64) -> Oop {
        let binding = self.integer_binding();
        let ops = unsafe { binding.pointer.read() };
        // Callbacks may collect or reenter. No RuntimeState borrow survives.
        self.enter(|| unsafe { (ops.make)(ops.context, value) })
    }
    pub(crate) fn integer_value(&self, object: Oop) -> i64 {
        let storage = self.integer_storage();
        // Machine's internal reader retains its pure-payload contract, including
        // callers inside a state borrow. Ordinary native clients use live ops.
        unsafe { (storage.read)(object.cast()) }
    }
    pub(crate) fn is_integer(&self, object: Oop) -> bool {
        self.assert_owner();
        self.integer
            .get()
            .is_some_and(|binding| self.checked_vtable_of(object) == binding.admitted.behavior)
    }

    pub(crate) fn prepare_integer_batch(
        &self,
        values: &[i64],
    ) -> Option<std::rc::Rc<crate::heap::PayloadBatch>> {
        // This is native untraced storage. Finish encoding before publication;
        // later settlement borrows those bytes without calling any codec.
        let storage = self.integer_storage();
        let mut batch = crate::heap::PayloadBatch::zeroed(
            self.integer_behavior(),
            storage.payload_size,
            values.len(),
        )?;
        for (index, &value) in values.iter().enumerate() {
            unsafe { (storage.initialize)(batch.payload_mut(index).as_mut_ptr(), value) };
        }
        Some(std::rc::Rc::new(batch))
    }

    pub(crate) fn integer_index_batch(&self) -> std::rc::Rc<crate::heap::PayloadBatch> {
        let existing = unsafe {
            (&*self.state.get()).heap.retained_payload_indices(
                self.integer_behavior(),
                self.integer_storage().payload_size,
            )
        };
        if let Some(batch) = existing {
            return batch;
        }
        // The ledger remains the sole long-lived owner, including its original
        // retention on reclaim and release on settlement. No hidden cache.
        let values: Vec<_> = (0..128).map(i64::from).collect();
        self.prepare_integer_batch(&values)
            .expect("bounded index payloads")
    }

}
