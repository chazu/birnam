//! Single-thread-owned process state and short-borrow accessors.
#![allow(dead_code)] // R2 harness precedes public id_init wiring.

use core::cell::{Cell, UnsafeCell};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::abi::SLOT_OOP;
use crate::abi::{MarkFn, Method, MethodOverwriteHook, Oop};
use crate::boundary;
use crate::env::Environment;
use crate::execution::{
    CallSiteKind, DecodedAction, ExecutionError, ExecutionObjects, ExecutionState, FrameDescriptor,
    FrameViewError, GenerationLeaseCapability, MachineCapability, MachineError, MaterializedEntry,
    ReturnDestination, RuntimeExecutionDiagnostics, SemanticRegistrationV1, SlotKind,
    StacklessTarget,
};
use crate::gc::{
    Marker, MetadataShell, invoke_with_mark_callback, mark_native_root_range,
    with_native_root_range,
};
use crate::heap::{Heap, Permanence};
use crate::raw::MethodAddress;
use crate::symbol::{EmbeddedNul, Symbols};
use crate::values::Values;
use crate::vtable::{AddMethod, Vtables};

static FRAME_VIEW_LAYOUT: [u8; 1] = [SLOT_OOP];

#[derive(Clone, Copy)]
pub(crate) struct DispatchGlobals {
    pub(crate) vtable_vt: Oop,
    pub(crate) object_vt: Oop,
    pub(crate) symbol_vt: Oop,
    pub(crate) lookup: Oop,
    pub(crate) does_not_understand: Oop,
}

/// Identities fixed by the one-way bootstrap transition.
#[derive(Clone, Copy)]
pub(crate) struct CanonicalObjects {
    pub(crate) symbol_list: Oop,
    pub(crate) environment: Oop,
    pub(crate) smallint_vt: Oop,
    pub(crate) string_vt: Oop,
    pub(crate) float_vt: Oop,
    pub(crate) word_vt: Oop,
    pub(crate) nil_vt: Oop,
    pub(crate) bool_vt: Oop,
    pub(crate) nil: Oop,
    pub(crate) true_object: Oop,
    pub(crate) false_object: Oop,
    pub(crate) object_prototype: Oop,
    pub(crate) integer_prototype: Oop,
    pub(crate) float_prototype: Oop,
    pub(crate) word_prototype: Oop,
    pub(crate) string_prototype: Oop,
    pub(crate) symbol_prototype: Oop,
}

struct RuntimeState {
    // Ordered authorities are dropped before the raw heap owner at harness
    // teardown. No destructor dereferences a heap payload.
    symbols: Symbols,
    values: Values,
    vtables: Vtables,
    heap: Heap,
    globals: DispatchGlobals,
    canonical: Option<CanonicalObjects>,
    internal_dictionaries: Option<(Oop, Oop)>,
    metadata: MetadataShell,
    environment: Environment,
    method_overwrite_hook: Option<MethodOverwriteHook>,
    execution: ExecutionState,
}

/// One runtime owned by the thread that constructed it.
pub(crate) struct Runtime {
    owner: u64,
    collection_phase: Cell<CollectionPhase>,
    state: UnsafeCell<RuntimeState>,
    entry_scopes: UnsafeCell<Vec<MachineCapability>>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum CollectionPhase {
    Idle,
    Marking,
    Finalizing,
}

struct CollectionGuard<'a> {
    phase: &'a Cell<CollectionPhase>,
}

struct StacklessDriveRootGuard<'a> {
    runtime: &'a Runtime,
    machine: Oop,
}

struct StacklessEntryScopeGuard<'a> {
    runtime: &'a Runtime,
    capability: MachineCapability,
}

struct StacklessEntryScopeSuppressionGuard<'a> {
    runtime: &'a Runtime,
    suppressed: Vec<MachineCapability>,
}

struct StacklessBoundaryGuard<'a> {
    runtime: &'a Runtime,
    capability: MachineCapability,
    owner: u64,
}

pub(crate) struct DispatchReadGuard<'a> {
    runtime: &'a Runtime,
}

impl Drop for StacklessDriveRootGuard<'_> {
    fn drop(&mut self) {
        self.runtime.pop_stackless_scoped_root(self.machine);
    }
}

impl Drop for StacklessEntryScopeGuard<'_> {
    fn drop(&mut self) {
        // SAFETY: entry scopes are owner-thread-only and unwind in LIFO order.
        let popped = unsafe { (&mut *self.runtime.entry_scopes.get()).pop() }
            .expect("stackless entry scope underflow");
        assert_eq!(popped, self.capability, "entry scopes must unwind LIFO");
    }
}

impl Drop for StacklessEntryScopeSuppressionGuard<'_> {
    fn drop(&mut self) {
        // SAFETY: suppression is owner-thread-only. Any fresh drive entered in
        // the dynamic extent must have unwound its own scopes before return.
        let current = unsafe { &mut *self.runtime.entry_scopes.get() };
        assert!(
            current.is_empty(),
            "suppressed entry-scope extent leaked a nested scope"
        );
        *current = core::mem::take(&mut self.suppressed);
    }
}

impl Drop for StacklessBoundaryGuard<'_> {
    fn drop(&mut self) {
        // SAFETY: the synchronous activation has returned; no callback or
        // execution-state borrow survives this point transition.
        unsafe {
            let state = &mut *self.runtime.state.get();
            if let Ok(machine) = state.execution.machine_mut(self.owner, self.capability) {
                let _ = machine.leave_synchronous_boundary(self.owner);
            }
        }
    }
}

impl Drop for DispatchReadGuard<'_> {
    fn drop(&mut self) {
        // SAFETY: owner-thread read tokens are scalar state and do not retain a
        // borrow across lookup or invocation.
        unsafe {
            (&mut *self.runtime.state.get())
                .execution
                .end_dispatch_read()
                .unwrap_or_else(|_| boundary::fatal_invariant("dispatch read token underflow"));
        }
    }
}

impl Drop for CollectionGuard<'_> {
    fn drop(&mut self) {
        self.phase.set(CollectionPhase::Idle);
    }
}

impl Runtime {
    pub(crate) fn enter_dispatch_read(&self) -> DispatchReadGuard<'_> {
        self.assert_owner();
        // SAFETY: owner-thread scalar transition; returned guard owns no state
        // borrow and may span metacircular lookup and immediate invocation.
        unsafe {
            (&mut *self.state.get())
                .execution
                .begin_dispatch_read()
                .unwrap_or_else(|_| boundary::fatal_invariant("dispatch read depth overflow"));
        }
        DispatchReadGuard { runtime: self }
    }

    pub(crate) fn new(
        heap: Heap,
        vtables: Vtables,
        symbols: Symbols,
        globals: DispatchGlobals,
    ) -> Self {
        Self::new_with_values(heap, vtables, symbols, Values::new(), globals)
    }

    pub(crate) fn new_with_values(
        heap: Heap,
        vtables: Vtables,
        symbols: Symbols,
        values: Values,
        globals: DispatchGlobals,
    ) -> Self {
        Self {
            owner: current_thread_token(),
            collection_phase: Cell::new(CollectionPhase::Idle),
            entry_scopes: UnsafeCell::new(Vec::new()),
            state: UnsafeCell::new(RuntimeState {
                symbols,
                values,
                vtables,
                heap,
                globals,
                canonical: None,
                internal_dictionaries: None,
                metadata: MetadataShell::new(),
                environment: Environment::new(),
                method_overwrite_hook: None,
                execution: ExecutionState::new(current_thread_token()),
            }),
        }
    }

    fn assert_owner(&self) {
        boundary::require_invariant(
            current_thread_token() == self.owner,
            "Rust floor entered from a non-owner thread",
        );
    }

    fn require_allocation_allowed(&self) {
        boundary::require_invariant(
            self.collection_phase.get() == CollectionPhase::Idle,
            "id allocation attempted during collection",
        );
    }

    fn maybe_collect_before_allocation(&self) {
        // Copy the decision before collection enters foreign root callbacks.
        // SAFETY: owner-thread serialization and no callback in this scope.
        let due = unsafe {
            let state = &*self.state.get();
            state
                .metadata
                .automatic_collection_due(state.heap.bytes_since_collection())
        };
        if due {
            let _ = self.gc_collect();
        }
    }

    pub(crate) fn globals(&self) -> DispatchGlobals {
        self.assert_owner();
        // SAFETY: owner-thread serialization; copy ends the shared access here.
        unsafe { (*self.state.get()).globals }
    }

    pub(crate) fn lookup_method(&self, vtable: Oop, selector: Oop) -> Option<MethodAddress> {
        self.assert_owner();
        // SAFETY: owner-thread serialization; MethodAddress is copied out and
        // the collection borrow ends before any caller can invoke it.
        unsafe { (*self.state.get()).vtables.lookup(vtable, selector) }
    }

    /// Lookup after `enter` has already established owner-thread authority.
    pub(crate) unsafe fn lookup_method_active(
        &self,
        vtable: Oop,
        selector: Oop,
    ) -> Option<MethodAddress> {
        // SAFETY: the active-runtime guard serializes access to RuntimeState.
        unsafe { (*self.state.get()).vtables.lookup(vtable, selector) }
    }

    pub(crate) fn add_method(&self, vtable: Oop, selector: Oop, method: Method) -> AddMethod {
        self.assert_owner();
        // Copy all callback inputs and end the state borrow before foreign code.
        // SAFETY: owner-thread serialization; direct_lookup and hook are copied.
        let hook = unsafe {
            let state = &*self.state.get();
            state
                .vtables
                .direct_lookup(vtable, selector)
                .and(state.method_overwrite_hook)
        };
        if let Some(hook) = hook {
            // No Rust state borrow survives this callback. The ABI contract
            // permits only pure Store-log mutation and read-only floor queries.
            unsafe { hook(vtable, selector) };
        }
        // SAFETY: callback returned and owner-thread serialization is restored.
        let result = unsafe {
            (&mut *self.state.get())
                .vtables
                .add_method(vtable, selector, method)
        };
        let added = MethodAddress::from_method(method);
        // Direct-binding accounting follows the mutation result so aliases own
        // independent references and inherited lookup adds none.
        unsafe {
            let execution = &mut (&mut *self.state.get()).execution;
            if let AddMethod::Overwrote(previous) = result
                && previous != added
            {
                execution
                    .note_direct_binding_removed(previous)
                    .unwrap_or_else(|_| {
                        boundary::fatal_invariant("resumable binding removal accounting failed")
                    });
            }
            if !matches!(result, AddMethod::Overwrote(previous) if previous == added) {
                execution
                    .note_direct_binding_added(added)
                    .unwrap_or_else(|_| {
                        boundary::fatal_invariant("resumable binding addition accounting failed")
                    });
            }
        }
        result
    }

    pub(crate) fn remove_method(&self, vtable: Oop, selector: Oop) -> Option<MethodAddress> {
        self.assert_owner();
        // Copy all callback inputs and end the state borrow before foreign code.
        // SAFETY: owner-thread serialization; direct_lookup and hook are copied.
        let hook = unsafe {
            let state = &*self.state.get();
            state
                .vtables
                .direct_lookup(vtable, selector)
                .and(state.method_overwrite_hook)
        };
        if let Some(hook) = hook {
            // No Rust state borrow survives this callback. The ABI contract
            // permits only pure Store-log mutation and read-only floor queries.
            unsafe { hook(vtable, selector) };
        }
        // SAFETY: callback returned and owner-thread serialization is restored.
        let removed = unsafe {
            (&mut *self.state.get())
                .vtables
                .remove_method(vtable, selector)
        };
        if let Some(method) = removed {
            // SAFETY: owner-thread point accounting after dictionary mutation.
            unsafe {
                (&mut *self.state.get())
                    .execution
                    .note_direct_binding_removed(method)
                    .unwrap_or_else(|_| {
                        boundary::fatal_invariant("resumable binding removal accounting failed")
                    });
            }
        }
        removed
    }

    pub(crate) fn set_method_overwrite_hook(&self, hook: Option<MethodOverwriteHook>) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        unsafe { (&mut *self.state.get()).method_overwrite_hook = hook };
    }

    pub(crate) fn create_vtable(&self, parent: Oop) -> Oop {
        self.assert_owner();
        self.require_allocation_allowed();
        self.maybe_collect_before_allocation();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        state
            .vtables
            .create(&mut state.heap, parent, state.globals.vtable_vt)
    }

    pub(crate) fn set_vtable_owner(&self, vtable: Oop, owner: Oop) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        unsafe { (&mut *self.state.get()).vtables.set_owner(vtable, owner) };
    }

    pub(crate) fn set_vtable_parent(&self, vtable: Oop, parent: Oop) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        unsafe { (&mut *self.state.get()).vtables.set_parent(vtable, parent) };
    }

    pub(crate) fn allocate_instance(&self, vtable: Oop, payload_size: usize) -> Oop {
        self.assert_owner();
        // SAFETY: assert_owner established the same authority as Runtime::enter.
        unsafe { self.allocate_instance_active(vtable, payload_size) }
    }

    /// Allocate an ordinary instance after `enter` has established owner-thread
    /// authority for this runtime.
    ///
    /// # Safety
    ///
    /// The caller must be executing within this exact runtime's active dynamic
    /// extent. Collection-phase and automatic-collection checks remain active.
    pub(crate) unsafe fn allocate_instance_active(&self, vtable: Oop, payload_size: usize) -> Oop {
        self.require_allocation_allowed();
        self.maybe_collect_before_allocation();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        let object = state.heap.allocate(payload_size, Permanence::Collectable);
        boundary::require_invariant(
            state.heap.set_vtable(object, vtable),
            "new instance block was not registered",
        );
        object
    }

    pub(crate) fn clone_object(&self, object: Oop) -> Oop {
        if object.is_null() {
            return object;
        }
        self.assert_owner();
        self.require_allocation_allowed();
        self.maybe_collect_before_allocation();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        unsafe { (&mut *self.state.get()).heap.clone_collectable(object) }
            .unwrap_or_else(|| boundary::fatal_invariant("clone receiver is not a floor object"))
    }

    pub(crate) fn ensure_private_vtable(&self, object: Oop) -> Oop {
        self.assert_owner();
        self.require_allocation_allowed();
        self.maybe_collect_before_allocation();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        let current = Vtables::checked_vtable_of(&state.heap, object);
        boundary::require_invariant(
            !current.is_null(),
            "ensurePrivateVtable receiver is not a floor object",
        );
        if current == state.globals.vtable_vt {
            return object;
        }
        if state.vtables.owner_of(current) == object {
            return current;
        }
        let private = state
            .vtables
            .create(&mut state.heap, current, state.globals.vtable_vt);
        state.vtables.set_owner(private, object);
        boundary::require_invariant(
            state.heap.set_vtable(object, private),
            "private-vtable receiver block was not registered",
        );
        private
    }

    pub(crate) fn allocate_empty(&self, vtable: Oop, permanence: Permanence) -> Oop {
        self.assert_owner();
        self.require_allocation_allowed();
        self.maybe_collect_before_allocation();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        state
            .values
            .allocate_empty(&mut state.heap, vtable, permanence)
    }

    pub(crate) fn allocate_integer(&self, vtable: Oop, value: i64) -> Oop {
        self.assert_owner();
        self.require_allocation_allowed();
        self.maybe_collect_before_allocation();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        state
            .values
            .allocate_integer(&mut state.heap, vtable, value)
    }

    pub(crate) fn make_integer(&self, value: i64) -> Oop {
        self.assert_owner();
        self.require_allocation_allowed();
        self.maybe_collect_before_allocation();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        let vtable = state
            .canonical
            .unwrap_or_else(|| boundary::fatal_invariant("canonical objects not installed"))
            .smallint_vt;
        state
            .values
            .allocate_integer(&mut state.heap, vtable, value)
    }

    pub(crate) fn allocate_float(&self, vtable: Oop, value: f64) -> Oop {
        self.assert_owner();
        self.require_allocation_allowed();
        self.maybe_collect_before_allocation();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        state.values.allocate_float(&mut state.heap, vtable, value)
    }

    pub(crate) fn make_float(&self, value: f64) -> Oop {
        self.assert_owner();
        self.require_allocation_allowed();
        self.maybe_collect_before_allocation();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        let vtable = state
            .canonical
            .unwrap_or_else(|| boundary::fatal_invariant("canonical objects not installed"))
            .float_vt;
        state.values.allocate_float(&mut state.heap, vtable, value)
    }

    pub(crate) fn allocate_word(&self, vtable: Oop, value: u64) -> Oop {
        self.assert_owner();
        self.require_allocation_allowed();
        self.maybe_collect_before_allocation();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        state.values.allocate_word(&mut state.heap, vtable, value)
    }

    pub(crate) fn make_word(&self, value: u64) -> Oop {
        self.assert_owner();
        self.require_allocation_allowed();
        self.maybe_collect_before_allocation();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        let vtable = state
            .canonical
            .unwrap_or_else(|| boundary::fatal_invariant("canonical objects not installed"))
            .word_vt;
        state.values.allocate_word(&mut state.heap, vtable, value)
    }

    pub(crate) fn allocate_string(&self, vtable: Oop, bytes: &[u8]) -> Oop {
        self.assert_owner();
        self.require_allocation_allowed();
        self.maybe_collect_before_allocation();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        state.values.allocate_string(&mut state.heap, vtable, bytes)
    }

    pub(crate) fn make_string_with_finalizer(
        &self,
        bytes: &[u8],
        finalizer: crate::abi::Finalizer,
    ) -> Oop {
        self.assert_owner();
        // SAFETY: assert_owner established the same authority as Runtime::enter.
        unsafe { self.make_string_with_finalizer_active(bytes, finalizer) }
    }

    /// Allocate a canonical String and install its finalizer after `enter` has
    /// established owner-thread authority for this runtime.
    ///
    /// # Safety
    ///
    /// The caller must be executing within this exact runtime's active dynamic
    /// extent. Collection-phase and automatic-collection checks remain active.
    pub(crate) unsafe fn make_string_with_finalizer_active(
        &self,
        bytes: &[u8],
        finalizer: crate::abi::Finalizer,
    ) -> Oop {
        self.require_allocation_allowed();
        self.maybe_collect_before_allocation();
        // SAFETY: active-runtime serialization and no foreign call in this
        // scope. Automatic collection completed before this state borrow.
        let state = unsafe { &mut *self.state.get() };
        let string_vtable = state
            .canonical
            .unwrap_or_else(|| boundary::fatal_invariant("canonical objects not installed"))
            .string_vt;
        let object = state
            .values
            .allocate_string(&mut state.heap, string_vtable, bytes);
        boundary::require_invariant(
            state.heap.set_finalizer(object, finalizer),
            "new String did not retain finalizer metadata",
        );
        object
    }

    pub(crate) fn clone_string(&self, object: Oop, finalizer: crate::abi::Finalizer) -> Oop {
        if object.is_null() {
            return object;
        }
        self.assert_owner();
        self.require_allocation_allowed();
        self.maybe_collect_before_allocation();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        let bytes = state
            .values
            .string_bytes(object)
            .unwrap_or_else(|| boundary::fatal_invariant("String clone lost byte ownership"))
            .to_vec();
        let string_vtable = state
            .canonical
            .unwrap_or_else(|| boundary::fatal_invariant("canonical objects not installed"))
            .string_vt;
        let clone = state
            .values
            .allocate_string(&mut state.heap, string_vtable, &bytes);
        boundary::require_invariant(
            state.heap.set_finalizer(clone, finalizer),
            "String clone did not retain finalizer metadata",
        );
        clone
    }

    pub(crate) fn install_canonical(&self, canonical: CanonicalObjects) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let slot = unsafe { &mut (*self.state.get()).canonical };
        boundary::require_invariant(slot.is_none(), "canonical objects installed twice");
        *slot = Some(canonical);
    }

    pub(crate) fn install_internal_dictionaries(&self, symbols: Oop, environment: Oop) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        boundary::require_invariant(
            state.internal_dictionaries.is_none(),
            "internal dictionaries installed twice",
        );
        state.internal_dictionaries = Some((symbols, environment));
        for index in 0..state.symbols.count() {
            let symbol = state.symbols.at(index);
            state
                .vtables
                .add_opaque_binding(symbols, symbol, core::ptr::null_mut());
        }
    }

    pub(crate) fn canonical(&self) -> CanonicalObjects {
        self.assert_owner();
        // SAFETY: owner-thread serialization; the identity set is copied out.
        unsafe { (*self.state.get()).canonical }
            .unwrap_or_else(|| boundary::fatal_invariant("canonical objects not installed"))
    }

    pub(crate) fn intern(&self, bytes: &[u8]) -> Result<Oop, EmbeddedNul> {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        intern_in_state(state, bytes)
    }

    pub(crate) fn symbol_name(&self, symbol: Oop) -> *const core::ffi::c_char {
        self.assert_owner();
        // SAFETY: owner-thread serialization; the permanent Symbol record owns
        // the returned NUL-terminated bytes for the runtime lifetime.
        unsafe { (*self.state.get()).symbols.c_name_of(symbol) }
    }

    /// Return a permanent Symbol name after `enter` has already established
    /// this runtime as active on the owner thread.
    pub(crate) unsafe fn symbol_name_active(&self, symbol: Oop) -> *const core::ffi::c_char {
        // SAFETY: the active-runtime guard proves owner-thread serialization;
        // the permanent Symbol record owns the returned bytes for runtime life.
        unsafe { (*self.state.get()).symbols.c_name_of(symbol) }
    }

    pub(crate) fn import(&self, bytes: &[u8]) -> Result<Oop, EmbeddedNul> {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        let symbol = intern_in_state(state, bytes)?;
        Ok(state.environment.import(symbol))
    }

    pub(crate) fn export(&self, bytes: &[u8], value: Oop) -> Result<Oop, EmbeddedNul> {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        let symbol = intern_in_state(state, bytes)?;
        let exported = state.environment.export(symbol, value);
        if let Some((_, environment)) = state.internal_dictionaries {
            state.vtables.add_opaque_binding(environment, symbol, value);
        }
        Ok(exported)
    }

    pub(crate) fn exported_name(&self, object: Oop) -> *const core::ffi::c_char {
        self.assert_owner();
        // SAFETY: owner-thread serialization; the permanent Symbol record owns
        // the returned NUL-terminated bytes for the runtime lifetime.
        let state = unsafe { &*self.state.get() };
        let symbol = state.environment.exported_name_of(object);
        state.symbols.c_name_of(symbol)
    }

    pub(crate) fn publish_canonical_exports(&self) {
        let globals = self.globals();
        let canonical = self.canonical();
        for (name, value) in [
            (&b"Object"[..], canonical.object_prototype),
            (&b"Integer"[..], canonical.integer_prototype),
            (&b"Float"[..], canonical.float_prototype),
            (&b"Word"[..], canonical.word_prototype),
            (&b"String"[..], canonical.string_prototype),
            (&b"Boolean"[..], canonical.true_object),
            (&b"Vtable"[..], globals.vtable_vt),
            (&b"Symbol"[..], canonical.symbol_prototype),
            (&b"SymbolVt"[..], globals.symbol_vt),
            (&b"ObjectVt"[..], globals.object_vt),
            (&b"IntegerVt"[..], canonical.smallint_vt),
            (&b"FloatVt"[..], canonical.float_vt),
            (&b"WordVt"[..], canonical.word_vt),
            (&b"StringVt"[..], canonical.string_vt),
        ] {
            self.export(name, value)
                .unwrap_or_else(|_| boundary::fatal_invariant("static export name contains NUL"));
        }
    }

    pub(crate) fn install_execution_objects(&self) {
        self.assert_owner();
        let globals = self.globals();
        let machine_vtable = self.create_vtable(globals.object_vt);
        let frame_view_vtable = self.create_vtable(globals.object_vt);
        let continue_action_vtable = self.create_vtable(globals.object_vt);
        let call_action_vtable = self.create_vtable(globals.object_vt);
        let tail_call_action_vtable = self.create_vtable(globals.object_vt);
        let return_action_vtable = self.create_vtable(globals.object_vt);
        let machine_prototype = self.allocate_empty(machine_vtable, Permanence::Permanent);
        let frame_view_prototype = self.allocate_empty(frame_view_vtable, Permanence::Permanent);
        let continue_action_prototype =
            self.allocate_empty(continue_action_vtable, Permanence::Permanent);
        let call_action_prototype = self.allocate_empty(call_action_vtable, Permanence::Permanent);
        let tail_call_action_prototype =
            self.allocate_empty(tail_call_action_vtable, Permanence::Permanent);
        let return_action_prototype =
            self.allocate_empty(return_action_vtable, Permanence::Permanent);
        let oop_at = self
            .intern(b"oopAt:")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let oop_at_put = self
            .intern(b"oopAt:put:")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let continue_at = self
            .intern(b"continueAt:")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let call_site = self
            .intern(b"callSite:")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let tail_call_site = self
            .intern(b"tailCallSite:")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let return_at = self
            .intern(b"returnAt:")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let new_selector = self
            .intern(b"new")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let clone_selector = self
            .intern(b"clone")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let action_kind_selector = self
            .intern(b"actionKind")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let schema_version_selector = self
            .intern(b"schemaVersion")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let machine_index_selector = self
            .intern(b"machineIndex")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let machine_generation_selector = self
            .intern(b"machineGeneration")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let drive_epoch_selector = self
            .intern(b"driveEpoch")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let value_slot_selector = self
            .intern(b"valueSlot")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let receiver_slot_selector = self
            .intern(b"receiverSlot")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let selector_slot_selector = self
            .intern(b"selectorSlot")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let argument_start_selector = self
            .intern(b"argumentStart")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let argument_count_selector = self
            .intern(b"argumentCount")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let result_slot_selector = self
            .intern(b"resultSlot")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let continue_kind = self
            .intern(b"continue")
            .unwrap_or_else(|_| boundary::fatal_invariant("static symbol contains NUL"));
        let call_kind = self
            .intern(b"call")
            .unwrap_or_else(|_| boundary::fatal_invariant("static symbol contains NUL"));
        let tail_call_kind = self
            .intern(b"tailCall")
            .unwrap_or_else(|_| boundary::fatal_invariant("static symbol contains NUL"));
        let return_kind = self
            .intern(b"return")
            .unwrap_or_else(|_| boundary::fatal_invariant("static symbol contains NUL"));
        let live_frame_count = self
            .intern(b"liveFrameCount")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let high_water_live_frame_count = self
            .intern(b"highWaterLiveFrameCount")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let reserved_arena_bytes = self
            .intern(b"reservedArenaBytes")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let current_synchronous_boundary_depth = self
            .intern(b"currentSynchronousBoundaryDepth")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let high_water_synchronous_boundary_depth = self
            .intern(b"highWaterSynchronousBoundaryDepth")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let current_frame_view = self
            .intern(b"currentFrameView")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let implementation_identity = self
            .intern(b"implementationIdentity")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let generation_identity = self
            .intern(b"generationIdentity")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let method_identity = self
            .intern(b"methodIdentity")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let source_map_identity = self
            .intern(b"sourceMapIdentity")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let source_map_location = self
            .intern(b"sourceMapLocation")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let program_counter = self
            .intern(b"programCounter")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let slot_count = self
            .intern(b"slotCount")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let slot_kind_at = self
            .intern(b"slotKindAt:")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let frame_view_oop_at = self
            .intern(b"oopAt:")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let oop_slot_kind = self
            .intern(b"oop")
            .unwrap_or_else(|_| boundary::fatal_invariant("static symbol contains NUL"));
        let raw_slot_kind = self
            .intern(b"raw")
            .unwrap_or_else(|_| boundary::fatal_invariant("static symbol contains NUL"));
        let stale_frame_view_kind = self
            .intern(b"staleFrameView")
            .unwrap_or_else(|_| boundary::fatal_invariant("static symbol contains NUL"));
        let invalid_frame_slot_kind = self
            .intern(b"invalidFrameSlot")
            .unwrap_or_else(|_| boundary::fatal_invariant("static symbol contains NUL"));

        for (vtable, owner) in [
            (machine_vtable, machine_prototype),
            (continue_action_vtable, continue_action_prototype),
            (call_action_vtable, call_action_prototype),
            (tail_call_action_vtable, tail_call_action_prototype),
            (return_action_vtable, return_action_prototype),
        ] {
            self.set_vtable_owner(vtable, owner);
            self.set_layout(vtable, core::ptr::null(), 0, crate::abi::SLOT_RAW);
        }
        self.set_vtable_owner(frame_view_vtable, frame_view_prototype);
        self.set_layout(
            frame_view_vtable,
            FRAME_VIEW_LAYOUT.as_ptr(),
            FRAME_VIEW_LAYOUT.len(),
            crate::abi::SLOT_RAW,
        );
        self.set_trace_hook(machine_vtable, crate::execution::machine_trace_hook);
        self.gc_register_root_provider(crate::execution::execution_root_provider);
        for (name, object) in [
            (&b"Machine"[..], machine_prototype),
            (&b"FrameView"[..], frame_view_prototype),
            (&b"ContinueAction"[..], continue_action_prototype),
            (&b"CallAction"[..], call_action_prototype),
            (&b"TailCallAction"[..], tail_call_action_prototype),
            (&b"ReturnAction"[..], return_action_prototype),
        ] {
            self.export(name, object).unwrap_or_else(|_| {
                boundary::fatal_invariant("static execution export name contains NUL")
            });
        }
        for (selector, method) in [
            (
                new_selector,
                crate::execution::machine_new as crate::abi::Method,
            ),
            (
                clone_selector,
                crate::execution::machine_clone_disabled as crate::abi::Method,
            ),
            (
                oop_at,
                crate::execution::machine_oop_at as crate::abi::Method,
            ),
            (
                oop_at_put,
                crate::execution::machine_oop_at_put as crate::abi::Method,
            ),
            (
                continue_at,
                crate::execution::machine_continue_at as crate::abi::Method,
            ),
            (
                call_site,
                crate::execution::machine_call_site as crate::abi::Method,
            ),
            (
                tail_call_site,
                crate::execution::machine_tail_call_site as crate::abi::Method,
            ),
            (
                return_at,
                crate::execution::machine_return_at as crate::abi::Method,
            ),
            (
                live_frame_count,
                crate::execution::machine_live_frame_count as crate::abi::Method,
            ),
            (
                high_water_live_frame_count,
                crate::execution::machine_high_water_live_frame_count as crate::abi::Method,
            ),
            (
                reserved_arena_bytes,
                crate::execution::machine_reserved_arena_bytes as crate::abi::Method,
            ),
            (
                current_synchronous_boundary_depth,
                crate::execution::machine_current_synchronous_boundary_depth as crate::abi::Method,
            ),
            (
                high_water_synchronous_boundary_depth,
                crate::execution::machine_high_water_synchronous_boundary_depth
                    as crate::abi::Method,
            ),
            (
                current_frame_view,
                crate::execution::machine_current_frame_view as crate::abi::Method,
            ),
        ] {
            let _ = self.add_method(machine_vtable, selector, method);
        }
        for vtable in [
            continue_action_vtable,
            call_action_vtable,
            tail_call_action_vtable,
            return_action_vtable,
        ] {
            for (selector, method) in [
                (
                    action_kind_selector,
                    crate::execution::action_kind as crate::abi::Method,
                ),
                (
                    schema_version_selector,
                    crate::execution::action_schema_version as crate::abi::Method,
                ),
                (
                    machine_index_selector,
                    crate::execution::action_machine_index as crate::abi::Method,
                ),
                (
                    machine_generation_selector,
                    crate::execution::action_machine_generation as crate::abi::Method,
                ),
                (
                    drive_epoch_selector,
                    crate::execution::action_drive_epoch as crate::abi::Method,
                ),
            ] {
                let _ = self.add_method(vtable, selector, method);
            }
        }
        let _ = self.add_method(
            return_action_vtable,
            value_slot_selector,
            crate::execution::return_action_value_slot,
        );
        for vtable in [call_action_vtable, tail_call_action_vtable] {
            for (selector, method) in [
                (
                    receiver_slot_selector,
                    crate::execution::action_receiver_slot as crate::abi::Method,
                ),
                (
                    selector_slot_selector,
                    crate::execution::action_selector_slot as crate::abi::Method,
                ),
                (
                    argument_start_selector,
                    crate::execution::action_argument_start as crate::abi::Method,
                ),
                (
                    argument_count_selector,
                    crate::execution::action_argument_count as crate::abi::Method,
                ),
            ] {
                let _ = self.add_method(vtable, selector, method);
            }
        }
        let _ = self.add_method(
            call_action_vtable,
            result_slot_selector,
            crate::execution::call_action_result_slot,
        );
        for (selector, method) in [
            (
                implementation_identity,
                crate::execution::frame_view_implementation_identity as crate::abi::Method,
            ),
            (
                generation_identity,
                crate::execution::frame_view_generation_identity as crate::abi::Method,
            ),
            (
                method_identity,
                crate::execution::frame_view_method_identity as crate::abi::Method,
            ),
            (
                source_map_identity,
                crate::execution::frame_view_source_map_identity as crate::abi::Method,
            ),
            (
                source_map_location,
                crate::execution::frame_view_source_map_location as crate::abi::Method,
            ),
            (
                program_counter,
                crate::execution::frame_view_program_counter as crate::abi::Method,
            ),
            (
                slot_count,
                crate::execution::frame_view_slot_count as crate::abi::Method,
            ),
            (
                slot_kind_at,
                crate::execution::frame_view_slot_kind_at as crate::abi::Method,
            ),
            (
                frame_view_oop_at,
                crate::execution::frame_view_oop_at as crate::abi::Method,
            ),
        ] {
            let _ = self.add_method(frame_view_vtable, selector, method);
        }

        // SAFETY: owner-thread serialization and no foreign call in this scope.
        unsafe {
            (&mut *self.state.get())
                .execution
                .install_objects(ExecutionObjects {
                    machine_vtable,
                    machine_prototype,
                    frame_view_vtable,
                    frame_view_prototype,
                    continue_action_vtable,
                    continue_action_prototype,
                    call_action_vtable,
                    call_action_prototype,
                    tail_call_action_vtable,
                    tail_call_action_prototype,
                    return_action_vtable,
                    return_action_prototype,
                    continue_kind,
                    call_kind,
                    tail_call_kind,
                    return_kind,
                    oop_slot_kind,
                    raw_slot_kind,
                    stale_frame_view_kind,
                    invalid_frame_slot_kind,
                    oop_at,
                    oop_at_put,
                    continue_at,
                    call_site,
                    tail_call_site,
                    return_at,
                })
        };
    }

    pub(crate) fn create_stackless_machine(&self) -> Result<Oop, MachineError> {
        self.assert_owner();
        self.require_allocation_allowed();
        // SAFETY: owner-thread serialization and a copied identity set.
        let objects = unsafe { (*self.state.get()).execution.objects() }
            .ok_or(MachineError::InvalidLifecycle)?;
        let machine = self.allocate_instance(objects.machine_vtable, 3 * size_of::<usize>());
        let continue_action =
            self.allocate_instance(objects.continue_action_vtable, 4 * size_of::<usize>());
        let call_action =
            self.allocate_instance(objects.call_action_vtable, size_of::<[usize; 9]>());
        let tail_call_action =
            self.allocate_instance(objects.tail_call_action_vtable, size_of::<[usize; 8]>());
        let return_action =
            self.allocate_instance(objects.return_action_vtable, 5 * size_of::<usize>());
        let owner = current_thread_token();
        // SAFETY: allocations completed; owner-thread serialization permits one
        // short mutable access with no callback or allocation in this scope.
        let initialized = unsafe {
            let state = &mut *self.state.get();
            let capability = state.execution.create_machine_with_objects(
                owner,
                machine,
                continue_action,
                call_action,
                tail_call_action,
                return_action,
            )?;
            state
                .execution
                .initialize_machine_payload(&mut state.heap, machine, capability)
        };
        if !initialized {
            return Err(MachineError::StaleCapability);
        }
        self.set_finalizer(machine, crate::execution::machine_finalizer);
        Ok(machine)
    }

    pub(crate) fn stackless_machine_capability(
        &self,
        machine: Oop,
    ) -> Result<MachineCapability, MachineError> {
        self.assert_owner();
        // SAFETY: owner-thread serialization and immutable access only.
        unsafe {
            let state = &*self.state.get();
            state.execution.capability_for_object(&state.heap, machine)
        }
    }

    pub(crate) fn destroy_stackless_machine(&self, machine: Oop) -> Result<(), MachineError> {
        let capability = self.stackless_machine_capability(machine)?;
        // SAFETY: owner-thread serialization and no callback in this scope.
        unsafe {
            (&mut *self.state.get())
                .execution
                .destroy_machine_explicit(current_thread_token(), capability)
        }
    }

    pub(crate) fn trace_stackless_machine(&self, machine: Oop, mark: MarkFn) {
        // SAFETY: the trace hook runs under owner-thread Runtime::enter and the
        // callback may be used only synchronously during this immutable access.
        unsafe {
            let state = &*self.state.get();
            if let Some(objects) = state.execution.objects() {
                state.execution.trace_object_roots(
                    &state.heap,
                    machine,
                    objects.machine_vtable,
                    mark,
                );
            }
        }
    }

    pub(crate) fn trace_stackless_scoped_roots(&self, mark: MarkFn) {
        // SAFETY: root-provider invocation is owner-thread-only, performs no
        // mutation, and the execution registry does not retain the callback.
        unsafe { (*self.state.get()).execution.trace_scoped_host_roots(mark) };
    }

    fn push_stackless_scoped_root(&self, machine: Oop) {
        // SAFETY: owner-thread driver setup and no callback in this scope.
        unsafe {
            (&mut *self.state.get())
                .execution
                .push_scoped_host_root(machine)
        };
    }

    fn pop_stackless_scoped_root(&self, machine: Oop) {
        // SAFETY: the drive guard unwinds on the owner thread in strict LIFO
        // order, after all generated callbacks have returned.
        unsafe {
            (&mut *self.state.get())
                .execution
                .pop_scoped_host_root(machine)
        };
    }

    fn enter_stackless_scope(&self, capability: MachineCapability) -> StacklessEntryScopeGuard<'_> {
        self.assert_owner();
        // SAFETY: the entry-scope stack is a distinct owner-thread-only borrow
        // domain and no reference to it crosses generated code.
        unsafe { (&mut *self.entry_scopes.get()).push(capability) };
        StacklessEntryScopeGuard {
            runtime: self,
            capability,
        }
    }

    fn without_stackless_entry_scope<R>(&self, operation: impl FnOnce() -> R) -> R {
        self.assert_owner();
        // SAFETY: the stack is a distinct owner-thread borrow domain; move its
        // contents out before invoking unrelated host/reactor work.
        let suppressed = unsafe { core::mem::take(&mut *self.entry_scopes.get()) };
        let _guard = StacklessEntryScopeSuppressionGuard {
            runtime: self,
            suppressed,
        };
        operation()
    }

    fn enter_stackless_boundary(
        &self,
        owner: u64,
        capability: MachineCapability,
    ) -> Result<StacklessBoundaryGuard<'_>, MachineError> {
        // SAFETY: this point mutation ends before the synchronous Method call.
        unsafe {
            (&mut *self.state.get())
                .execution
                .machine_mut(owner, capability)?
                .enter_synchronous_boundary(owner)?;
        }
        Ok(StacklessBoundaryGuard {
            runtime: self,
            capability,
            owner,
        })
    }

    /// Finalize a detached Machine payload under the active Runtime.
    ///
    /// # Safety
    ///
    /// The heap must still own the detached allocation until this call returns.
    pub(crate) unsafe fn finalize_stackless_machine(&self, machine: Oop) {
        // SAFETY: owner-thread finalization provides exclusive execution-state
        // access, and the caller forwards the detached-payload lifetime.
        unsafe {
            (&mut *self.state.get())
                .execution
                .finalize_detached_machine(machine)
        };
    }

    pub(crate) fn stackless_oop_at(
        &self,
        machine: Oop,
        index: usize,
    ) -> Result<Oop, ExecutionError> {
        let capability = self.stackless_machine_capability(machine)?;
        // SAFETY: owner-thread serialization and immutable access only.
        unsafe {
            (*self.state.get()).execution.read_current_oop(
                current_thread_token(),
                capability,
                index,
            )
        }
    }

    pub(crate) fn stackless_oop_at_put(
        &self,
        machine: Oop,
        index: usize,
        value: Oop,
    ) -> Result<Oop, ExecutionError> {
        let capability = self.stackless_machine_capability(machine)?;
        // SAFETY: owner-thread serialization and no callback in this scope.
        unsafe {
            (&mut *self.state.get()).execution.write_current_oop(
                current_thread_token(),
                capability,
                index,
                value,
            )
        }
    }

    pub(crate) fn stackless_current_frame_view(&self, machine: Oop) -> Result<Oop, FrameViewError> {
        let capability = self
            .stackless_machine_capability(machine)
            .map_err(|_| FrameViewError::StaleFrameView)?;
        let handle = unsafe {
            (*self.state.get())
                .execution
                .current_frame_view_handle(current_thread_token(), capability)?
        };
        let Some(handle) = handle else {
            return Ok(self.canonical().nil);
        };
        self.push_stackless_scoped_root(machine);
        let _root_guard = StacklessDriveRootGuard {
            runtime: self,
            machine,
        };
        let objects = unsafe { (*self.state.get()).execution.objects() }
            .ok_or(FrameViewError::InvalidView)?;
        let view = self.allocate_instance(objects.frame_view_vtable, 4 * size_of::<usize>());
        unsafe {
            let state = &mut *self.state.get();
            state
                .execution
                .initialize_frame_view(&mut state.heap, view, machine, handle)?;
        }
        Ok(view)
    }

    pub(crate) fn stackless_frame_view_identity_word(
        &self,
        view: Oop,
        field: usize,
    ) -> Result<Oop, FrameViewError> {
        let snapshot = unsafe {
            let state = &*self.state.get();
            state
                .execution
                .frame_view_snapshot(&state.heap, current_thread_token(), view)?
        };
        let value = match field {
            0 | 2 => snapshot.identity.implementation,
            1 => snapshot.identity.generation,
            3 => snapshot.identity.source_map,
            _ => return Err(FrameViewError::InvalidView),
        };
        Ok(self.make_word(value))
    }

    pub(crate) fn stackless_frame_view_index_word(
        &self,
        view: Oop,
        field: usize,
    ) -> Result<Oop, FrameViewError> {
        let snapshot = unsafe {
            let state = &*self.state.get();
            state
                .execution
                .frame_view_snapshot(&state.heap, current_thread_token(), view)?
        };
        // `sourceMapLocation` is the current PC within the separately named
        // immutable source map. The pair is the stable copied location token.
        let value = match field {
            0 | 1 => snapshot.program_counter,
            2 => snapshot.slot_count,
            _ => return Err(FrameViewError::InvalidView),
        };
        let value = i64::try_from(value).map_err(|_| FrameViewError::InvalidView)?;
        Ok(self.make_integer(value))
    }

    pub(crate) fn stackless_frame_view_slot_kind(
        &self,
        view: Oop,
        index: usize,
    ) -> Result<Oop, FrameViewError> {
        let kind = unsafe {
            let state = &*self.state.get();
            state.execution.frame_view_slot_kind(
                &state.heap,
                current_thread_token(),
                view,
                index,
            )?
        };
        let objects = unsafe { (*self.state.get()).execution.objects() }
            .ok_or(FrameViewError::InvalidView)?;
        Ok(match kind {
            SlotKind::Oop => objects.oop_slot_kind,
            SlotKind::Raw => objects.raw_slot_kind,
        })
    }

    pub(crate) fn stackless_frame_view_oop(
        &self,
        view: Oop,
        index: usize,
    ) -> Result<Oop, FrameViewError> {
        unsafe {
            let state = &*self.state.get();
            state
                .execution
                .frame_view_oop(&state.heap, current_thread_token(), view, index)
        }
    }

    pub(crate) fn stackless_frame_view_error(&self, error: FrameViewError) -> Oop {
        let Some(objects) = (unsafe { (*self.state.get()).execution.objects() }) else {
            return self.canonical().nil;
        };
        let kind = match error {
            FrameViewError::InvalidFrameSlot => objects.invalid_frame_slot_kind,
            FrameViewError::InvalidView | FrameViewError::StaleFrameView => {
                objects.stale_frame_view_kind
            }
        };
        let Ok(factory) = self.import(b"Error") else {
            return self.canonical().nil;
        };
        if factory.is_null() {
            return self.canonical().nil;
        }
        let selector = self
            .intern(b"kind:")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        self.send_slice(factory, selector, &[kind])
    }

    pub(crate) fn stackless_continue_at(
        &self,
        machine: Oop,
        next_pc: usize,
    ) -> Result<Oop, ExecutionError> {
        let capability = self.stackless_machine_capability(machine)?;
        // SAFETY: owner-thread serialization; execution and heap are disjoint
        // fields borrowed only for this allocation-free publication.
        unsafe {
            let state = &mut *self.state.get();
            state.execution.publish_continue(
                &mut state.heap,
                current_thread_token(),
                capability,
                next_pc,
            )
        }
    }

    pub(crate) fn stackless_return_at(
        &self,
        machine: Oop,
        value_slot: usize,
    ) -> Result<Oop, ExecutionError> {
        let capability = self.stackless_machine_capability(machine)?;
        // SAFETY: same short disjoint-field borrow as stackless_continue_at.
        unsafe {
            let state = &mut *self.state.get();
            state.execution.publish_return(
                &mut state.heap,
                current_thread_token(),
                capability,
                value_slot,
            )
        }
    }

    pub(crate) fn stackless_call_site(
        &self,
        machine: Oop,
        call_site_index: usize,
        kind: CallSiteKind,
    ) -> Result<Oop, ExecutionError> {
        let capability = self.stackless_machine_capability(machine)?;
        // SAFETY: owner-thread serialization; execution and heap are disjoint
        // fields borrowed only for this allocation-free publication.
        unsafe {
            let state = &mut *self.state.get();
            state.execution.publish_call_site(
                &mut state.heap,
                current_thread_token(),
                capability,
                call_site_index,
                kind,
            )
        }
    }

    pub(crate) fn stackless_action_kind(&self, action: Oop) -> Option<Oop> {
        self.assert_owner();
        // SAFETY: owner-thread serialization and immutable access only.
        unsafe {
            let state = &*self.state.get();
            state.execution.action_kind(&state.heap, action)
        }
    }

    pub(crate) fn stackless_action_integer_word(&self, action: Oop, index: usize) -> Oop {
        self.assert_owner();
        // Copy the raw value before make_integer can allocate or collect.
        let word = unsafe {
            let state = &*self.state.get();
            state.execution.action_word(&state.heap, action, index)
        };
        let Some(value) = word.and_then(|word| i64::try_from(word).ok()) else {
            return self.canonical().nil;
        };
        self.make_integer(value)
    }

    pub(crate) fn stackless_machine_diagnostic_word(&self, machine: Oop, index: usize) -> Oop {
        let capability = match self.stackless_machine_capability(machine) {
            Ok(capability) => capability,
            Err(_) => return self.canonical().nil,
        };
        // Copy diagnostics before make_integer can allocate or collect.
        let diagnostics = unsafe {
            (*self.state.get())
                .execution
                .machine_diagnostics(current_thread_token(), capability)
        };
        let Ok(diagnostics) = diagnostics else {
            return self.canonical().nil;
        };
        let value = match index {
            0 => diagnostics.live_frames,
            1 => diagnostics.high_water_live_frames,
            2 => diagnostics.reserved_bytes,
            3 => diagnostics.current_synchronous_boundary_depth,
            4 => diagnostics.high_water_synchronous_boundary_depth,
            _ => return self.canonical().nil,
        };
        let Ok(value) = i64::try_from(value) else {
            return self.canonical().nil;
        };
        self.make_integer(value)
    }

    pub(crate) fn stackless_runtime_diagnostics(&self) -> RuntimeExecutionDiagnostics {
        self.assert_owner();
        // SAFETY: owner-thread serialization and copied scalar diagnostics.
        unsafe { (*self.state.get()).execution.runtime_diagnostics() }
    }

    pub(crate) fn stackless_registration_diagnostics(
        &self,
    ) -> crate::execution::RegistrationDiagnostics {
        self.assert_owner();
        // SAFETY: owner-thread immutable scalar snapshot.
        unsafe { (*self.state.get()).execution.registration_diagnostics() }
    }

    pub(crate) fn register_stackless_resumable(
        &self,
        carrier: SemanticRegistrationV1,
    ) -> Result<crate::execution::ResumableEntryCapability, crate::execution::RegistrationError>
    {
        self.assert_owner();
        // SAFETY: registration is an owner-thread point mutation and copies or
        // owns every carrier field before returning.
        unsafe {
            (&mut *self.state.get())
                .execution
                .register_resumable(current_thread_token(), carrier)
        }
    }

    pub(crate) fn unregister_unpublished_stackless_resumable(
        &self,
        capability: crate::execution::ResumableEntryCapability,
    ) -> Result<(), crate::execution::RegistrationError> {
        self.assert_owner();
        // SAFETY: unpublished rollback is an owner-thread point mutation.
        unsafe {
            (&mut *self.state.get())
                .execution
                .unregister_unpublished_resumable(current_thread_token(), capability)
        }
    }

    pub(crate) fn acquire_stackless_generation_lease(
        &self,
        method: MethodAddress,
    ) -> Result<GenerationLeaseCapability, crate::execution::RegistrationError> {
        self.assert_owner();
        // SAFETY: explicit lease acquisition is one owner-thread point mutation.
        unsafe {
            (&mut *self.state.get())
                .execution
                .acquire_explicit_generation_lease(method)
        }
    }

    pub(crate) fn release_stackless_generation_lease(
        &self,
        lease: GenerationLeaseCapability,
    ) -> Result<(), crate::execution::RegistrationError> {
        self.assert_owner();
        // SAFETY: explicit lease release is one owner-thread point mutation.
        unsafe {
            (&mut *self.state.get())
                .execution
                .release_explicit_generation_lease(lease)
        }
    }

    fn resolve_stackless_target(
        &self,
        receiver: Oop,
        selector: Oop,
    ) -> Result<StacklessTarget, ExecutionError> {
        let _read = self.enter_dispatch_read();
        // SAFETY: callers establish Runtime::enter and pass live values copied
        // from traced Frame storage.
        let Some(method) = (unsafe { self.bind_active(receiver, selector) }) else {
            return Ok(StacklessTarget::Unbound);
        };
        // Classification uses the exact carrier returned by metacircular
        // lookup; there is no second vtable or parent-chain walk.
        let resumable = unsafe {
            (&mut *self.state.get())
                .execution
                .classify_resumable(method)
        }?;
        Ok(resumable.unwrap_or_else(|| StacklessTarget::Synchronous(method.to_method())))
    }

    fn materialize_registered_target(
        &self,
        entry: crate::execution::ResumableEntryCapability,
        receiver: Oop,
        arguments: &[Oop],
    ) -> Result<MaterializedEntry, ExecutionError> {
        let result = unsafe {
            (*self.state.get()).execution.materialize_acquired_entry(
                entry,
                receiver,
                arguments,
                self.canonical().nil,
            )
        };
        if result.is_err() {
            // SAFETY: a failed materialization cannot transfer the acquisition.
            let _ = unsafe {
                (&mut *self.state.get())
                    .execution
                    .release_acquired_entry(entry)
            };
        }
        result.map_err(ExecutionError::from)
    }

    pub(crate) unsafe fn enter_registered_resumable(
        &self,
        thunk: Method,
        receiver: Oop,
        arguments: *const Oop,
        count: usize,
    ) -> Result<Oop, ExecutionError> {
        if arguments.is_null() && count != 0 {
            return Err(ExecutionError::InvalidAction);
        }
        let arguments = if count == 0 {
            &[]
        } else {
            // SAFETY: the ordinary Method ABI supplies count live Oops.
            unsafe { core::slice::from_raw_parts(arguments, count) }
        };
        let method = MethodAddress::from_method(thunk);
        let target = unsafe {
            (&mut *self.state.get())
                .execution
                .classify_resumable(method)
        }?
        .ok_or(ExecutionError::MissingImplementation)?;
        let StacklessTarget::Registered { entry } = target else {
            return Err(ExecutionError::MissingImplementation);
        };
        let MaterializedEntry {
            descriptor,
            initial_oops,
        } = self.materialize_registered_target(entry, receiver, arguments)?;
        let inherited = unsafe { (&*self.entry_scopes.get()).last().copied() };
        if let Some(capability) = inherited {
            let machine = unsafe {
                (*self.state.get())
                    .execution
                    .machine_object(current_thread_token(), capability)?
            };
            self.drive_stackless_nested_with_resolver(
                machine,
                descriptor,
                &initial_oops,
                Some(entry),
                |runtime, receiver, selector, arguments| {
                    let _ = arguments;
                    runtime.resolve_stackless_target(receiver, selector)
                },
            )
        } else {
            let machine = self.create_stackless_machine()?;
            let result = self.drive_stackless_with_resolver(
                machine,
                descriptor,
                &initial_oops,
                Some(entry),
                |runtime, receiver, selector, arguments| {
                    let _ = arguments;
                    runtime.resolve_stackless_target(receiver, selector)
                },
            );
            if result.is_ok() {
                self.destroy_stackless_machine(machine)?;
            }
            result
        }
    }

    pub(crate) unsafe fn enter_registered_resumable_or_fatal(
        &self,
        thunk: Method,
        receiver: Oop,
        arguments: *const Oop,
        count: usize,
    ) -> Oop {
        // SAFETY: the thunk forwards its ordinary Method ABI arguments.
        unsafe { self.enter_registered_resumable(thunk, receiver, arguments, count) }
            .unwrap_or_else(|error| boundary::fatal_invariant(error.failure_kind().symbol()))
    }

    fn invoke_stackless_synchronous(
        &self,
        owner: u64,
        capability: MachineCapability,
        method: Method,
        receiver: Oop,
        arguments: &[Oop],
    ) -> Result<Oop, ExecutionError> {
        let _boundary = self.enter_stackless_boundary(owner, capability)?;
        let _scope = self.enter_stackless_scope(capability);
        Ok(self.enter(|| {
            let args = if arguments.is_empty() {
                core::ptr::null()
            } else {
                arguments.as_ptr()
            };
            // SAFETY: the driver-owned argument slice remains live through the
            // Method call and uses null exactly for a zero count.
            unsafe { method(receiver, args, arguments.len()) }
        }))
    }

    fn invoke_stackless_unbound(
        &self,
        owner: u64,
        capability: MachineCapability,
        receiver: Oop,
        selector: Oop,
        arguments: &[Oop],
    ) -> Result<Oop, ExecutionError> {
        let _boundary = self.enter_stackless_boundary(owner, capability)?;
        let _scope = self.enter_stackless_scope(capability);
        Ok(self.enter(|| {
            let args = if arguments.is_empty() {
                core::ptr::null()
            } else {
                arguments.as_ptr()
            };
            // SAFETY: same live driver-owned buffer contract as the direct
            // synchronous adapter; send_abi performs normal DNU redispatch.
            unsafe { self.send_abi(receiver, selector, args, arguments.len()) }
        }))
    }

    fn finish_stackless_return(
        &self,
        owner: u64,
        capability: MachineCapability,
        result: Oop,
    ) -> Result<Option<Oop>, ExecutionError> {
        // SAFETY: return application is one allocation-free owner-thread state
        // transition and invokes no generated code while borrowed.
        unsafe {
            let state = &mut *self.state.get();
            let (answer, entry) = {
                let machine = state.execution.machine_mut(owner, capability)?;
                let frame = machine
                    .arena()
                    .current_handle()
                    .ok_or(crate::execution::FrameError::StaleHandle)?;
                let destination = machine.arena().return_destination(frame)?;
                let entry = machine.arena().resumable_entry(frame)?;
                machine.arena_mut().pop()?;
                let answer = if let Some(destination) = destination {
                    let caller = machine
                        .arena()
                        .current_handle()
                        .ok_or(crate::execution::FrameError::StaleHandle)?;
                    machine
                        .arena_mut()
                        .write_oop(caller, destination.result_slot, result)?;
                    None
                } else {
                    machine.complete_barrier_drive(owner)?;
                    Some(result)
                };
                (answer, entry)
            };
            if let Some(entry) = entry {
                state.execution.release_entry_frame_pin(entry)?;
            }
            Ok(answer)
        }
    }

    pub(crate) fn drive_stackless(
        &self,
        machine: Oop,
        descriptor: std::sync::Arc<FrameDescriptor>,
        initial_oops: &[(usize, Oop)],
    ) -> Result<Oop, ExecutionError> {
        self.drive_stackless_with_resolver(
            machine,
            descriptor,
            initial_oops,
            None,
            |runtime, receiver, selector, _arguments| {
                runtime.resolve_stackless_target(receiver, selector)
            },
        )
    }

    fn drive_stackless_with_resolver(
        &self,
        machine: Oop,
        descriptor: std::sync::Arc<FrameDescriptor>,
        initial_oops: &[(usize, Oop)],
        initial_entry: Option<crate::execution::ResumableEntryCapability>,
        mut resolve: impl FnMut(&Runtime, Oop, Oop, &[Oop]) -> Result<StacklessTarget, ExecutionError>,
    ) -> Result<Oop, ExecutionError> {
        self.assert_owner();
        let capability = self.stackless_machine_capability(machine)?;
        let owner = current_thread_token();
        let nil = self.canonical().nil;
        self.push_stackless_scoped_root(machine);
        let _root_guard = StacklessDriveRootGuard {
            runtime: self,
            machine,
        };
        // SAFETY: owner-thread serialization; this borrow ends before any
        // descriptor/frame mutation or generated-code entry.
        unsafe {
            let state = &mut *self.state.get();
            let machine_state = state.execution.machine_mut(owner, capability)?;
            machine_state.begin_initial_drive(owner)
        }?;
        // Frame push validates all descriptor inputs before touching storage.
        // If it still refuses after the lifecycle transition, fail the Machine
        // rather than leaving a Running state with no current frame.
        let pushed = unsafe {
            let state = &mut *self.state.get();
            if let Some(entry) = initial_entry {
                state.execution.transfer_acquired_entry_to_frame(entry)?;
            }
            let pushed = state
                .execution
                .machine_mut(owner, capability)?
                .arena_mut()
                .push_with_return_and_entry(descriptor, 0, nil, initial_oops, None, initial_entry);
            if pushed.is_err()
                && let Some(entry) = initial_entry
            {
                state
                    .execution
                    .release_entry_frame_pin(entry)
                    .map_err(ExecutionError::from)?;
            }
            pushed
        };
        if let Err(error) = pushed {
            // SAFETY: owner-thread serialization and no callback in this scope.
            unsafe {
                let state = &mut *self.state.get();
                if let Ok(machine_state) = state.execution.machine_mut(owner, capability) {
                    let _ = machine_state.fail(owner);
                }
            }
            return Err(error.into());
        }

        let result = self.run_stackless_loop(machine, capability, owner, nil, &mut resolve);
        self.fail_stackless_drive_on_error(owner, capability, result)
    }

    fn drive_stackless_nested_with_resolver(
        &self,
        machine: Oop,
        descriptor: std::sync::Arc<FrameDescriptor>,
        initial_oops: &[(usize, Oop)],
        initial_entry: Option<crate::execution::ResumableEntryCapability>,
        mut resolve: impl FnMut(&Runtime, Oop, Oop, &[Oop]) -> Result<StacklessTarget, ExecutionError>,
    ) -> Result<Oop, ExecutionError> {
        self.assert_owner();
        let capability = self.stackless_machine_capability(machine)?;
        // The inherited logical scope is checked before Frame storage is
        // touched. A merely Running Machine is not authority to nest a drive.
        let inherited = unsafe { (&*self.entry_scopes.get()).last().copied() };
        if inherited != Some(capability) {
            return Err(MachineError::InvalidLifecycle.into());
        }
        let owner = current_thread_token();
        let nil = self.canonical().nil;
        self.push_stackless_scoped_root(machine);
        let _root_guard = StacklessDriveRootGuard {
            runtime: self,
            machine,
        };
        // Begin the nested epoch before touching Frame storage. This makes any
        // outer scratch action permanently stale before the nested step runs.
        unsafe {
            (&mut *self.state.get())
                .execution
                .machine_mut(owner, capability)?
                .begin_nested_drive(owner)?;
        }
        let pushed = unsafe {
            let state = &mut *self.state.get();
            if let Some(entry) = initial_entry {
                state.execution.transfer_acquired_entry_to_frame(entry)?;
            }
            let pushed = state
                .execution
                .machine_mut(owner, capability)?
                .arena_mut()
                .push_with_return_and_entry(descriptor, 0, nil, initial_oops, None, initial_entry);
            if pushed.is_err()
                && let Some(entry) = initial_entry
            {
                state
                    .execution
                    .release_entry_frame_pin(entry)
                    .map_err(ExecutionError::from)?;
            }
            pushed
        };
        if let Err(error) = pushed {
            unsafe {
                let state = &mut *self.state.get();
                if let Ok(machine_state) = state.execution.machine_mut(owner, capability) {
                    let _ = machine_state.fail(owner);
                }
            }
            return Err(error.into());
        }
        let result = self.run_stackless_loop(machine, capability, owner, nil, &mut resolve);
        self.fail_stackless_drive_on_error(owner, capability, result)
    }

    fn fail_stackless_drive_on_error(
        &self,
        owner: u64,
        capability: MachineCapability,
        result: Result<Oop, ExecutionError>,
    ) -> Result<Oop, ExecutionError> {
        if result.is_err() {
            // SAFETY: all generated activations have unwound to the driver and
            // this owner-thread point mutation invokes no callback.
            unsafe {
                let state = &mut *self.state.get();
                if let Ok(machine) = state.execution.machine_mut(owner, capability) {
                    let _ = machine.fail(owner);
                }
            }
        }
        result
    }

    fn run_stackless_loop(
        &self,
        machine: Oop,
        capability: MachineCapability,
        owner: u64,
        nil: Oop,
        resolve: &mut impl FnMut(&Runtime, Oop, Oop, &[Oop]) -> Result<StacklessTarget, ExecutionError>,
    ) -> Result<Oop, ExecutionError> {
        loop {
            // Copy all invocation inputs before crossing into generated code.
            let (method, program_counter, argument_count) = unsafe {
                let state = &*self.state.get();
                let machine_state = state.execution.machine(owner, capability)?;
                let frame = machine_state
                    .arena()
                    .current_handle()
                    .ok_or(crate::execution::FrameError::StaleHandle)?;
                let descriptor = machine_state.arena().descriptor(frame)?;
                (
                    descriptor.step_method(),
                    machine_state.arena().program_counter(frame)?,
                    descriptor.driver_argument_count(),
                )
            };
            let resume_token = if argument_count == 1 {
                let value = i64::try_from(program_counter).map_err(|_| {
                    ExecutionError::Frame(crate::execution::FrameError::InvalidProgramCounter)
                })?;
                Some(self.make_integer(value))
            } else {
                None
            };
            let action = {
                let _scope = self.enter_stackless_scope(capability);
                self.enter(|| {
                    let (arguments, count) = match &resume_token {
                        Some(token) => (core::ptr::from_ref(token), 1),
                        None => (core::ptr::null(), 0),
                    };
                    // SAFETY: descriptor validation freezes a Method-shaped entry;
                    // the optional one-Oop argument buffer lives through the call.
                    unsafe { method(machine, arguments, count) }
                })
            };

            let decoded = unsafe {
                let state = &*self.state.get();
                state
                    .execution
                    .decode_action(&state.heap, owner, capability, action)
            };
            let decoded = match decoded {
                Ok(decoded) => decoded,
                Err(error) => {
                    // SAFETY: owner-thread serialization and no callback here.
                    unsafe {
                        let state = &mut *self.state.get();
                        if let Ok(machine_state) = state.execution.machine_mut(owner, capability) {
                            let _ = machine_state.fail(owner);
                        }
                    }
                    return Err(error);
                }
            };
            match decoded {
                DecodedAction::Continue => {}
                DecodedAction::Call {
                    receiver_slot,
                    selector_slot,
                    argument_start,
                    argument_count,
                    result_slot,
                } => {
                    let operands = unsafe {
                        (*self.state.get()).execution.copy_send_operands(
                            owner,
                            capability,
                            receiver_slot,
                            selector_slot,
                            argument_start,
                            argument_count,
                        )
                    }?;
                    let target = {
                        let _scope = self.enter_stackless_scope(capability);
                        self.enter(|| {
                            resolve(
                                self,
                                operands.receiver,
                                operands.selector,
                                &operands.arguments,
                            )
                        })
                    }?;
                    match target {
                        StacklessTarget::Registered { entry } => {
                            let MaterializedEntry {
                                descriptor,
                                initial_oops,
                            } = self.materialize_registered_target(
                                entry,
                                operands.receiver,
                                &operands.arguments,
                            )?;
                            unsafe {
                                let state = &mut *self.state.get();
                                state.execution.transfer_acquired_entry_to_frame(entry)?;
                                let machine_state =
                                    state.execution.machine_mut(owner, capability)?;
                                let pushed = machine_state.arena_mut().push_with_return_and_entry(
                                    descriptor,
                                    0,
                                    nil,
                                    &initial_oops,
                                    Some(ReturnDestination { result_slot }),
                                    Some(entry),
                                );
                                if pushed.is_err() {
                                    state.execution.release_entry_frame_pin(entry)?;
                                }
                                pushed?;
                            }
                        }
                        StacklessTarget::Resumable {
                            descriptor,
                            initial_oops,
                            entry,
                        } => unsafe {
                            let state = &mut *self.state.get();
                            if let Some(entry) = entry {
                                state.execution.transfer_acquired_entry_to_frame(entry)?;
                            }
                            let machine_state = state.execution.machine_mut(owner, capability)?;
                            let pushed = machine_state.arena_mut().push_with_return_and_entry(
                                descriptor,
                                0,
                                nil,
                                &initial_oops,
                                Some(ReturnDestination { result_slot }),
                                entry,
                            );
                            if pushed.is_err()
                                && let Some(entry) = entry
                            {
                                state.execution.release_entry_frame_pin(entry)?;
                            }
                            pushed?;
                        },
                        StacklessTarget::Synchronous(method) => {
                            let result = self.invoke_stackless_synchronous(
                                owner,
                                capability,
                                method,
                                operands.receiver,
                                &operands.arguments,
                            )?;
                            unsafe {
                                (&mut *self.state.get()).execution.write_current_oop(
                                    owner,
                                    capability,
                                    result_slot,
                                    result,
                                )?;
                            }
                        }
                        StacklessTarget::Unbound => {
                            let result = self.invoke_stackless_unbound(
                                owner,
                                capability,
                                operands.receiver,
                                operands.selector,
                                &operands.arguments,
                            )?;
                            unsafe {
                                (&mut *self.state.get()).execution.write_current_oop(
                                    owner,
                                    capability,
                                    result_slot,
                                    result,
                                )?;
                            }
                        }
                    }
                }
                DecodedAction::TailCall {
                    receiver_slot,
                    selector_slot,
                    argument_start,
                    argument_count,
                } => {
                    let operands = unsafe {
                        (*self.state.get()).execution.copy_send_operands(
                            owner,
                            capability,
                            receiver_slot,
                            selector_slot,
                            argument_start,
                            argument_count,
                        )
                    }?;
                    let target = {
                        let _scope = self.enter_stackless_scope(capability);
                        self.enter(|| {
                            resolve(
                                self,
                                operands.receiver,
                                operands.selector,
                                &operands.arguments,
                            )
                        })
                    }?;
                    match target {
                        StacklessTarget::Registered { entry } => {
                            let MaterializedEntry {
                                descriptor,
                                initial_oops,
                            } = self.materialize_registered_target(
                                entry,
                                operands.receiver,
                                &operands.arguments,
                            )?;
                            unsafe {
                                let state = &mut *self.state.get();
                                let old_entry = {
                                    let machine = state.execution.machine(owner, capability)?;
                                    let frame = machine
                                        .arena()
                                        .current_handle()
                                        .ok_or(crate::execution::FrameError::StaleHandle)?;
                                    machine.arena().resumable_entry(frame)?
                                };
                                state.execution.transfer_acquired_entry_to_frame(entry)?;
                                let replaced = state
                                    .execution
                                    .machine_mut(owner, capability)?
                                    .arena_mut()
                                    .replace_current_with_entry(
                                        descriptor,
                                        0,
                                        nil,
                                        &initial_oops,
                                        Some(entry),
                                    );
                                if replaced.is_err() {
                                    state.execution.release_entry_frame_pin(entry)?;
                                } else if let Some(old_entry) = old_entry {
                                    state.execution.release_entry_frame_pin(old_entry)?;
                                }
                                replaced?;
                            }
                        }
                        StacklessTarget::Resumable {
                            descriptor,
                            initial_oops,
                            entry,
                        } => unsafe {
                            let state = &mut *self.state.get();
                            let old_entry = {
                                let machine = state.execution.machine(owner, capability)?;
                                let frame = machine
                                    .arena()
                                    .current_handle()
                                    .ok_or(crate::execution::FrameError::StaleHandle)?;
                                machine.arena().resumable_entry(frame)?
                            };
                            if let Some(entry) = entry {
                                state.execution.transfer_acquired_entry_to_frame(entry)?;
                            }
                            let replaced = state
                                .execution
                                .machine_mut(owner, capability)?
                                .arena_mut()
                                .replace_current_with_entry(
                                    descriptor,
                                    0,
                                    nil,
                                    &initial_oops,
                                    entry,
                                );
                            if replaced.is_err() {
                                if let Some(entry) = entry {
                                    state.execution.release_entry_frame_pin(entry)?;
                                }
                            } else if let Some(old_entry) = old_entry {
                                state.execution.release_entry_frame_pin(old_entry)?;
                            }
                            replaced?;
                        },
                        StacklessTarget::Synchronous(method) => {
                            let result = self.invoke_stackless_synchronous(
                                owner,
                                capability,
                                method,
                                operands.receiver,
                                &operands.arguments,
                            )?;
                            if let Some(answer) =
                                self.finish_stackless_return(owner, capability, result)?
                            {
                                return Ok(answer);
                            }
                        }
                        StacklessTarget::Unbound => {
                            let result = self.invoke_stackless_unbound(
                                owner,
                                capability,
                                operands.receiver,
                                operands.selector,
                                &operands.arguments,
                            )?;
                            if let Some(answer) =
                                self.finish_stackless_return(owner, capability, result)?
                            {
                                return Ok(answer);
                            }
                        }
                    }
                }
                DecodedAction::Return { value_slot } => {
                    let result = unsafe {
                        let state = &*self.state.get();
                        let machine_state = state.execution.machine(owner, capability)?;
                        let frame = machine_state
                            .arena()
                            .current_handle()
                            .ok_or(crate::execution::FrameError::StaleHandle)?;
                        machine_state.arena().read_oop(frame, value_slot)?
                    };
                    if let Some(answer) = self.finish_stackless_return(owner, capability, result)? {
                        return Ok(answer);
                    }
                }
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn export_count(&self) -> usize {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied count.
        unsafe { (*self.state.get()).environment.count() }
    }

    #[cfg(test)]
    pub(crate) fn export_name_at(&self, index: usize) -> Oop {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied identity.
        unsafe { (*self.state.get()).environment.name_at(index) }
    }

    pub(crate) fn vtable_for_valid_receiver(&self, receiver: Oop) -> Oop {
        self.assert_owner();
        // SAFETY: dispatch callers guarantee a live floor receiver. No runtime
        // collection borrow is needed for the header fast path.
        unsafe { Vtables::dispatch_vtable_of(receiver) }
    }

    /// Read a dispatch header after `enter` established owner-thread authority.
    ///
    /// # Safety
    ///
    /// The runtime must be active and `receiver` must be an exact live object.
    pub(crate) unsafe fn vtable_for_valid_receiver_active(&self, receiver: Oop) -> Oop {
        // SAFETY: the caller supplies the live receiver precondition.
        unsafe { Vtables::dispatch_vtable_of(receiver) }
    }

    /// Copy dispatch globals after `enter` established owner-thread authority.
    pub(crate) unsafe fn globals_active(&self) -> DispatchGlobals {
        // SAFETY: the active-runtime guard serializes access to RuntimeState.
        unsafe { (*self.state.get()).globals }
    }

    pub(crate) fn enter<R>(&self, operation: impl FnOnce() -> R) -> R {
        ACTIVE_RUNTIME.with(|active| {
            let current = active.get();
            if core::ptr::eq(current, self) {
                // The surrounding dynamic extent already proved owner-thread
                // authority for this exact runtime.
                return operation();
            }
            self.assert_owner();
            boundary::require_invariant(
                current.is_null(),
                "two Rust floor runtimes entered on one thread",
            );
            active.set(self as *const Runtime);
            let guard = ActiveRuntimeGuard {
                active,
                previous: current,
            };
            let result = operation();
            drop(guard);
            result
        })
    }

    #[cfg(test)]
    pub(crate) fn symbol_count(&self) -> usize {
        // SAFETY: test calls remain owner-thread and copy only the count.
        unsafe { (*self.state.get()).symbols.count() }
    }

    #[cfg(test)]
    pub(crate) fn symbol_at(&self, index: usize) -> Oop {
        // SAFETY: test calls remain owner-thread and copy only an identity.
        unsafe { (*self.state.get()).symbols.at(index) }
    }

    #[cfg(test)]
    pub(crate) fn symbol_bytes(&self, symbol: Oop) -> Option<&[u8]> {
        // SAFETY: test calls remain owner-thread; returned bytes are inspected
        // synchronously without mutation or foreign calls.
        unsafe { (*self.state.get()).symbols.bytes_of(symbol) }
    }

    pub(crate) fn vtable_count(&self) -> usize {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied count.
        unsafe { (*self.state.get()).vtables.count() }
    }

    pub(crate) fn vtable_at(&self, index: usize) -> Oop {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied identity.
        unsafe { (*self.state.get()).vtables.at(index) }
    }

    pub(crate) fn vtable_parent(&self, vtable: Oop) -> Oop {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied identity.
        unsafe { (*self.state.get()).vtables.parent_of(vtable) }
    }

    pub(crate) fn vtable_owner(&self, vtable: Oop) -> Oop {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied identity.
        unsafe { (*self.state.get()).vtables.owner_of(vtable) }
    }

    pub(crate) fn vtable_selector_at(&self, vtable: Oop, index: usize) -> Oop {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied identity.
        unsafe { (*self.state.get()).vtables.selector_at(vtable, index) }
    }

    pub(crate) fn checked_vtable_of(&self, object: Oop) -> Oop {
        self.assert_owner();
        // SAFETY: owner-thread serialization; checked lookup copies the header.
        unsafe { Vtables::checked_vtable_of(&(*self.state.get()).heap, object) }
    }

    /// Perform the same exact-membership vtable lookup after `enter` has already
    /// established this runtime as active on the owner thread.
    pub(crate) unsafe fn checked_vtable_of_active(&self, object: Oop) -> Oop {
        // SAFETY: the active-runtime guard proves owner-thread serialization;
        // checked_vtable_of still validates exact heap membership before reading.
        unsafe { Vtables::checked_vtable_of(&(*self.state.get()).heap, object) }
    }

    pub(crate) fn payload_size(&self, object: Oop) -> Option<usize> {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied size.
        unsafe { (*self.state.get()).heap.payload_size(object) }
    }

    pub(crate) fn gc_set_stack_bottom(&self, address: usize) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        unsafe { (&mut *self.state.get()).metadata.set_stack_bottom(address) };
    }

    pub(crate) fn gc_set_auto_requested(&self, enabled: bool) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        unsafe {
            (&mut *self.state.get())
                .metadata
                .set_auto_requested(enabled)
        };
    }

    pub(crate) fn gc_set_precise(&self, enabled: bool) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        unsafe { (&mut *self.state.get()).metadata.set_precise(enabled) };
    }

    pub(crate) fn gc_register_root_provider(&self, provider: crate::abi::RootProvider) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        unsafe {
            (&mut *self.state.get())
                .metadata
                .register_root_provider(provider)
        };
    }

    pub(crate) fn gc_collect(&self) -> usize {
        self.enter(|| self.gc_collect_active())
    }

    fn gc_collect_active(&self) -> usize {
        if self.collection_phase.get() != CollectionPhase::Idle {
            return 0;
        }
        // SAFETY: owner-thread serialization and a copied stack anchor.
        let stack_bottom = unsafe { (*self.state.get()).metadata.stack_bottom() };
        if stack_bottom == 0 {
            return 0;
        }
        with_native_root_range(stack_bottom, |root_start| {
            self.gc_collect_from_native_range(root_start, stack_bottom)
        })
        .unwrap_or(0)
    }

    fn gc_collect_from_native_range(&self, root_start: usize, stack_bottom: usize) -> usize {
        self.collection_phase.set(CollectionPhase::Marking);
        let collection_guard = CollectionGuard {
            phase: &self.collection_phase,
        };
        let mut marker = Marker::new();
        // End this state borrow before run_mark_phase can invoke foreign code.
        // SAFETY: owner-thread serialization and no callback in this scope.
        unsafe {
            let state = &mut *self.state.get();
            state.heap.clear_marks();
            mark_native_root_range(&mut state.heap, &mut marker, root_start, stack_bottom);
        }
        self.run_mark_phase(marker);
        // Snapshot before mutation, then detach each allocation so no registry
        // or RuntimeState borrow overlaps its foreign finalizer.
        // SAFETY: mark callbacks have returned and owner-thread serialization is
        // restored for the copied address list.
        let dead = unsafe { (*self.state.get()).heap.dead_collectable_addresses() };
        self.collection_phase.set(CollectionPhase::Finalizing);
        let mut reclaimed = 0;
        for address in dead {
            // SAFETY: no callback occurs while the short heap borrow is live.
            let detached = unsafe {
                (&mut *self.state.get())
                    .heap
                    .detach_dead_collectable(address)
            }
            .unwrap_or_else(|| boundary::fatal_invariant("dead GC block could not be detached"));
            let object = detached.object();
            if let Some(finalizer) = detached.finalizer() {
                // The detached payload remains live, but no Rust state borrow
                // crosses the call. Allocation is dynamically forbidden until
                // the complete sweep has finished.
                unsafe { finalizer(object) };
            }
            detached.reclaim();
            reclaimed += 1;
        }
        // SAFETY: all finalizers returned and the owner thread again has sole
        // access to the registry.
        unsafe {
            let heap = &mut (&mut *self.state.get()).heap;
            heap.clear_marks();
            heap.reset_allocation_bytes();
        }
        drop(collection_guard);
        reclaimed
    }

    pub(crate) fn gc_live_count(&self) -> usize {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied count.
        unsafe { (*self.state.get()).heap.allocation_count() }
    }

    fn run_mark_phase(&self, mut marker: Marker) {
        let state_pointer = self.state.get();
        // Copy callback authority and raw owner pointers, then end the state
        // borrow before the first foreign call.
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let providers = unsafe {
            let state = &mut *state_pointer;
            mark_floor_roots(state, &mut marker);
            state.metadata.root_providers()
        };

        for provider in providers {
            // Form callback pointers only for this invocation. Retaining a
            // field-derived pointer across a later RuntimeState borrow would
            // invalidate its stacked-borrows provenance.
            let heap_pointer = unsafe { core::ptr::addr_of_mut!((*state_pointer).heap) };
            let marker_pointer = core::ptr::addr_of_mut!(marker);
            // SAFETY: no Rust references to state.heap or marker survive this
            // call; the provider may use the mark function only synchronously.
            unsafe {
                invoke_with_mark_callback(heap_pointer, marker_pointer, |mark| provider(mark))
            };
        }

        while let Some(address) = marker.pop() {
            // Conservative scanning and hook lookup finish before foreign code.
            // SAFETY: owner-thread serialization and no callback in this scope.
            let (object, hook) = unsafe {
                let state = &mut *state_pointer;
                let object = state
                    .heap
                    .object_at_address(address)
                    .unwrap_or_else(|| boundary::fatal_invariant("marked GC block disappeared"));
                let vtable = state.heap.vtable_of(object).unwrap_or_else(|| {
                    boundary::fatal_invariant("marked GC block lost its vtable")
                });
                if state.metadata.precise() && state.vtables.has_layout(vtable) {
                    marker.trace_selected_payload(&mut state.heap, address, |word| {
                        state.vtables.slot_kind(vtable, word) == SLOT_OOP
                    });
                } else {
                    marker.trace_conservative_payload(&mut state.heap, address);
                }
                (object, state.vtables.trace_hook(vtable))
            };
            if let Some(hook) = hook {
                let heap_pointer = unsafe { core::ptr::addr_of_mut!((*state_pointer).heap) };
                let marker_pointer = core::ptr::addr_of_mut!(marker);
                // SAFETY: no Rust references to state.heap or marker survive
                // this call; the hook may mark only synchronously.
                unsafe {
                    invoke_with_mark_callback(heap_pointer, marker_pointer, |mark| {
                        hook(object, mark)
                    })
                };
            }
        }
    }

    pub(crate) fn set_trace_hook(&self, vtable: Oop, hook: crate::abi::TraceHook) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        unsafe {
            (&mut *self.state.get())
                .vtables
                .set_trace_hook(vtable, hook)
        };
    }

    pub(crate) fn set_finalizer(&self, object: Oop, finalizer: crate::abi::Finalizer) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let _ = unsafe {
            (&mut *self.state.get())
                .heap
                .set_finalizer(object, finalizer)
        };
    }

    pub(crate) fn set_layout(&self, vtable: Oop, kinds: *const u8, count: usize, rest: u8) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        unsafe {
            (&mut *self.state.get())
                .vtables
                .set_layout(vtable, kinds, count, rest)
        };
    }

    pub(crate) fn slot_kind(&self, vtable: Oop, word: usize) -> u8 {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied slot kind.
        unsafe { (*self.state.get()).vtables.slot_kind(vtable, word) }
    }

    #[cfg(test)]
    pub(crate) fn permanence_of(&self, object: Oop) -> Option<Permanence> {
        // SAFETY: test calls remain owner-thread and copy only the class.
        unsafe { (*self.state.get()).heap.permanence_of(object) }
    }

    pub(crate) fn integer_value(&self, object: Oop) -> i64 {
        self.assert_owner();
        // SAFETY: caller supplies an Integer object.
        unsafe { (*self.state.get()).values.integer_value(object) }
    }

    pub(crate) fn float_value(&self, object: Oop) -> f64 {
        self.assert_owner();
        // SAFETY: caller supplies a Float object.
        unsafe { (*self.state.get()).values.float_value(object) }
    }

    pub(crate) fn word_value(&self, object: Oop) -> u64 {
        self.assert_owner();
        // SAFETY: caller supplies a Word object.
        unsafe { (*self.state.get()).values.word_value(object) }
    }

    pub(crate) fn is_integer(&self, object: Oop) -> bool {
        self.has_canonical_vtable(object, |canonical| canonical.smallint_vt)
    }

    pub(crate) fn is_float(&self, object: Oop) -> bool {
        self.has_canonical_vtable(object, |canonical| canonical.float_vt)
    }

    pub(crate) fn is_word(&self, object: Oop) -> bool {
        self.has_canonical_vtable(object, |canonical| canonical.word_vt)
    }

    fn has_canonical_vtable(
        &self,
        object: Oop,
        select: impl FnOnce(CanonicalObjects) -> Oop,
    ) -> bool {
        self.assert_owner();
        // SAFETY: owner-thread serialization keeps both the exact-address heap
        // membership query and copied canonical identities in one state access.
        let state = unsafe { &*self.state.get() };
        let vtable = Vtables::checked_vtable_of(&state.heap, object);
        let canonical = state
            .canonical
            .unwrap_or_else(|| boundary::fatal_invariant("canonical objects not installed"));
        vtable == select(canonical)
    }

    pub(crate) fn string_bytes(&self, object: Oop) -> Option<&[u8]> {
        self.assert_owner();
        // SAFETY: caller supplies a String object and performs no mutation or
        // foreign call while inspecting the owned bytes.
        unsafe { (*self.state.get()).values.string_bytes(object) }
    }

    pub(crate) fn release_string(&self, object: Oop) {
        self.assert_owner();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let released = unsafe { (&mut *self.state.get()).values.release_string(object) };
        boundary::require_invariant(released, "String finalizer lost byte ownership");
    }

    #[cfg(test)]
    pub(crate) fn has_finalizer(&self, object: Oop) -> Option<bool> {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied metadata bit.
        unsafe { (*self.state.get()).heap.has_finalizer(object) }
    }

    #[cfg(test)]
    pub(crate) fn gc_mark_floor_roots_for_test(&self) {
        self.assert_owner();
        // SAFETY: test calls remain owner-thread and invoke no foreign code.
        unsafe { (&mut *self.state.get()).heap.clear_marks() };
        self.run_mark_phase(Marker::new());
    }

    #[cfg(test)]
    pub(crate) fn is_marked_for_test(&self, object: Oop) -> Option<bool> {
        self.assert_owner();
        // SAFETY: test calls remain owner-thread and copy one metadata bit.
        unsafe { (*self.state.get()).heap.is_marked(object) }
    }
}

fn mark_floor_roots(state: &mut RuntimeState, marker: &mut Marker) {
    let export_values = state.environment.root_values();
    let interned_symbols = (0..state.symbols.count())
        .map(|index| state.symbols.at(index))
        .collect::<Vec<_>>();
    for root in export_values.into_iter().chain(interned_symbols) {
        marker.mark_oop(&mut state.heap, root);
    }
}

fn intern_in_state(state: &mut RuntimeState, bytes: &[u8]) -> Result<Oop, EmbeddedNul> {
    let before = state.symbols.count();
    let symbol = state
        .symbols
        .intern(&mut state.heap, state.globals.symbol_vt, bytes)?;
    if state.symbols.count() != before
        && let Some((symbols, _)) = state.internal_dictionaries
    {
        state
            .vtables
            .add_opaque_binding(symbols, symbol, core::ptr::null_mut());
    }
    Ok(symbol)
}

static NEXT_THREAD_TOKEN: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static THREAD_TOKEN: u64 = {
        let token = NEXT_THREAD_TOKEN.fetch_add(1, Ordering::Relaxed);
        boundary::require_invariant(token != 0, "Rust floor thread identity space exhausted");
        token
    };
    static ACTIVE_RUNTIME: Cell<*const Runtime> = const { Cell::new(core::ptr::null()) };
}

fn current_thread_token() -> u64 {
    THREAD_TOKEN.with(|token| *token)
}

struct ActiveRuntimeGuard<'a> {
    active: &'a Cell<*const Runtime>,
    previous: *const Runtime,
}

impl Drop for ActiveRuntimeGuard<'_> {
    fn drop(&mut self) {
        self.active.set(self.previous);
    }
}

pub(crate) fn with_active<R>(operation: impl FnOnce(&Runtime) -> R) -> R {
    ACTIVE_RUNTIME.with(|active| {
        let runtime = active.get();
        boundary::require_invariant(!runtime.is_null(), "no active Rust floor runtime");
        // SAFETY: enter installs a pointer to a live owner-thread Runtime for
        // this dynamic extent, including synchronous reentrant callbacks.
        operation(unsafe { &*runtime })
    })
}

/// Run `operation` only when a surrounding `Runtime::enter` has already proved
/// owner-thread authority. The closure cannot return a borrow of the runtime,
/// so the TLS-installed pointer cannot escape its guarded dynamic extent.
pub(crate) fn with_active_if_present<R>(operation: impl FnOnce(&Runtime) -> R) -> Option<R> {
    ACTIVE_RUNTIME.with(|active| {
        let runtime = active.get();
        if runtime.is_null() {
            return None;
        }
        // SAFETY: enter installs this live pointer only after assert_owner and
        // restores the previous TLS value before its Runtime can go out of scope.
        Some(operation(unsafe { &*runtime }))
    })
}

#[cfg(test)]
mod tests {
    use super::{current_thread_token, with_active_if_present};
    use crate::abi::{Oop, SLOT_OOP};
    use crate::dispatch::build_bootstrap;
    use crate::execution::{
        CallSiteDescriptor, CallSiteKind, DescriptorIdentity, FRAME_LAYOUT_VERSION_V1,
        FrameDescriptor, FrameDescriptorSpec, SemanticRegistrationV1, SlotKind, StacklessTarget,
    };
    use crate::gc::Marker;
    use crate::heap::Permanence;
    use core::cell::Cell;

    const CONTINUE_PROBE_TRANSITIONS: usize = 1_000_000;

    thread_local! {
        static CONTINUE_PROBE_COUNT: Cell<usize> = const { Cell::new(0) };
        static CONTINUE_PROBE_SELECTOR: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
        static RETURN_PROBE_SELECTOR: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
        static PROBE_ZERO: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
        static PROBE_STACK_MIN: Cell<usize> = const { Cell::new(usize::MAX) };
        static PROBE_STACK_MAX: Cell<usize> = const { Cell::new(0) };
        static PROBE_FORCED_GC: Cell<bool> = const { Cell::new(false) };
        static CALL_SITE_PROBE_SELECTOR: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
        static TAIL_CALL_SITE_PROBE_SELECTOR: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
        static TAIL_PROBE_TRANSITIONS: Cell<usize> = const { Cell::new(0) };
        static TAIL_PROBE_STACK_MIN: Cell<usize> = const { Cell::new(usize::MAX) };
        static TAIL_PROBE_STACK_MAX: Cell<usize> = const { Cell::new(0) };
        static NESTED_PROBE_MACHINE: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
        static NESTED_PROBE_SELECTOR: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
        static NESTED_PROBE_DEPTH_BEFORE: Cell<usize> = const { Cell::new(0) };
        static NESTED_PROBE_DEPTH_AFTER: Cell<usize> = const { Cell::new(0) };
        static CALL_PROBE_FORCED_GC: Cell<bool> = const { Cell::new(false) };
        static REGISTERED_THUNK_CALLS: Cell<usize> = const { Cell::new(0) };
        static REGISTERED_FRAME_PIN_HIGH_WATER: Cell<usize> = const { Cell::new(0) };
        static REDEFINE_PROBE_SELECTOR: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
        static LATE_BOUND_RETURN_A_CALLS: Cell<usize> = const { Cell::new(0) };
        static LATE_BOUND_RETURN_B_CALLS: Cell<usize> = const { Cell::new(0) };
        static CUSTOM_LOOKUP_CALLS: Cell<usize> = const { Cell::new(0) };
        static CUSTOM_LOOKUP_SELECTOR: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
        static CUSTOM_LOOKUP_RESULT: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
        static RETAINED_STEP_ACTION: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
        static MICROCODE_REPLACEMENT_CALLS: Cell<usize> = const { Cell::new(0) };
        static MICROCODE_REPLACEMENT_MACHINE_HIGH_WATER: Cell<usize> = const { Cell::new(0) };
        static OUTER_ACTION_STALE_DURING_NESTED: Cell<bool> = const { Cell::new(false) };
        static OUTER_ACTION_STALE_AFTER_NESTED: Cell<bool> = const { Cell::new(false) };
        static SUPPRESSED_ENTRY_SELECTOR: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
        static SUPPRESSED_ENTRY_MACHINE_HIGH_WATER: Cell<usize> = const { Cell::new(0) };
        static MEASURE_LIMIT: Cell<usize> = const { Cell::new(0) };
        static MEASURE_COUNT: Cell<usize> = const { Cell::new(0) };
        static MEASURE_VALUE_SLOT: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    }

    unsafe extern "C" fn inert_finalizer(_: Oop) {}

    unsafe extern "C" fn inert_step(_: Oop, _: *const Oop, _: usize) -> Oop {
        core::ptr::null_mut()
    }

    unsafe extern "C" fn malformed_action_step(_: Oop, _: *const Oop, _: usize) -> Oop {
        core::ptr::null_mut()
    }

    unsafe extern "C" fn capture_return_action_step(
        machine: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        with_active_if_present(|runtime| {
            if count != 0 || !args.is_null() {
                return runtime.canonical().nil;
            }
            let slot = runtime.make_integer(0);
            let action = runtime.send_slice(machine, RETURN_PROBE_SELECTOR.get(), &[slot]);
            RETAINED_STEP_ACTION.set(action);
            action
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn return_retained_step_action(
        _: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        if count != 0 || !args.is_null() {
            return core::ptr::null_mut();
        }
        RETAINED_STEP_ACTION.get()
    }

    unsafe extern "C" fn stale_return_action_step(
        machine: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        with_active_if_present(|runtime| {
            if count != 0 || !args.is_null() {
                return runtime.canonical().nil;
            }
            let slot = runtime.make_integer(0);
            let action = runtime.send_slice(machine, RETURN_PROBE_SELECTOR.get(), &[slot]);
            let wrote = unsafe {
                (&mut *runtime.state.get())
                    .heap
                    .write_payload_word(action, 3, 0)
            };
            assert!(wrote, "stale-action probe rewrites the copied epoch word");
            action
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn wrong_slot_return_action_step(
        machine: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        with_active_if_present(|runtime| {
            if count != 0 || !args.is_null() {
                return runtime.canonical().nil;
            }
            let slot = runtime.make_integer(0);
            let action = runtime.send_slice(machine, RETURN_PROBE_SELECTOR.get(), &[slot]);
            let wrote = unsafe {
                (&mut *runtime.state.get())
                    .heap
                    .write_payload_word(action, 4, 1)
            };
            assert!(
                wrote,
                "wrong-slot probe rewrites only the copied slot index"
            );
            action
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn test_error_kind_factory(_: Oop, args: *const Oop, count: usize) -> Oop {
        if count != 1 || args.is_null() {
            return core::ptr::null_mut();
        }
        // SAFETY: the Method ABI promises one live argument for count == 1.
        unsafe { args.read() }
    }

    unsafe extern "C" fn million_continue_step(
        machine: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        with_active_if_present(|runtime| {
            if count != 0 || !args.is_null() {
                return runtime.canonical().nil;
            }
            let stack_probe = 0usize;
            let stack_address = core::ptr::addr_of!(stack_probe).addr();
            PROBE_STACK_MIN.set(PROBE_STACK_MIN.get().min(stack_address));
            PROBE_STACK_MAX.set(PROBE_STACK_MAX.get().max(stack_address));
            let transitions = CONTINUE_PROBE_COUNT.get();
            if transitions == 0 && !PROBE_FORCED_GC.replace(true) {
                let _ = runtime.gc_collect_from_native_range(0, 0);
                PROBE_ZERO.set(runtime.make_integer(0));
            }
            let operand = PROBE_ZERO.get();
            if transitions < CONTINUE_PROBE_TRANSITIONS {
                CONTINUE_PROBE_COUNT.set(transitions + 1);
                runtime.send_slice(machine, CONTINUE_PROBE_SELECTOR.get(), &[operand])
            } else {
                runtime.send_slice(machine, RETURN_PROBE_SELECTOR.get(), &[operand])
            }
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn measured_continue_step(
        machine: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        with_active_if_present(|runtime| {
            if count != 0 || !args.is_null() {
                return runtime.canonical().nil;
            }
            let completed = MEASURE_COUNT.get();
            if completed < MEASURE_LIMIT.get() {
                MEASURE_COUNT.set(completed + 1);
                runtime.send_slice(machine, CONTINUE_PROBE_SELECTOR.get(), &[PROBE_ZERO.get()])
            } else {
                runtime.send_slice(machine, RETURN_PROBE_SELECTOR.get(), &[PROBE_ZERO.get()])
            }
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn measured_synchronous_step(
        machine: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        with_active_if_present(|runtime| {
            if count != 1 || args.is_null() {
                return runtime.canonical().nil;
            }
            // SAFETY: this descriptor declares one resume-token argument.
            let pc = runtime.integer_value(unsafe { args.read() });
            if pc == 0 {
                runtime.send_slice(machine, CALL_SITE_PROBE_SELECTOR.get(), &[PROBE_ZERO.get()])
            } else {
                let completed = MEASURE_COUNT.get() + 1;
                MEASURE_COUNT.set(completed);
                if completed < MEASURE_LIMIT.get() {
                    runtime.send_slice(machine, CONTINUE_PROBE_SELECTOR.get(), &[PROBE_ZERO.get()])
                } else {
                    runtime.send_slice(
                        machine,
                        RETURN_PROBE_SELECTOR.get(),
                        &[MEASURE_VALUE_SLOT.get()],
                    )
                }
            }
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn recursive_drive_probe_step(
        machine: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        with_active_if_present(|runtime| {
            if count != 0 || !args.is_null() {
                return runtime.canonical().nil;
            }
            let capability = runtime
                .stackless_machine_capability(machine)
                .expect("recursive probe Machine validates");
            let (descriptor, live_value) = unsafe {
                let state = &*runtime.state.get();
                let machine_state = state
                    .execution
                    .machine(current_thread_token(), capability)
                    .expect("recursive probe Machine resolves");
                let frame = machine_state
                    .arena()
                    .current_handle()
                    .expect("recursive probe Frame is live");
                (
                    machine_state
                        .arena()
                        .descriptor_arc(frame)
                        .expect("recursive probe descriptor resolves"),
                    machine_state
                        .arena()
                        .read_oop(frame, 0)
                        .expect("recursive probe slot reads"),
                )
            };
            assert_eq!(
                runtime.drive_stackless(machine, descriptor, &[]),
                Err(crate::execution::ExecutionError::Machine(
                    crate::execution::MachineError::InvalidLifecycle
                )),
                "direct recursive drive is rejected without an entry barrier"
            );
            assert_eq!(
                runtime.stackless_oop_at(machine, 0),
                Ok(live_value),
                "recursive rejection leaves the active Frame untouched"
            );
            runtime.send_slice(machine, RETURN_PROBE_SELECTOR.get(), &[PROBE_ZERO.get()])
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn call_parent_step(machine: Oop, args: *const Oop, count: usize) -> Oop {
        with_active_if_present(|runtime| {
            if count != 1 || args.is_null() {
                return runtime.canonical().nil;
            }
            // SAFETY: this descriptor declares exactly one resume token.
            let token = unsafe { args.read() };
            if !runtime.is_integer(token) {
                return runtime.canonical().nil;
            }
            let program_counter = runtime.integer_value(token);
            let operand = runtime.make_integer(if program_counter == 2 {
                4
            } else {
                program_counter
            });
            let selector = if program_counter == 2 {
                RETURN_PROBE_SELECTOR.get()
            } else {
                CALL_SITE_PROBE_SELECTOR.get()
            };
            runtime.send_slice(machine, selector, &[operand])
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn call_child_step(machine: Oop, args: *const Oop, count: usize) -> Oop {
        with_active_if_present(|runtime| {
            if count != 0 || !args.is_null() {
                return runtime.canonical().nil;
            }
            if !CALL_PROBE_FORCED_GC.replace(true) {
                let _ = runtime.gc_collect_from_native_range(0, 0);
            }
            REGISTERED_FRAME_PIN_HIGH_WATER.set(
                REGISTERED_FRAME_PIN_HIGH_WATER.get().max(
                    runtime
                        .stackless_registration_diagnostics()
                        .active_frame_pins,
                ),
            );
            let slot = runtime.make_integer(1);
            runtime.send_slice(machine, RETURN_PROBE_SELECTOR.get(), &[slot])
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn synchronous_increment(_: Oop, args: *const Oop, count: usize) -> Oop {
        with_active_if_present(|runtime| {
            if count != 1 || args.is_null() {
                return runtime.canonical().nil;
            }
            // SAFETY: this synchronous probe requires one argument.
            let value = unsafe { args.read() };
            runtime.make_integer(runtime.integer_value(value) + 1)
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn synchronous_identity(_: Oop, args: *const Oop, count: usize) -> Oop {
        if count != 1 || args.is_null() {
            return core::ptr::null_mut();
        }
        // SAFETY: the probe requires one argument.
        unsafe { args.read() }
    }

    fn late_bound_return(receiver: Oop, args: *const Oop, count: usize) -> Oop {
        with_active_if_present(|runtime| {
            if count != 1 || args.is_null() {
                return runtime.canonical().nil;
            }
            // SAFETY: this microcode probe requires one live slot operand.
            let operand = unsafe { args.read() };
            if !runtime.is_integer(operand) {
                return runtime.canonical().nil;
            }
            let slot = runtime.integer_value(operand);
            let Ok(slot) = usize::try_from(slot) else {
                return runtime.canonical().nil;
            };
            let _ = runtime.gc_collect_from_native_range(0, 0);
            runtime
                .stackless_return_at(receiver, slot)
                .unwrap_or_else(|_| runtime.canonical().nil)
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn late_bound_return_a(receiver: Oop, args: *const Oop, count: usize) -> Oop {
        LATE_BOUND_RETURN_A_CALLS.set(LATE_BOUND_RETURN_A_CALLS.get() + 1);
        late_bound_return(receiver, args, count)
    }

    unsafe extern "C" fn late_bound_return_b(receiver: Oop, args: *const Oop, count: usize) -> Oop {
        LATE_BOUND_RETURN_B_CALLS.set(LATE_BOUND_RETURN_B_CALLS.get() + 1);
        late_bound_return(receiver, args, count)
    }

    unsafe extern "C" fn late_bound_custom_lookup(
        receiver: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        with_active_if_present(|runtime| {
            if count != 1 || args.is_null() {
                return runtime.canonical().nil;
            }
            // SAFETY: the lookup protocol supplies the requested selector.
            let selector = unsafe { args.read() };
            if selector == CUSTOM_LOOKUP_SELECTOR.get() {
                CUSTOM_LOOKUP_CALLS.set(CUSTOM_LOOKUP_CALLS.get() + 1);
                return CUSTOM_LOOKUP_RESULT.get();
            }
            // SAFETY: the custom lookup is invoked inside Runtime::enter.
            unsafe { runtime.lookup_method_active(receiver, selector) }
                .map(crate::raw::MethodAddress::as_oop)
                .unwrap_or(core::ptr::null_mut())
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn registered_probe_thunk(
        receiver: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        REGISTERED_THUNK_CALLS.set(REGISTERED_THUNK_CALLS.get() + 1);
        with_active_if_present(|runtime| {
            // SAFETY: this is an ordinary Method thunk forwarding its ABI.
            unsafe {
                runtime.enter_registered_resumable_or_fatal(
                    registered_probe_thunk,
                    receiver,
                    args,
                    count,
                )
            }
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn microcode_replacement_thunk(
        receiver: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        MICROCODE_REPLACEMENT_CALLS.set(MICROCODE_REPLACEMENT_CALLS.get() + 1);
        with_active_if_present(|runtime| {
            // SAFETY: this ordinary Method thunk forwards its live ABI buffer.
            unsafe {
                runtime.enter_registered_resumable_or_fatal(
                    microcode_replacement_thunk,
                    receiver,
                    args,
                    count,
                )
            }
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn suppressed_entry_thunk(
        receiver: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        with_active_if_present(|runtime| {
            // SAFETY: this ordinary Method thunk forwards its live ABI buffer.
            unsafe {
                runtime.enter_registered_resumable_or_fatal(
                    suppressed_entry_thunk,
                    receiver,
                    args,
                    count,
                )
            }
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn suppressed_entry_step(
        machine: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        with_active_if_present(|runtime| {
            if count != 0 || !args.is_null() {
                return runtime.canonical().nil;
            }
            SUPPRESSED_ENTRY_MACHINE_HIGH_WATER.set(
                SUPPRESSED_ENTRY_MACHINE_HIGH_WATER
                    .get()
                    .max(runtime.stackless_runtime_diagnostics().live_machines),
            );
            let slot = runtime.make_integer(1);
            runtime.send_slice(machine, RETURN_PROBE_SELECTOR.get(), &[slot])
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn synchronous_suppressed_entry(
        receiver: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        with_active_if_present(|runtime| {
            if count != 1 || args.is_null() {
                return runtime.canonical().nil;
            }
            // SAFETY: this synchronous probe requires one live argument.
            let argument = unsafe { args.read() };
            runtime.without_stackless_entry_scope(|| {
                runtime.send_slice(receiver, SUPPRESSED_ENTRY_SELECTOR.get(), &[argument])
            })
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn microcode_replacement_step(
        machine: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        with_active_if_present(|runtime| {
            if count != 0 || !args.is_null() {
                return runtime.canonical().nil;
            }
            let diagnostics = runtime.stackless_runtime_diagnostics();
            MICROCODE_REPLACEMENT_MACHINE_HIGH_WATER.set(
                MICROCODE_REPLACEMENT_MACHINE_HIGH_WATER
                    .get()
                    .max(diagnostics.live_machines),
            );
            let capability = runtime
                .stackless_machine_capability(machine)
                .expect("nested Machine capability validates");
            let stale = unsafe {
                let state = &*runtime.state.get();
                state.execution.decode_action(
                    &state.heap,
                    current_thread_token(),
                    capability,
                    RETAINED_STEP_ACTION.get(),
                )
            };
            OUTER_ACTION_STALE_DURING_NESTED.set(matches!(
                stale,
                Err(crate::execution::ExecutionError::StaleAction)
            ));
            let slot = runtime.make_integer(1);
            runtime.send_slice(machine, RETURN_PROBE_SELECTOR.get(), &[slot])
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn all_microcode_step(machine: Oop, args: *const Oop, count: usize) -> Oop {
        with_active_if_present(|runtime| {
            if count != 1 || args.is_null() {
                return runtime.canonical().nil;
            }
            // SAFETY: this descriptor declares one resume-token argument.
            let pc = runtime.integer_value(unsafe { args.read() });
            let index = runtime.make_integer(0);
            let value_slot = runtime.make_integer(2);
            if pc == 0 {
                let oop_at = runtime.intern(b"oopAt:").expect("static selector");
                let oop_at_put = runtime.intern(b"oopAt:put:").expect("static selector");
                let call_site = runtime.intern(b"callSite:").expect("static selector");
                let tail_call_site = runtime.intern(b"tailCallSite:").expect("static selector");
                let continue_at = runtime.intern(b"continueAt:").expect("static selector");
                let action_kind = runtime.intern(b"actionKind").expect("static selector");

                let outer_action = runtime.send_slice(machine, continue_at, &[index]);
                RETAINED_STEP_ACTION.set(outer_action);
                let replacement_answer = runtime.send_slice(machine, oop_at, &[index]);
                assert!(runtime.is_integer(replacement_answer));
                assert_eq!(runtime.integer_value(replacement_answer), 0);
                let capability = runtime
                    .stackless_machine_capability(machine)
                    .expect("outer Machine capability validates");
                let stale = unsafe {
                    let state = &*runtime.state.get();
                    state.execution.decode_action(
                        &state.heap,
                        current_thread_token(),
                        capability,
                        outer_action,
                    )
                };
                OUTER_ACTION_STALE_AFTER_NESTED.set(matches!(
                    stale,
                    Err(crate::execution::ExecutionError::StaleAction)
                ));

                let replacement_value = runtime.make_integer(211);
                assert_eq!(
                    runtime.send_slice(machine, oop_at_put, &[value_slot, replacement_value]),
                    replacement_value
                );
                let raw_slot = runtime.make_integer(5);
                assert_eq!(
                    runtime.send_slice(machine, oop_at_put, &[raw_slot, replacement_value]),
                    runtime.canonical().nil,
                    "wrong-slot-kind write is rejected"
                );

                let call_action = runtime.send_slice(machine, call_site, &[index]);
                assert_eq!(
                    runtime.send_slice(call_action, action_kind, &[]),
                    runtime.intern(b"call").expect("static symbol")
                );
                let tail_index = runtime.make_integer(1);
                let tail_action = runtime.send_slice(machine, tail_call_site, &[tail_index]);
                assert_eq!(
                    runtime.send_slice(tail_action, action_kind, &[]),
                    runtime.intern(b"tailCall").expect("static symbol")
                );
                let next_pc = runtime.make_integer(1);
                runtime.send_slice(machine, continue_at, &[next_pc])
            } else {
                runtime.send_slice(machine, RETURN_PROBE_SELECTOR.get(), &[value_slot])
            }
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn redefining_probe_thunk(
        receiver: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        with_active_if_present(|runtime| {
            // SAFETY: this is an ordinary Method thunk forwarding its ABI.
            unsafe {
                runtime.enter_registered_resumable_or_fatal(
                    redefining_probe_thunk,
                    receiver,
                    args,
                    count,
                )
            }
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn redefine_during_registered_step(
        machine: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        with_active_if_present(|runtime| {
            if count != 0 || !args.is_null() {
                return runtime.canonical().nil;
            }
            assert_eq!(
                runtime
                    .stackless_registration_diagnostics()
                    .active_frame_pins,
                1,
                "registered frame owns its generation pin before entry"
            );
            let _ = runtime.add_method(
                runtime.globals().object_vt,
                REDEFINE_PROBE_SELECTOR.get(),
                synchronous_identity,
            );
            let slot = runtime.make_integer(1);
            runtime.send_slice(machine, RETURN_PROBE_SELECTOR.get(), &[slot])
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn single_call_parent_step(
        machine: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        with_active_if_present(|runtime| {
            if count != 1 || args.is_null() {
                return runtime.canonical().nil;
            }
            // SAFETY: this descriptor declares exactly one resume token.
            let token = unsafe { args.read() };
            let pc = runtime.integer_value(token);
            let operand = runtime.make_integer(if pc == 0 { 0 } else { 3 });
            let selector = if pc == 0 {
                CALL_SITE_PROBE_SELECTOR.get()
            } else {
                RETURN_PROBE_SELECTOR.get()
            };
            runtime.send_slice(machine, selector, &[operand])
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn synchronous_nested_entry(
        receiver: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        with_active_if_present(|runtime| {
            if count != 1 || args.is_null() {
                return runtime.canonical().nil;
            }
            let machine = NESTED_PROBE_MACHINE.get();
            let capability = runtime
                .stackless_machine_capability(machine)
                .expect("scoped Machine validates during synchronous entry");
            let depth = unsafe {
                (*runtime.state.get())
                    .execution
                    .machine_diagnostics(current_thread_token(), capability)
                    .expect("boundary diagnostics resolve")
                    .current_synchronous_boundary_depth
            };
            NESTED_PROBE_DEPTH_BEFORE.set(depth);
            // SAFETY: the synchronous Method ABI supplies one live argument.
            let argument = unsafe { args.read() };
            let answer = runtime.send_slice(receiver, NESTED_PROBE_SELECTOR.get(), &[argument]);
            let depth = unsafe {
                (*runtime.state.get())
                    .execution
                    .machine_diagnostics(current_thread_token(), capability)
                    .expect("boundary diagnostics resolve")
                    .current_synchronous_boundary_depth
            };
            NESTED_PROBE_DEPTH_AFTER.set(depth);
            answer
        })
        .unwrap_or(core::ptr::null_mut())
    }

    fn alternating_tail_step(machine: Oop, args: *const Oop, count: usize) -> Oop {
        with_active_if_present(|runtime| {
            if count != 0 || !args.is_null() {
                return runtime.canonical().nil;
            }
            let stack_probe = 0usize;
            let stack_address = core::ptr::addr_of!(stack_probe).addr();
            TAIL_PROBE_STACK_MIN.set(TAIL_PROBE_STACK_MIN.get().min(stack_address));
            TAIL_PROBE_STACK_MAX.set(TAIL_PROBE_STACK_MAX.get().max(stack_address));
            let transitions = TAIL_PROBE_TRANSITIONS.get();
            let slot = runtime.make_integer(if transitions < CONTINUE_PROBE_TRANSITIONS {
                TAIL_PROBE_TRANSITIONS.set(transitions + 1);
                0
            } else {
                2
            });
            let selector = if transitions < CONTINUE_PROBE_TRANSITIONS {
                TAIL_CALL_SITE_PROBE_SELECTOR.get()
            } else {
                RETURN_PROBE_SELECTOR.get()
            };
            runtime.send_slice(machine, selector, &[slot])
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn alternating_tail_step_a(
        machine: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        alternating_tail_step(machine, args, count)
    }

    unsafe extern "C" fn alternating_tail_step_b(
        machine: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        alternating_tail_step(machine, args, count)
    }

    #[test]
    fn owner_tokens_are_stable_and_unique_across_threads() {
        let owner = current_thread_token();
        assert_eq!(current_thread_token(), owner);
        let other = std::thread::spawn(current_thread_token)
            .join()
            .expect("token thread completes");
        assert_ne!(other, owner);
    }

    #[test]
    fn nested_entry_reuses_and_restores_the_active_runtime() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        assert!(with_active_if_present(|_| ()).is_none());

        runtime.enter(|| {
            assert!(
                with_active_if_present(|active| core::ptr::eq(active, runtime)).unwrap_or(false)
            );
            runtime.enter(|| {
                assert!(
                    with_active_if_present(|active| core::ptr::eq(active, runtime))
                        .unwrap_or(false)
                );
            });
            assert!(
                with_active_if_present(|active| core::ptr::eq(active, runtime)).unwrap_or(false)
            );
        });

        assert!(with_active_if_present(|_| ()).is_none());
    }

    #[test]
    fn active_allocations_preserve_instance_and_string_metadata() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let globals = runtime.globals();
        let canonical = runtime.canonical();

        runtime.enter(|| {
            // SAFETY: Runtime::enter established owner-thread authority for
            // this exact runtime for the complete dynamic extent.
            let instance = unsafe { runtime.allocate_instance_active(globals.object_vt, 9) };
            // SAFETY: same active-runtime authority as above.
            let string =
                unsafe { runtime.make_string_with_finalizer_active(b"Birnam", inert_finalizer) };

            assert_eq!(
                runtime.payload_size(instance),
                Some(2 * core::mem::size_of::<usize>()),
                "active allocation retains the floor's word-padded extent"
            );
            assert_eq!(
                runtime.checked_vtable_of(instance),
                globals.object_vt,
                "active instance allocation retains its requested vtable"
            );
            assert_eq!(runtime.string_bytes(string), Some(&b"Birnam"[..]));
            assert_eq!(runtime.checked_vtable_of(string), canonical.string_vt);
            assert_eq!(runtime.has_finalizer(string), Some(true));
        });
    }

    #[test]
    fn reachable_machine_traces_only_live_frame_slots_and_scratch_actions() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        let canonical = runtime.canonical();
        let globals = runtime.globals();
        let machine = runtime
            .create_stackless_machine()
            .expect("stackless Machine creates");
        let capability = runtime
            .stackless_machine_capability(machine)
            .expect("Machine capability validates");
        let rooted = runtime.allocate_instance(globals.object_vt, 0);
        let popped_only = runtime.allocate_instance(globals.object_vt, 0);
        let descriptor = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 1,
                generation: 1,
                source_map: 1,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 1,
            driver_argument_count: 0,
            slot_kinds: vec![SlotKind::Oop, SlotKind::Raw, SlotKind::Oop],
            call_sites: Vec::new(),
            step_method: inert_step,
        })
        .expect("integration descriptor validates");

        let (frame, continue_action, call_action, tail_call_action, return_action) = unsafe {
            let state = &mut *runtime.state.get();
            let machine_state = state
                .execution
                .machine_mut(current_thread_token(), capability)
                .expect("Machine state resolves");
            machine_state
                .begin_initial_drive(current_thread_token())
                .expect("initial drive begins");
            let frame = machine_state
                .arena_mut()
                .push(
                    descriptor.clone(),
                    0,
                    canonical.nil,
                    &[(0, rooted), (2, popped_only)],
                )
                .expect("frame push initializes roots");
            let actions = machine_state.scratch_actions();
            (frame, actions.0, actions.1, actions.2, actions.3)
        };

        let marker = unsafe {
            let state = &mut *runtime.state.get();
            state.heap.clear_marks();
            let mut marker = Marker::new();
            marker.mark_oop(&mut state.heap, machine);
            marker
        };
        runtime.enter(|| runtime.run_mark_phase(marker));
        assert_eq!(runtime.is_marked_for_test(rooted), Some(true));
        assert_eq!(runtime.is_marked_for_test(popped_only), Some(true));
        assert_eq!(runtime.is_marked_for_test(continue_action), Some(true));
        assert_eq!(runtime.is_marked_for_test(call_action), Some(true));
        assert_eq!(runtime.is_marked_for_test(tail_call_action), Some(true));
        assert_eq!(runtime.is_marked_for_test(return_action), Some(true));

        let marker = unsafe {
            let state = &mut *runtime.state.get();
            let machine_state = state
                .execution
                .machine_mut(current_thread_token(), capability)
                .expect("Machine state remains live");
            assert_eq!(machine_state.arena_mut().pop(), Ok(frame));
            state.heap.clear_marks();
            let mut marker = Marker::new();
            marker.mark_oop(&mut state.heap, machine);
            marker
        };
        runtime.enter(|| runtime.run_mark_phase(marker));
        assert_eq!(runtime.is_marked_for_test(rooted), Some(false));
        assert_eq!(runtime.is_marked_for_test(popped_only), Some(false));
        assert_eq!(runtime.is_marked_for_test(continue_action), Some(true));
        assert_eq!(runtime.is_marked_for_test(call_action), Some(true));
        assert_eq!(runtime.is_marked_for_test(tail_call_action), Some(true));
        assert_eq!(runtime.is_marked_for_test(return_action), Some(true));
    }

    #[test]
    fn unreachable_machine_finalization_invalidates_capability_exactly_once() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        let machine = runtime
            .create_stackless_machine()
            .expect("stackless Machine creates");
        let capability = runtime
            .stackless_machine_capability(machine)
            .expect("Machine capability validates before collection");

        let reclaimed = runtime.enter(|| runtime.gc_collect_from_native_range(0, 0));
        assert!(
            reclaimed >= 3,
            "Machine and its scratch actions are reclaimed"
        );
        unsafe {
            let state = &mut *runtime.state.get();
            assert!(matches!(
                state.execution.machine(current_thread_token(), capability),
                Err(crate::execution::MachineError::StaleCapability)
            ));
        }
    }

    #[test]
    fn exported_machine_new_creates_checked_capability_and_clone_is_disabled() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        let prototype = runtime.import(b"Machine").expect("static name has no NUL");
        let new_selector = runtime.intern(b"new").expect("static selector has no NUL");
        let clone_selector = runtime
            .intern(b"clone")
            .expect("static selector has no NUL");

        let machine = runtime.enter(|| runtime.send_slice(prototype, new_selector, &[]));
        runtime
            .stackless_machine_capability(machine)
            .expect("Machine new answers a checked capability");
        let clone = runtime.enter(|| runtime.send_slice(machine, clone_selector, &[]));
        assert_eq!(clone, runtime.canonical().nil);
    }

    #[test]
    fn frame_view_reflects_copied_data_traces_machine_and_rejects_reused_slot() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        let globals = runtime.globals();
        let canonical = runtime.canonical();

        // A minimal factory exposes which kind the primitive requested. The
        // real Error rung separately owns the ErrorVt representation.
        let error_factory_vtable = runtime.create_vtable(globals.object_vt);
        let error_factory = runtime.allocate_empty(error_factory_vtable, Permanence::Permanent);
        runtime.set_vtable_owner(error_factory_vtable, error_factory);
        runtime.set_layout(
            error_factory_vtable,
            core::ptr::null(),
            0,
            crate::abi::SLOT_RAW,
        );
        let kind_selector = runtime.intern(b"kind:").expect("static selector");
        let _ = runtime.add_method(error_factory_vtable, kind_selector, test_error_kind_factory);
        runtime
            .export(b"Error", error_factory)
            .expect("static export name");

        let machine = runtime
            .create_stackless_machine()
            .expect("stackless Machine creates");
        let capability = runtime
            .stackless_machine_capability(machine)
            .expect("Machine capability validates");
        let selected = runtime.allocate_instance(globals.object_vt, 0);
        let descriptor = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 101,
                generation: 17,
                source_map: 29,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 2,
            driver_argument_count: 1,
            slot_kinds: vec![SlotKind::Oop, SlotKind::Raw],
            call_sites: Vec::new(),
            step_method: inert_step,
        })
        .expect("FrameView descriptor validates");
        let nested = unsafe {
            let state = &mut *runtime.state.get();
            let machine_state = state
                .execution
                .machine_mut(current_thread_token(), capability)
                .expect("Machine state remains live");
            machine_state
                .begin_initial_drive(current_thread_token())
                .expect("test drive begins");
            machine_state
                .arena_mut()
                .push(descriptor.clone(), 0, canonical.nil, &[])
                .expect("outer frame pushes");
            machine_state
                .arena_mut()
                .push(descriptor.clone(), 1, canonical.nil, &[(0, selected)])
                .expect("nested frame pushes")
        };
        let current_frame_view = runtime
            .intern(b"currentFrameView")
            .expect("static selector");
        let view = runtime.enter(|| runtime.send_slice(machine, current_frame_view, &[]));
        let objects = unsafe { (*runtime.state.get()).execution.objects() }
            .expect("execution objects installed");
        assert_eq!(runtime.checked_vtable_of(view), objects.frame_view_vtable);
        assert_eq!(runtime.slot_kind(objects.frame_view_vtable, 0), SLOT_OOP);
        assert_eq!(
            runtime.slot_kind(objects.frame_view_vtable, 1),
            crate::abi::SLOT_RAW
        );

        let implementation_identity = runtime
            .intern(b"implementationIdentity")
            .expect("static selector");
        let generation_identity = runtime
            .intern(b"generationIdentity")
            .expect("static selector");
        let method_identity = runtime.intern(b"methodIdentity").expect("static selector");
        let source_map_identity = runtime
            .intern(b"sourceMapIdentity")
            .expect("static selector");
        let source_map_location = runtime
            .intern(b"sourceMapLocation")
            .expect("static selector");
        let program_counter = runtime.intern(b"programCounter").expect("static selector");
        let slot_count = runtime.intern(b"slotCount").expect("static selector");
        let slot_kind_at = runtime.intern(b"slotKindAt:").expect("static selector");
        let oop_at = runtime.intern(b"oopAt:").expect("static selector");
        let zero = runtime.make_integer(0);
        let one = runtime.make_integer(1);
        runtime.enter(|| {
            let implementation = runtime.send_slice(view, implementation_identity, &[]);
            let generation = runtime.send_slice(view, generation_identity, &[]);
            let method = runtime.send_slice(view, method_identity, &[]);
            let source_map = runtime.send_slice(view, source_map_identity, &[]);
            assert_eq!(runtime.word_value(implementation), 101);
            assert_eq!(runtime.word_value(generation), 17);
            assert_eq!(runtime.word_value(method), 101);
            assert_eq!(runtime.word_value(source_map), 29);
            assert_eq!(
                runtime.integer_value(runtime.send_slice(view, source_map_location, &[])),
                1
            );
            assert_eq!(
                runtime.integer_value(runtime.send_slice(view, program_counter, &[])),
                1
            );
            assert_eq!(
                runtime.integer_value(runtime.send_slice(view, slot_count, &[])),
                2
            );
            assert_eq!(
                runtime.send_slice(view, slot_kind_at, &[zero]),
                runtime.intern(b"oop").expect("static symbol")
            );
            assert_eq!(
                runtime.send_slice(view, slot_kind_at, &[one]),
                runtime.intern(b"raw").expect("static symbol")
            );
            assert_eq!(runtime.send_slice(view, oop_at, &[zero]), selected);
            assert_eq!(
                runtime.send_slice(view, oop_at, &[one]),
                runtime.intern(b"invalidFrameSlot").expect("static symbol")
            );
        });

        let marker = unsafe {
            let state = &mut *runtime.state.get();
            state.heap.clear_marks();
            let mut marker = Marker::new();
            marker.mark_oop(&mut state.heap, view);
            marker
        };
        runtime.enter(|| runtime.run_mark_phase(marker));
        assert_eq!(runtime.is_marked_for_test(machine), Some(true));
        assert_eq!(runtime.is_marked_for_test(selected), Some(true));

        let replacement = unsafe {
            let state = &mut *runtime.state.get();
            let machine_state = state
                .execution
                .machine_mut(current_thread_token(), capability)
                .expect("Machine state remains live");
            assert_eq!(machine_state.arena_mut().pop(), Ok(nested));
            machine_state
                .arena_mut()
                .push(descriptor, 0, canonical.nil, &[])
                .expect("outer call reuses nested arena offset")
        };
        assert_eq!(nested.words().unwrap()[1], replacement.words().unwrap()[1]);
        assert_ne!(nested.words().unwrap()[2], replacement.words().unwrap()[2]);
        let stale_kind = runtime.intern(b"staleFrameView").expect("static symbol");
        runtime.enter(|| {
            assert_eq!(
                runtime.send_slice(view, program_counter, &[]),
                stale_kind,
                "the retained view cannot alias the reused arena slot"
            );
        });

        unsafe {
            let state = &mut *runtime.state.get();
            let machine_state = state
                .execution
                .machine_mut(current_thread_token(), capability)
                .expect("Machine state remains live");
            assert_eq!(machine_state.arena_mut().pop(), Ok(replacement));
            machine_state.arena_mut().pop().expect("outer frame pops");
            machine_state
                .complete_initial_drive(current_thread_token())
                .expect("test drive completes");
        }
        runtime
            .destroy_stackless_machine(machine)
            .expect("completed Machine destroys");
    }

    #[test]
    fn handwritten_stepmethod_runs_one_million_transitions_without_stack_or_action_growth() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        let machine = runtime
            .create_stackless_machine()
            .expect("stackless Machine creates");
        let expected = runtime.make_integer(42);
        let zero = runtime.make_integer(0);
        CONTINUE_PROBE_COUNT.set(0);
        CONTINUE_PROBE_SELECTOR.set(
            runtime
                .intern(b"continueAt:")
                .expect("static selector contains no NUL"),
        );
        RETURN_PROBE_SELECTOR.set(
            runtime
                .intern(b"returnAt:")
                .expect("static selector contains no NUL"),
        );
        PROBE_ZERO.set(zero);
        PROBE_STACK_MIN.set(usize::MAX);
        PROBE_STACK_MAX.set(0);
        PROBE_FORCED_GC.set(false);
        let descriptor = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 2,
                generation: 1,
                source_map: 2,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 1,
            driver_argument_count: 0,
            slot_kinds: vec![SlotKind::Oop],
            call_sites: Vec::new(),
            step_method: million_continue_step,
        })
        .expect("single-entry probe descriptor validates");
        let live_before = runtime.gc_live_count();

        let answer = runtime
            .drive_stackless(machine, descriptor, &[(0, expected)])
            .expect("hand-written StepMethod completes");

        assert_eq!(answer, expected);
        assert_eq!(CONTINUE_PROBE_COUNT.get(), CONTINUE_PROBE_TRANSITIONS);
        assert!(PROBE_FORCED_GC.get(), "probe forced collection mid-drive");
        assert_eq!(
            runtime.gc_live_count(),
            live_before,
            "steady-state Continue and Return reuse Machine scratch actions"
        );
        assert!(
            PROBE_STACK_MAX.get().saturating_sub(PROBE_STACK_MIN.get()) <= 4096,
            "driver-loop transitions keep bounded native-stack movement"
        );
        let capability = runtime
            .stackless_machine_capability(machine)
            .expect("completed Machine capability remains inspectable");
        let (continue_action, _, _, return_action) = unsafe {
            let state = &*runtime.state.get();
            let machine_state = state
                .execution
                .machine(current_thread_token(), capability)
                .expect("completed Machine state remains live");
            assert_eq!(
                machine_state.lifecycle(),
                crate::execution::MachineLifecycle::Completed
            );
            assert_eq!(machine_state.arena().diagnostics().live_frames, 0);
            assert_eq!(
                machine_state.arena().diagnostics().high_water_live_frames,
                1
            );
            machine_state.scratch_actions()
        };
        let action_kind = runtime
            .intern(b"actionKind")
            .expect("static selector contains no NUL");
        let schema_version = runtime
            .intern(b"schemaVersion")
            .expect("static selector contains no NUL");
        let value_slot = runtime
            .intern(b"valueSlot")
            .expect("static selector contains no NUL");
        let continue_kind = runtime
            .intern(b"continue")
            .expect("static symbol contains no NUL");
        let return_kind = runtime
            .intern(b"return")
            .expect("static symbol contains no NUL");
        let live_frame_count = runtime
            .intern(b"liveFrameCount")
            .expect("static selector contains no NUL");
        let high_water_live_frame_count = runtime
            .intern(b"highWaterLiveFrameCount")
            .expect("static selector contains no NUL");
        let reserved_arena_bytes = runtime
            .intern(b"reservedArenaBytes")
            .expect("static selector contains no NUL");
        runtime.enter(|| {
            assert_eq!(
                runtime.send_slice(continue_action, action_kind, &[]),
                continue_kind
            );
            assert_eq!(
                runtime.send_slice(return_action, action_kind, &[]),
                return_kind
            );
            let schema = runtime.send_slice(return_action, schema_version, &[]);
            assert!(runtime.is_integer(schema));
            assert_eq!(runtime.integer_value(schema), 1);
            let slot = runtime.send_slice(return_action, value_slot, &[]);
            assert!(runtime.is_integer(slot));
            assert_eq!(runtime.integer_value(slot), 0);
            let live = runtime.send_slice(machine, live_frame_count, &[]);
            assert!(runtime.is_integer(live));
            assert_eq!(runtime.integer_value(live), 0);
            let high_water = runtime.send_slice(machine, high_water_live_frame_count, &[]);
            assert!(runtime.is_integer(high_water));
            assert_eq!(runtime.integer_value(high_water), 1);
            let reserved = runtime.send_slice(machine, reserved_arena_bytes, &[]);
            assert!(runtime.is_integer(reserved));
            assert!(runtime.integer_value(reserved) > 0);
        });
        let before_destroy = runtime.stackless_runtime_diagnostics();
        assert_eq!(before_destroy.live_machines, 1);
        assert_eq!(before_destroy.live_frames, 0);
        assert!(before_destroy.reserved_arena_bytes > 0);
        runtime
            .destroy_stackless_machine(machine)
            .expect("completed Machine explicitly destroys");
        let after_destroy = runtime.stackless_runtime_diagnostics();
        assert_eq!(after_destroy.live_machines, 0);
        assert_eq!(after_destroy.live_frames, 0);
        assert_eq!(after_destroy.reserved_arena_bytes, 0);
        assert_eq!(
            runtime.stackless_machine_capability(machine),
            Err(crate::execution::MachineError::StaleCapability)
        );
    }

    #[test]
    fn direct_recursive_drive_is_rejected_before_frame_mutation() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        RETURN_PROBE_SELECTOR.set(runtime.intern(b"returnAt:").expect("static selector"));
        PROBE_ZERO.set(runtime.make_integer(0));
        let expected = runtime.make_integer(149);
        let descriptor = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 203,
                generation: 1,
                source_map: 203,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 1,
            driver_argument_count: 0,
            slot_kinds: vec![SlotKind::Oop],
            call_sites: Vec::new(),
            step_method: recursive_drive_probe_step,
        })
        .expect("recursive-drive descriptor validates");
        let machine = runtime
            .create_stackless_machine()
            .expect("recursive-drive Machine creates");

        assert_eq!(
            runtime
                .drive_stackless(machine, descriptor, &[(0, expected)])
                .expect("outer drive completes after recursive rejection"),
            expected
        );
        let capability = runtime
            .stackless_machine_capability(machine)
            .expect("completed Machine remains inspectable");
        let diagnostics = unsafe {
            (*runtime.state.get())
                .execution
                .machine_diagnostics(current_thread_token(), capability)
                .expect("recursive-drive diagnostics resolve")
        };
        assert_eq!(diagnostics.high_water_live_frames, 1);
        assert_eq!(diagnostics.live_frames, 0);
    }

    #[test]
    #[ignore = "Release-mode S1 publication probe; run with BIRNAM_S_NS from the same micro suite"]
    fn publish_stackless_s1_transition_measurements() {
        fn median_ns(mut samples: Vec<f64>) -> f64 {
            samples.sort_by(f64::total_cmp);
            samples[samples.len() / 2]
        }

        const ITERATIONS: usize = 200_000;
        const SAMPLES: usize = 5;
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        CONTINUE_PROBE_SELECTOR.set(runtime.intern(b"continueAt:").expect("static selector"));
        CALL_SITE_PROBE_SELECTOR.set(runtime.intern(b"callSite:").expect("static selector"));
        RETURN_PROBE_SELECTOR.set(runtime.intern(b"returnAt:").expect("static selector"));
        PROBE_ZERO.set(runtime.make_integer(0));
        MEASURE_VALUE_SLOT.set(runtime.make_integer(3));
        MEASURE_LIMIT.set(ITERATIONS);

        let continue_descriptor = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 201,
                generation: 1,
                source_map: 201,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 1,
            driver_argument_count: 0,
            slot_kinds: vec![SlotKind::Oop],
            call_sites: Vec::new(),
            step_method: measured_continue_step,
        })
        .expect("measurement Continue descriptor validates");

        let synchronous_selector = runtime
            .intern(b"measuredSynchronousIdentity:")
            .expect("static selector");
        let _ = runtime.add_method(
            runtime.globals().object_vt,
            synchronous_selector,
            synchronous_identity,
        );
        let synchronous_descriptor = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 202,
                generation: 1,
                source_map: 202,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 2,
            driver_argument_count: 1,
            slot_kinds: vec![SlotKind::Oop; 4],
            call_sites: vec![CallSiteDescriptor {
                kind: CallSiteKind::Call,
                receiver_slot: 0,
                selector_slot: 1,
                argument_start: 2,
                argument_count: 1,
                result_slot: Some(3),
                continuation_pc: Some(1),
            }],
            step_method: measured_synchronous_step,
        })
        .expect("measurement synchronous descriptor validates");
        let receiver = runtime.canonical().object_prototype;
        let argument = runtime.make_integer(41);
        let mut continue_samples = Vec::with_capacity(SAMPLES);
        let mut synchronous_samples = Vec::with_capacity(SAMPLES);

        for _ in 0..SAMPLES {
            let machine = runtime
                .create_stackless_machine()
                .expect("Continue measurement Machine creates");
            MEASURE_COUNT.set(0);
            let started = std::time::Instant::now();
            let answer = runtime
                .drive_stackless(machine, continue_descriptor.clone(), &[(0, argument)])
                .expect("Continue measurement completes");
            let elapsed = started.elapsed();
            std::hint::black_box(answer);
            assert_eq!(MEASURE_COUNT.get(), ITERATIONS);
            continue_samples.push(elapsed.as_nanos() as f64 / ITERATIONS as f64);
            runtime
                .destroy_stackless_machine(machine)
                .expect("Continue measurement Machine destroys");

            let machine = runtime
                .create_stackless_machine()
                .expect("synchronous measurement Machine creates");
            MEASURE_COUNT.set(0);
            let started = std::time::Instant::now();
            let answer = runtime
                .drive_stackless(
                    machine,
                    synchronous_descriptor.clone(),
                    &[(0, receiver), (1, synchronous_selector), (2, argument)],
                )
                .expect("synchronous measurement completes");
            let elapsed = started.elapsed();
            std::hint::black_box(answer);
            assert_eq!(MEASURE_COUNT.get(), ITERATIONS);
            synchronous_samples.push(elapsed.as_nanos() as f64 / ITERATIONS as f64);
            runtime
                .destroy_stackless_machine(machine)
                .expect("synchronous measurement Machine destroys");
        }

        let continue_ns = median_ns(continue_samples);
        let synchronous_ns = median_ns(synchronous_samples);
        let baseline = std::env::var("BIRNAM_S_NS")
            .expect("set BIRNAM_S_NS to same-run sends.compiled-unary ns/op")
            .parse::<f64>()
            .expect("BIRNAM_S_NS is numeric");
        let continue_ratio = continue_ns / baseline;
        let synchronous_ratio = synchronous_ns / baseline;
        println!(
            "STACKLESS_S1_MEASUREMENTS baseline_s_ns={baseline:.3} continue_ns={continue_ns:.3} continue_ratio={continue_ratio:.3} synchronous_resolve_and_leaf_ns={synchronous_ns:.3} synchronous_ratio={synchronous_ratio:.3} microcode_sends_continue=1 microcode_sends_call=1 microcode_sends_return=1 iterations={ITERATIONS} samples={SAMPLES}"
        );
    }

    #[test]
    fn resumable_call_returns_into_waiting_frame_then_adapts_synchronous_method() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        CALL_SITE_PROBE_SELECTOR.set(
            runtime
                .intern(b"callSite:")
                .expect("static selector contains no NUL"),
        );
        RETURN_PROBE_SELECTOR.set(
            runtime
                .intern(b"returnAt:")
                .expect("static selector contains no NUL"),
        );
        let child_selector = runtime
            .intern(b"s1Child:")
            .expect("static selector contains no NUL");
        let synchronous_selector = runtime
            .intern(b"s1Increment:")
            .expect("static selector contains no NUL");
        let receiver = runtime.canonical().object_prototype;
        let input = runtime.make_integer(41);
        CALL_PROBE_FORCED_GC.set(false);
        let machine = runtime
            .create_stackless_machine()
            .expect("stackless Machine creates");
        let child = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 12,
                generation: 1,
                source_map: 12,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 1,
            driver_argument_count: 0,
            slot_kinds: vec![SlotKind::Oop, SlotKind::Oop],
            call_sites: Vec::new(),
            step_method: call_child_step,
        })
        .expect("child descriptor validates");
        let parent = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 11,
                generation: 1,
                source_map: 11,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 3,
            driver_argument_count: 1,
            slot_kinds: vec![SlotKind::Oop; 6],
            call_sites: vec![
                CallSiteDescriptor {
                    kind: CallSiteKind::Call,
                    receiver_slot: 0,
                    selector_slot: 1,
                    argument_start: 2,
                    argument_count: 1,
                    result_slot: Some(3),
                    continuation_pc: Some(1),
                },
                CallSiteDescriptor {
                    kind: CallSiteKind::Call,
                    receiver_slot: 0,
                    selector_slot: 5,
                    argument_start: 3,
                    argument_count: 1,
                    result_slot: Some(4),
                    continuation_pc: Some(2),
                },
            ],
            step_method: call_parent_step,
        })
        .expect("parent descriptor validates");

        let answer = runtime
            .drive_stackless_with_resolver(
                machine,
                parent,
                &[
                    (0, receiver),
                    (1, child_selector),
                    (2, input),
                    (5, synchronous_selector),
                ],
                None,
                |_, resolved_receiver, selector, arguments| {
                    assert_eq!(resolved_receiver, receiver);
                    assert_eq!(arguments.len(), 1);
                    if selector == child_selector {
                        Ok(StacklessTarget::Resumable {
                            descriptor: child.clone(),
                            initial_oops: vec![(0, resolved_receiver), (1, arguments[0])],
                            entry: None,
                        })
                    } else if selector == synchronous_selector {
                        Ok(StacklessTarget::Synchronous(synchronous_increment))
                    } else {
                        panic!("unexpected probe selector")
                    }
                },
            )
            .expect("resumable and synchronous calls complete");

        assert!(runtime.is_integer(answer));
        assert_eq!(runtime.integer_value(answer), 42);
        assert!(CALL_PROBE_FORCED_GC.get());
        let capability = runtime
            .stackless_machine_capability(machine)
            .expect("completed Machine remains inspectable");
        let diagnostics = unsafe {
            (*runtime.state.get())
                .execution
                .machine_diagnostics(current_thread_token(), capability)
                .expect("diagnostics remain available")
        };
        assert_eq!(diagnostics.live_frames, 0);
        assert_eq!(diagnostics.high_water_live_frames, 2);
        assert_eq!(diagnostics.current_synchronous_boundary_depth, 0);
        assert_eq!(diagnostics.high_water_synchronous_boundary_depth, 1);
        let call_action = unsafe {
            (*runtime.state.get())
                .execution
                .machine(current_thread_token(), capability)
                .expect("completed Machine remains live")
                .scratch_actions()
                .1
        };
        let action_kind = runtime.intern(b"actionKind").expect("static selector");
        let call_kind = runtime.intern(b"call").expect("static symbol");
        for (name, expected) in [
            (&b"receiverSlot"[..], 0),
            (&b"selectorSlot"[..], 5),
            (&b"argumentStart"[..], 3),
            (&b"argumentCount"[..], 1),
            (&b"resultSlot"[..], 4),
        ] {
            let selector = runtime.intern(name).expect("static selector");
            let value = runtime.enter(|| runtime.send_slice(call_action, selector, &[]));
            assert!(runtime.is_integer(value));
            assert_eq!(runtime.integer_value(value), expected);
        }
        assert_eq!(
            runtime.enter(|| runtime.send_slice(call_action, action_kind, &[])),
            call_kind
        );
    }

    #[test]
    fn synchronous_entry_nests_above_barrier_and_restores_outer_drive() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        CALL_SITE_PROBE_SELECTOR.set(runtime.intern(b"callSite:").expect("static selector"));
        RETURN_PROBE_SELECTOR.set(runtime.intern(b"returnAt:").expect("static selector"));
        let synchronous_selector = runtime.intern(b"s1Nested:").expect("static selector");
        let nested_selector = runtime
            .intern(b"registeredNested:")
            .expect("static selector");
        let receiver = runtime.canonical().object_prototype;
        let expected = runtime.make_integer(61);
        CALL_PROBE_FORCED_GC.set(false);
        let nested = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 32,
                generation: 1,
                source_map: 32,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 1,
            driver_argument_count: 0,
            slot_kinds: vec![SlotKind::Oop, SlotKind::Oop],
            call_sites: Vec::new(),
            step_method: call_child_step,
        })
        .expect("nested descriptor validates");
        let outer = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 31,
                generation: 1,
                source_map: 31,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 2,
            driver_argument_count: 1,
            slot_kinds: vec![SlotKind::Oop; 4],
            call_sites: vec![CallSiteDescriptor {
                kind: CallSiteKind::Call,
                receiver_slot: 0,
                selector_slot: 1,
                argument_start: 2,
                argument_count: 1,
                result_slot: Some(3),
                continuation_pc: Some(1),
            }],
            step_method: single_call_parent_step,
        })
        .expect("outer descriptor validates");
        let machine = runtime
            .create_stackless_machine()
            .expect("stackless Machine creates");

        assert_eq!(
            runtime.drive_stackless_nested_with_resolver(
                machine,
                nested.clone(),
                &[(0, receiver), (1, expected)],
                None,
                |_, _, _, _| Ok(StacklessTarget::Unbound),
            ),
            Err(crate::execution::ExecutionError::Machine(
                crate::execution::MachineError::InvalidLifecycle
            )),
            "a Running flag or host call alone cannot establish a nested drive"
        );
        let capability = runtime
            .stackless_machine_capability(machine)
            .expect("rejected nested entry leaves Machine live");
        assert_eq!(
            unsafe {
                (*runtime.state.get())
                    .execution
                    .machine_diagnostics(current_thread_token(), capability)
                    .expect("diagnostics resolve")
                    .live_frames
            },
            0,
            "entry authority is rejected before Frame storage"
        );

        runtime
            .register_stackless_resumable(SemanticRegistrationV1::new(
                registered_probe_thunk,
                nested,
                1,
                0,
                1,
            ))
            .expect("nested entry registers before publication");
        let _ = runtime.add_method(
            runtime.globals().object_vt,
            nested_selector,
            registered_probe_thunk,
        );

        NESTED_PROBE_MACHINE.set(machine);
        NESTED_PROBE_SELECTOR.set(nested_selector);
        NESTED_PROBE_DEPTH_BEFORE.set(0);
        NESTED_PROBE_DEPTH_AFTER.set(0);
        let answer = runtime
            .drive_stackless_with_resolver(
                machine,
                outer,
                &[(0, receiver), (1, synchronous_selector), (2, expected)],
                None,
                |_, _, selector, _| {
                    assert_eq!(selector, synchronous_selector);
                    Ok(StacklessTarget::Synchronous(synchronous_nested_entry))
                },
            )
            .expect("outer drive resumes after nested entry");

        assert_eq!(answer, expected);
        assert!(CALL_PROBE_FORCED_GC.get());
        assert_eq!(NESTED_PROBE_DEPTH_BEFORE.get(), 1);
        assert_eq!(NESTED_PROBE_DEPTH_AFTER.get(), 1);
        let diagnostics = unsafe {
            (*runtime.state.get())
                .execution
                .machine_diagnostics(current_thread_token(), capability)
                .expect("completed diagnostics resolve")
        };
        assert_eq!(diagnostics.live_frames, 0);
        assert_eq!(diagnostics.high_water_live_frames, 2);
        assert_eq!(diagnostics.current_synchronous_boundary_depth, 0);
        assert_eq!(diagnostics.high_water_synchronous_boundary_depth, 1);
        assert_eq!(
            unsafe {
                (*runtime.state.get())
                    .execution
                    .machine(current_thread_token(), capability)
                    .expect("Machine remains inspectable")
                    .drive_epoch()
            },
            4,
            "outer and nested drive entry and exit each advance the epoch"
        );
    }

    #[test]
    fn suppressed_entry_scope_starts_a_fresh_machine_during_an_active_drive() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        CALL_SITE_PROBE_SELECTOR.set(runtime.intern(b"callSite:").expect("static selector"));
        RETURN_PROBE_SELECTOR.set(runtime.intern(b"returnAt:").expect("static selector"));
        let synchronous_selector = runtime
            .intern(b"synchronousSuppressedEntry:")
            .expect("static selector");
        let registered_selector = runtime
            .intern(b"registeredSuppressedEntry:")
            .expect("static selector");
        SUPPRESSED_ENTRY_SELECTOR.set(registered_selector);
        SUPPRESSED_ENTRY_MACHINE_HIGH_WATER.set(0);
        let receiver = runtime.canonical().object_prototype;
        let expected = runtime.make_integer(67);

        let nested = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 34,
                generation: 2,
                source_map: 34,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 1,
            driver_argument_count: 0,
            slot_kinds: vec![SlotKind::Oop, SlotKind::Oop],
            call_sites: Vec::new(),
            step_method: suppressed_entry_step,
        })
        .expect("suppressed-entry descriptor validates");
        runtime
            .register_stackless_resumable(SemanticRegistrationV1::new(
                suppressed_entry_thunk,
                nested,
                1,
                0,
                1,
            ))
            .expect("suppressed entry registers");
        let _ = runtime.add_method(
            runtime.globals().object_vt,
            registered_selector,
            suppressed_entry_thunk,
        );

        let outer = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 33,
                generation: 2,
                source_map: 33,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 2,
            driver_argument_count: 1,
            slot_kinds: vec![SlotKind::Oop; 4],
            call_sites: vec![CallSiteDescriptor {
                kind: CallSiteKind::Call,
                receiver_slot: 0,
                selector_slot: 1,
                argument_start: 2,
                argument_count: 1,
                result_slot: Some(3),
                continuation_pc: Some(1),
            }],
            step_method: single_call_parent_step,
        })
        .expect("suppressed-entry outer descriptor validates");
        let machine = runtime
            .create_stackless_machine()
            .expect("outer Machine creates");
        let answer = runtime
            .drive_stackless_with_resolver(
                machine,
                outer,
                &[(0, receiver), (1, synchronous_selector), (2, expected)],
                None,
                |_, _, selector, _| {
                    assert_eq!(selector, synchronous_selector);
                    Ok(StacklessTarget::Synchronous(synchronous_suppressed_entry))
                },
            )
            .expect("suppressed entry returns to the outer drive");

        assert_eq!(answer, expected);
        assert_eq!(
            SUPPRESSED_ENTRY_MACHINE_HIGH_WATER.get(),
            2,
            "unrelated work creates a fresh Machine beside the active outer one"
        );
        assert_eq!(
            runtime.stackless_runtime_diagnostics().live_machines,
            1,
            "the fresh Machine is destroyed before the outer drive resumes"
        );
        let capability = runtime
            .stackless_machine_capability(machine)
            .expect("outer Machine remains inspectable");
        let diagnostics = unsafe {
            (*runtime.state.get())
                .execution
                .machine_diagnostics(current_thread_token(), capability)
                .expect("outer diagnostics resolve")
        };
        assert_eq!(diagnostics.high_water_live_frames, 1);
        assert_eq!(diagnostics.live_frames, 0);
    }

    #[test]
    fn registered_ordinary_send_enters_thunk_and_answers_predecessor_value() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        RETURN_PROBE_SELECTOR.set(runtime.intern(b"returnAt:").expect("static selector"));
        CALL_SITE_PROBE_SELECTOR.set(runtime.intern(b"callSite:").expect("static selector"));
        CALL_PROBE_FORCED_GC.set(false);
        REGISTERED_THUNK_CALLS.set(0);
        REGISTERED_FRAME_PIN_HIGH_WATER.set(0);
        let migrated_selector = runtime
            .intern(b"registeredIdentity:")
            .expect("static selector");
        let predecessor_selector = runtime
            .intern(b"synchronousIdentity:")
            .expect("static selector");
        let receiver = runtime.canonical().object_prototype;
        let expected = runtime.make_integer(73);
        let descriptor = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 41,
                generation: 9,
                source_map: 41,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 1,
            driver_argument_count: 0,
            slot_kinds: vec![SlotKind::Oop, SlotKind::Oop],
            call_sites: Vec::new(),
            step_method: call_child_step,
        })
        .expect("registered descriptor validates");

        let mut rejected =
            SemanticRegistrationV1::new(registered_probe_thunk, descriptor.clone(), 1, 0, 1);
        rejected.byte_size -= 1;
        assert_eq!(
            runtime.register_stackless_resumable(rejected),
            Err(crate::execution::RegistrationError::RecordTooSmall),
            "invalid registration fails before publication"
        );
        assert!(
            runtime
                .lookup_method(runtime.globals().object_vt, migrated_selector)
                .is_none(),
            "rejected registration did not publish a binding"
        );

        let _registration = runtime
            .register_stackless_resumable(SemanticRegistrationV1::new(
                registered_probe_thunk,
                descriptor,
                1,
                0,
                1,
            ))
            .expect("semantic v1 registration succeeds");
        assert!(
            runtime
                .lookup_method(runtime.globals().object_vt, migrated_selector)
                .is_none(),
            "entry exists before its direct binding becomes visible"
        );
        let _ = runtime.add_method(
            runtime.globals().object_vt,
            migrated_selector,
            registered_probe_thunk,
        );
        let _ = runtime.add_method(
            runtime.globals().object_vt,
            predecessor_selector,
            synchronous_identity,
        );
        let installed = runtime
            .lookup_method(runtime.globals().object_vt, migrated_selector)
            .expect("migrated selector resolves");
        assert_eq!(
            installed,
            crate::raw::MethodAddress::from_method(registered_probe_thunk)
        );
        assert_ne!(
            installed,
            crate::raw::MethodAddress::from_method(call_child_step),
            "a vtable never contains the StepMethod address"
        );

        let predecessor =
            runtime.enter(|| runtime.send_slice(receiver, predecessor_selector, &[expected]));
        let migrated =
            runtime.enter(|| runtime.send_slice(receiver, migrated_selector, &[expected]));
        assert_eq!(migrated, predecessor);
        assert_eq!(migrated, expected);
        assert_eq!(REGISTERED_THUNK_CALLS.get(), 1);
        assert_eq!(REGISTERED_FRAME_PIN_HIGH_WATER.get(), 1);
        assert_eq!(
            runtime
                .stackless_registration_diagnostics()
                .active_frame_pins,
            0,
            "completed thunk drive releases its Frame pin"
        );
        assert!(CALL_PROBE_FORCED_GC.get());
        assert_eq!(
            runtime.stackless_runtime_diagnostics().live_machines,
            0,
            "ordinary caller never observes the thunk's fresh Machine"
        );

        let parent = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 42,
                generation: 9,
                source_map: 42,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 2,
            driver_argument_count: 1,
            slot_kinds: vec![SlotKind::Oop; 4],
            call_sites: vec![CallSiteDescriptor {
                kind: CallSiteKind::Call,
                receiver_slot: 0,
                selector_slot: 1,
                argument_start: 2,
                argument_count: 1,
                result_slot: Some(3),
                continuation_pc: Some(1),
            }],
            step_method: single_call_parent_step,
        })
        .expect("registered caller descriptor validates");
        let machine = runtime
            .create_stackless_machine()
            .expect("driver Machine creates");
        let direct = runtime
            .drive_stackless(
                machine,
                parent,
                &[(0, receiver), (1, migrated_selector), (2, expected)],
            )
            .expect("driver classifies registered thunk as resumable");
        assert_eq!(direct, expected);
        assert_eq!(
            REGISTERED_THUNK_CALLS.get(),
            1,
            "migrated-to-migrated resolution bypasses the thunk"
        );
        runtime
            .destroy_stackless_machine(machine)
            .expect("completed driver Machine destroys");

        let alias_selector = runtime
            .intern(b"registeredIdentityAlias:")
            .expect("static selector");
        let _ = runtime.add_method(
            runtime.globals().object_vt,
            alias_selector,
            registered_probe_thunk,
        );
        assert_eq!(
            runtime
                .stackless_registration_diagnostics()
                .direct_binding_references,
            2
        );
        let _ = runtime.add_method(
            runtime.globals().object_vt,
            migrated_selector,
            synchronous_identity,
        );
        assert_eq!(
            runtime
                .stackless_registration_diagnostics()
                .direct_binding_references,
            1,
            "overwriting one alias retires only its direct reference"
        );
        assert_eq!(
            runtime.enter(|| runtime.send_slice(receiver, alias_selector, &[expected])),
            expected,
            "the remaining alias stays resumable"
        );
        assert!(
            runtime
                .remove_method(runtime.globals().object_vt, alias_selector)
                .is_some()
        );
        let diagnostics = runtime.stackless_registration_diagnostics();
        assert_eq!(diagnostics.direct_binding_references, 0);
        assert_eq!(diagnostics.live_entries, 0);
        assert_eq!(diagnostics.retired_entries, 1);
    }

    #[test]
    fn registered_stepmethods_never_appear_in_any_method_vtable() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        let selector = runtime
            .intern(b"enumeratedRegisteredIdentity:")
            .expect("static selector");
        let descriptor = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 43,
                generation: 9,
                source_map: 43,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 1,
            driver_argument_count: 0,
            slot_kinds: vec![SlotKind::Oop, SlotKind::Oop],
            call_sites: Vec::new(),
            step_method: call_child_step,
        })
        .expect("enumeration descriptor validates");
        runtime
            .register_stackless_resumable(SemanticRegistrationV1::new(
                registered_probe_thunk,
                descriptor,
                1,
                0,
                1,
            ))
            .expect("enumerated entry registers");
        let _ = runtime.add_method(
            runtime.globals().object_vt,
            selector,
            registered_probe_thunk,
        );

        let step_address = crate::raw::MethodAddress::from_method(call_child_step);
        let opaque = unsafe { (*runtime.state.get()).internal_dictionaries }
            .expect("bootstrap internal dictionaries are identified");
        for vtable_index in 0..runtime.vtable_count() {
            let vtable = runtime.vtable_at(vtable_index);
            if vtable == opaque.0 || vtable == opaque.1 {
                continue;
            }
            let mut selector_index = 0;
            loop {
                let candidate = runtime.vtable_selector_at(vtable, selector_index);
                if candidate.is_null() {
                    break;
                }
                assert_ne!(
                    runtime.lookup_method(vtable, candidate),
                    Some(step_address),
                    "vtable {vtable_index} selector {selector_index} exposed a StepMethod"
                );
                selector_index += 1;
            }
        }
        assert_eq!(
            runtime.lookup_method(runtime.globals().object_vt, selector),
            Some(crate::raw::MethodAddress::from_method(
                registered_probe_thunk
            )),
            "enumeration includes the installed entry-thunk binding"
        );
    }

    #[test]
    fn stepmethod_sends_every_microcode_message_with_a_resumable_replacement() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        RETURN_PROBE_SELECTOR.set(runtime.intern(b"returnAt:").expect("static selector"));
        MICROCODE_REPLACEMENT_CALLS.set(0);
        MICROCODE_REPLACEMENT_MACHINE_HIGH_WATER.set(0);
        OUTER_ACTION_STALE_DURING_NESTED.set(false);
        OUTER_ACTION_STALE_AFTER_NESTED.set(false);

        let replacement = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 45,
                generation: 10,
                source_map: 45,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 1,
            driver_argument_count: 0,
            slot_kinds: vec![SlotKind::Oop, SlotKind::Oop],
            call_sites: Vec::new(),
            step_method: microcode_replacement_step,
        })
        .expect("replacement descriptor validates");
        runtime
            .register_stackless_resumable(SemanticRegistrationV1::new(
                microcode_replacement_thunk,
                replacement,
                1,
                0,
                1,
            ))
            .expect("microcode replacement registers");

        let machine = runtime
            .create_stackless_machine()
            .expect("microcode Machine creates");
        let private_machine_vtable = runtime.ensure_private_vtable(machine);
        let oop_at = runtime.intern(b"oopAt:").expect("static selector");
        let _ = runtime.add_method(private_machine_vtable, oop_at, microcode_replacement_thunk);
        let receiver = runtime.canonical().object_prototype;
        let selector = runtime
            .intern(b"unusedMicrocodeTarget:")
            .expect("static selector");
        let original = runtime.make_integer(17);
        let argument = runtime.make_integer(23);
        let descriptor = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 44,
                generation: 10,
                source_map: 44,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 2,
            driver_argument_count: 1,
            slot_kinds: vec![
                SlotKind::Oop,
                SlotKind::Oop,
                SlotKind::Oop,
                SlotKind::Oop,
                SlotKind::Oop,
                SlotKind::Raw,
            ],
            call_sites: vec![
                CallSiteDescriptor {
                    kind: CallSiteKind::Call,
                    receiver_slot: 0,
                    selector_slot: 1,
                    argument_start: 3,
                    argument_count: 1,
                    result_slot: Some(2),
                    continuation_pc: Some(1),
                },
                CallSiteDescriptor {
                    kind: CallSiteKind::TailCall,
                    receiver_slot: 0,
                    selector_slot: 1,
                    argument_start: 3,
                    argument_count: 1,
                    result_slot: None,
                    continuation_pc: None,
                },
            ],
            step_method: all_microcode_step,
        })
        .expect("all-microcode descriptor validates");

        let result = runtime
            .drive_stackless(
                machine,
                descriptor,
                &[
                    (0, receiver),
                    (1, selector),
                    (2, original),
                    (3, argument),
                    (4, runtime.canonical().nil),
                ],
            )
            .expect("all microcode messages remain reentrant");
        assert!(runtime.is_integer(result));
        assert_eq!(runtime.integer_value(result), 211);
        assert_eq!(MICROCODE_REPLACEMENT_CALLS.get(), 1);
        assert!(OUTER_ACTION_STALE_DURING_NESTED.get());
        assert!(OUTER_ACTION_STALE_AFTER_NESTED.get());
        assert_eq!(
            MICROCODE_REPLACEMENT_MACHINE_HIGH_WATER.get(),
            1,
            "the resumable replacement nested on the scoped Machine"
        );
        let capability = runtime
            .stackless_machine_capability(machine)
            .expect("completed Machine remains inspectable");
        let diagnostics = unsafe {
            (*runtime.state.get())
                .execution
                .machine_diagnostics(current_thread_token(), capability)
                .expect("Machine diagnostics resolve")
        };
        assert_eq!(diagnostics.live_frames, 0);
        assert_eq!(diagnostics.high_water_live_frames, 2);
    }

    #[test]
    fn identical_thunk_addresses_are_registered_per_runtime() {
        let first_bootstrap = build_bootstrap();
        let first = &first_bootstrap.runtime;
        first.publish_canonical_exports();
        first.install_execution_objects();
        let second_bootstrap = build_bootstrap();
        let second = &second_bootstrap.runtime;
        second.publish_canonical_exports();
        second.install_execution_objects();

        let first_return = first.intern(b"returnAt:").expect("static selector");
        let second_return = second.intern(b"returnAt:").expect("static selector");
        let first_selector = first
            .intern(b"runtimeLocalRegisteredIdentity:")
            .expect("static selector");
        let second_selector = second
            .intern(b"runtimeLocalRegisteredIdentity:")
            .expect("static selector");
        let first_receiver = first.canonical().object_prototype;
        let second_receiver = second.canonical().object_prototype;
        let first_expected = first.make_integer(101);
        let second_expected = second.make_integer(103);

        let descriptor = |implementation, generation| {
            FrameDescriptor::validate(FrameDescriptorSpec {
                identity: DescriptorIdentity {
                    implementation,
                    generation,
                    source_map: implementation,
                },
                layout_version: FRAME_LAYOUT_VERSION_V1,
                program_counter_count: 1,
                driver_argument_count: 0,
                slot_kinds: vec![SlotKind::Oop, SlotKind::Oop],
                call_sites: Vec::new(),
                step_method: call_child_step,
            })
            .expect("runtime-local descriptor validates")
        };

        let mapped_generation = crate::execution::RegistrationGenerationLease::new(17);
        let mapped_generation_observer = mapped_generation.clone();
        first
            .register_stackless_resumable(
                SemanticRegistrationV1::new(registered_probe_thunk, descriptor(71, 17), 1, 0, 1)
                    .with_generation_lease(mapped_generation.clone()),
            )
            .expect("first Runtime accepts the thunk address");
        second
            .register_stackless_resumable(
                SemanticRegistrationV1::new(registered_probe_thunk, descriptor(71, 17), 1, 0, 1)
                    .with_generation_lease(mapped_generation),
            )
            .expect("second Runtime independently accepts the same thunk address");
        assert_eq!(mapped_generation_observer.holder_count(), 3);
        let _ = first.add_method(
            first.globals().object_vt,
            first_selector,
            registered_probe_thunk,
        );
        let _ = second.add_method(
            second.globals().object_vt,
            second_selector,
            registered_probe_thunk,
        );

        RETURN_PROBE_SELECTOR.set(first_return);
        assert_eq!(
            first.enter(|| first.send_slice(first_receiver, first_selector, &[first_expected])),
            first_expected
        );
        RETURN_PROBE_SELECTOR.set(second_return);
        assert_eq!(
            second.enter(|| second.send_slice(
                second_receiver,
                second_selector,
                &[second_expected]
            )),
            second_expected
        );

        assert!(
            first
                .remove_method(first.globals().object_vt, first_selector)
                .is_some()
        );
        let first_diagnostics = first.stackless_registration_diagnostics();
        assert_eq!(first_diagnostics.live_entries, 0);
        assert_eq!(first_diagnostics.retired_entries, 1);
        assert_eq!(first_diagnostics.registration_generation_leases, 0);
        let second_diagnostics = second.stackless_registration_diagnostics();
        assert_eq!(second_diagnostics.live_entries, 1);
        assert_eq!(second_diagnostics.retired_entries, 0);
        assert_eq!(second_diagnostics.direct_binding_references, 1);
        assert_eq!(second_diagnostics.registration_generation_leases, 1);
        assert_eq!(mapped_generation_observer.holder_count(), 2);
        assert_eq!(
            second.enter(|| second.send_slice(
                second_receiver,
                second_selector,
                &[second_expected]
            )),
            second_expected,
            "retiring one Runtime's entry does not change the other registry"
        );
    }

    #[test]
    fn failed_initial_frame_push_releases_registered_entry_pin() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        let selector = runtime
            .intern(b"registeredPushFailure:")
            .expect("static selector");
        let receiver = runtime.canonical().object_prototype;
        let argument = runtime.make_integer(107);
        let descriptor = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 73,
                generation: 23,
                source_map: 73,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 1,
            driver_argument_count: 0,
            slot_kinds: vec![SlotKind::Oop, SlotKind::Oop],
            call_sites: Vec::new(),
            step_method: call_child_step,
        })
        .expect("push-failure descriptor validates");
        runtime
            .register_stackless_resumable(SemanticRegistrationV1::new(
                registered_probe_thunk,
                descriptor,
                1,
                0,
                1,
            ))
            .expect("entry registers");
        let _ = runtime.add_method(
            runtime.globals().object_vt,
            selector,
            registered_probe_thunk,
        );

        let read = runtime.enter_dispatch_read();
        let method = runtime
            .lookup_method(runtime.globals().object_vt, selector)
            .expect("published thunk resolves");
        let target = unsafe {
            (&mut *runtime.state.get())
                .execution
                .classify_resumable(method)
                .expect("published thunk classifies")
                .expect("registered thunk is resumable")
        };
        drop(read);
        let StacklessTarget::Registered { entry } = target else {
            panic!("classification must acquire the registered entry")
        };
        let crate::execution::MaterializedEntry { descriptor, .. } = runtime
            .materialize_registered_target(entry, receiver, &[argument])
            .expect("acquired entry materializes only at Frame push");
        assert_eq!(
            runtime
                .stackless_registration_diagnostics()
                .acquired_entries,
            1
        );

        let machine = runtime
            .create_stackless_machine()
            .expect("failure-probe Machine creates");
        let result = runtime.drive_stackless_with_resolver(
            machine,
            descriptor,
            &[(usize::MAX, argument)],
            Some(entry),
            |_, _, _, _| unreachable!("invalid initial storage fails before stepping"),
        );
        assert_eq!(
            result,
            Err(crate::execution::ExecutionError::Frame(
                crate::execution::FrameError::InvalidSlot
            ))
        );
        let diagnostics = runtime.stackless_registration_diagnostics();
        assert_eq!(diagnostics.acquired_entries, 0);
        assert_eq!(diagnostics.active_frame_pins, 0);
        assert_eq!(diagnostics.live_entries, 1);

        assert!(
            runtime
                .remove_method(runtime.globals().object_vt, selector)
                .is_some()
        );
        let diagnostics = runtime.stackless_registration_diagnostics();
        assert_eq!(diagnostics.live_entries, 0);
        assert_eq!(diagnostics.retired_entries, 1);
    }

    #[derive(Clone, Copy, Debug)]
    enum LateBoundMicrocodeMutation {
        Add,
        Overwrite,
        Removal,
        InheritedReplacement,
        PrivateChildShadow,
        CustomLookup,
    }

    fn exercise_late_bound_microcode_mutation(case: LateBoundMicrocodeMutation) {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        LATE_BOUND_RETURN_A_CALLS.set(0);
        LATE_BOUND_RETURN_B_CALLS.set(0);
        CUSTOM_LOOKUP_CALLS.set(0);

        let objects = unsafe {
            (*runtime.state.get())
                .execution
                .objects()
                .expect("execution objects are installed")
        };
        let return_selector = runtime.intern(b"returnAt:").expect("static selector");
        let added_selector = runtime
            .intern(b"lateBoundReturnAt:")
            .expect("static selector");
        let microcode_selector = match case {
            LateBoundMicrocodeMutation::Add | LateBoundMicrocodeMutation::InheritedReplacement => {
                added_selector
            }
            LateBoundMicrocodeMutation::Overwrite
            | LateBoundMicrocodeMutation::Removal
            | LateBoundMicrocodeMutation::PrivateChildShadow
            | LateBoundMicrocodeMutation::CustomLookup => return_selector,
        };
        CUSTOM_LOOKUP_SELECTOR.set(microcode_selector);
        CUSTOM_LOOKUP_RESULT
            .set(crate::raw::MethodAddress::from_method(late_bound_return_b).as_oop());
        CALL_SITE_PROBE_SELECTOR.set(runtime.intern(b"callSite:").expect("static selector"));
        RETURN_PROBE_SELECTOR.set(microcode_selector);

        match case {
            LateBoundMicrocodeMutation::Removal => {
                let _ = runtime.add_method(
                    runtime.globals().object_vt,
                    return_selector,
                    late_bound_return_b,
                );
            }
            LateBoundMicrocodeMutation::InheritedReplacement => {
                let _ = runtime.add_method(
                    runtime.globals().object_vt,
                    added_selector,
                    late_bound_return_a,
                );
            }
            _ => {}
        }

        let synchronous_selector = runtime
            .intern(b"lateBindingSynchronousIdentity:")
            .expect("static selector");
        let receiver = runtime.canonical().object_prototype;
        let expected = runtime.make_integer(109);
        let descriptor_identity = DescriptorIdentity {
            implementation: 80 + case as u64,
            generation: 29,
            source_map: 80 + case as u64,
        };
        let descriptor = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: descriptor_identity,
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 2,
            driver_argument_count: 1,
            slot_kinds: vec![SlotKind::Oop; 4],
            call_sites: vec![CallSiteDescriptor {
                kind: CallSiteKind::Call,
                receiver_slot: 0,
                selector_slot: 1,
                argument_start: 2,
                argument_count: 1,
                result_slot: Some(3),
                continuation_pc: Some(1),
            }],
            step_method: single_call_parent_step,
        })
        .expect("late-binding descriptor validates");
        let machine = runtime
            .create_stackless_machine()
            .expect("late-binding Machine creates");
        let capability = runtime
            .stackless_machine_capability(machine)
            .expect("Machine capability resolves");
        let mut mutation_count = 0;
        let answer = runtime
            .drive_stackless_with_resolver(
                machine,
                descriptor,
                &[(0, receiver), (1, synchronous_selector), (2, expected)],
                None,
                |_, _, selector, _| {
                    assert_eq!(selector, synchronous_selector);
                    mutation_count += 1;
                    assert_eq!(mutation_count, 1, "mutation occurs at one turn boundary");
                    let active_identity = unsafe {
                        let state = &*runtime.state.get();
                        let machine_state = state
                            .execution
                            .machine(current_thread_token(), capability)
                            .expect("active Machine remains registered");
                        let frame = machine_state
                            .arena()
                            .current_handle()
                            .expect("originating Frame remains active");
                        machine_state
                            .arena()
                            .descriptor(frame)
                            .expect("captured descriptor remains installed")
                            .identity()
                    };
                    assert_eq!(active_identity, descriptor_identity);

                    match case {
                        LateBoundMicrocodeMutation::Add => {
                            let _ = runtime.add_method(
                                objects.machine_vtable,
                                added_selector,
                                late_bound_return_b,
                            );
                        }
                        LateBoundMicrocodeMutation::Overwrite => {
                            let _ = runtime.add_method(
                                objects.machine_vtable,
                                return_selector,
                                late_bound_return_b,
                            );
                        }
                        LateBoundMicrocodeMutation::Removal => {
                            runtime
                                .remove_method(objects.machine_vtable, return_selector)
                                .expect("direct microcode binding removes");
                        }
                        LateBoundMicrocodeMutation::InheritedReplacement => {
                            let _ = runtime.add_method(
                                runtime.globals().object_vt,
                                added_selector,
                                late_bound_return_b,
                            );
                        }
                        LateBoundMicrocodeMutation::PrivateChildShadow => {
                            let private = runtime.ensure_private_vtable(machine);
                            assert_ne!(private, objects.machine_vtable);
                            assert_eq!(runtime.vtable_parent(private), objects.machine_vtable);
                            let _ =
                                runtime.add_method(private, return_selector, late_bound_return_b);
                        }
                        LateBoundMicrocodeMutation::CustomLookup => {
                            let _ = runtime.add_method(
                                runtime.globals().vtable_vt,
                                runtime.globals().lookup,
                                late_bound_custom_lookup,
                            );
                        }
                    }
                    Ok(StacklessTarget::Synchronous(synchronous_identity))
                },
            )
            .unwrap_or_else(|error| panic!("{case:?} drive failed: {error:?}"));

        assert_eq!(
            answer, expected,
            "{case:?} preserves the active computation"
        );
        assert_eq!(mutation_count, 1);
        assert_eq!(LATE_BOUND_RETURN_A_CALLS.get(), 0);
        assert_eq!(LATE_BOUND_RETURN_B_CALLS.get(), 1);
        assert_eq!(
            CUSTOM_LOOKUP_CALLS.get(),
            usize::from(matches!(case, LateBoundMicrocodeMutation::CustomLookup))
        );
        runtime
            .stackless_machine_capability(machine)
            .expect("private shadow retains Machine capability identity");
        runtime
            .destroy_stackless_machine(machine)
            .expect("completed late-binding Machine destroys");
    }

    #[test]
    fn active_frame_observes_all_ordinary_microcode_lookup_mutations() {
        for case in [
            LateBoundMicrocodeMutation::Add,
            LateBoundMicrocodeMutation::Overwrite,
            LateBoundMicrocodeMutation::Removal,
            LateBoundMicrocodeMutation::InheritedReplacement,
            LateBoundMicrocodeMutation::PrivateChildShadow,
            LateBoundMicrocodeMutation::CustomLookup,
        ] {
            exercise_late_bound_microcode_mutation(case);
        }
    }

    #[test]
    fn active_registered_frame_survives_binding_redefinition_until_return() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        RETURN_PROBE_SELECTOR.set(runtime.intern(b"returnAt:").expect("static selector"));
        let selector = runtime
            .intern(b"redefineWhileActive:")
            .expect("static selector");
        REDEFINE_PROBE_SELECTOR.set(selector);
        let receiver = runtime.canonical().object_prototype;
        let expected = runtime.make_integer(89);
        let descriptor = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 51,
                generation: 12,
                source_map: 51,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 1,
            driver_argument_count: 0,
            slot_kinds: vec![SlotKind::Oop, SlotKind::Oop],
            call_sites: Vec::new(),
            step_method: redefine_during_registered_step,
        })
        .expect("redefinition descriptor validates");
        runtime
            .register_stackless_resumable(SemanticRegistrationV1::new(
                redefining_probe_thunk,
                descriptor,
                1,
                0,
                1,
            ))
            .expect("entry registers before binding");
        let _ = runtime.add_method(
            runtime.globals().object_vt,
            selector,
            redefining_probe_thunk,
        );

        let answer = runtime.enter(|| runtime.send_slice(receiver, selector, &[expected]));
        assert_eq!(answer, expected);
        assert_eq!(
            runtime.lookup_method(runtime.globals().object_vt, selector),
            Some(crate::raw::MethodAddress::from_method(synchronous_identity)),
            "redefinition is visible after the captured StepMethod returns"
        );
        let diagnostics = runtime.stackless_registration_diagnostics();
        assert_eq!(diagnostics.direct_binding_references, 0);
        assert_eq!(diagnostics.active_frame_pins, 0);
        assert_eq!(diagnostics.retired_entries, 1);
    }

    #[test]
    fn pre_retirement_dispatch_reader_classifies_and_acquires_exact_thunk() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        let selector = runtime
            .intern(b"pauseBeforeClassification:")
            .expect("static selector");
        let descriptor = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 61,
                generation: 14,
                source_map: 61,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 1,
            driver_argument_count: 0,
            slot_kinds: vec![SlotKind::Oop, SlotKind::Oop],
            call_sites: Vec::new(),
            step_method: call_child_step,
        })
        .expect("reader-race descriptor validates");
        runtime
            .register_stackless_resumable(SemanticRegistrationV1::new(
                registered_probe_thunk,
                descriptor,
                1,
                0,
                1,
            ))
            .expect("entry registers");
        let _ = runtime.add_method(
            runtime.globals().object_vt,
            selector,
            registered_probe_thunk,
        );

        let read = runtime.enter_dispatch_read();
        let method = runtime
            .lookup_method(runtime.globals().object_vt, selector)
            .expect("reader resolves the installed thunk");
        assert!(
            runtime
                .remove_method(runtime.globals().object_vt, selector)
                .is_some(),
            "deterministic pause retires the final binding"
        );
        let target = unsafe {
            (&mut *runtime.state.get())
                .execution
                .classify_resumable(method)
                .expect("pre-retirement reader still classifies")
                .expect("known thunk remains resumable")
        };
        let StacklessTarget::Registered { entry } = target else {
            panic!("classification must acquire the registered entry")
        };
        let diagnostics = runtime.stackless_registration_diagnostics();
        assert_eq!(diagnostics.acquired_entries, 1);
        assert_eq!(diagnostics.retired_entries, 0);
        drop(read);
        assert_eq!(
            runtime.stackless_registration_diagnostics().retired_entries,
            0,
            "an acquired entry keeps retirement pending after the reader exits"
        );
        unsafe {
            (&mut *runtime.state.get())
                .execution
                .release_acquired_entry(entry)
                .expect("failed-push style cleanup releases acquisition");
        }
        let diagnostics = runtime.stackless_registration_diagnostics();
        assert_eq!(diagnostics.acquired_entries, 0);
        assert_eq!(diagnostics.retired_entries, 1);
        let stale = unsafe {
            (&mut *runtime.state.get())
                .execution
                .classify_resumable(method)
        };
        assert!(
            matches!(
                stale,
                Err(crate::execution::RegistrationError::EntryRetired)
            ),
            "a later reader never reinterprets the retired thunk as synchronous"
        );
    }

    #[test]
    fn explicit_generation_lease_keeps_custom_lookup_cache_live_until_release() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        RETURN_PROBE_SELECTOR.set(runtime.intern(b"returnAt:").expect("static selector"));
        CALL_PROBE_FORCED_GC.set(false);
        REGISTERED_THUNK_CALLS.set(0);
        CUSTOM_LOOKUP_CALLS.set(0);
        let selector = runtime
            .intern(b"leasedCustomLookupIdentity:")
            .expect("static selector");
        let receiver = runtime.canonical().object_prototype;
        let expected = runtime.make_integer(113);
        let descriptor = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 91,
                generation: 31,
                source_map: 91,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 1,
            driver_argument_count: 0,
            slot_kinds: vec![SlotKind::Oop, SlotKind::Oop],
            call_sites: Vec::new(),
            step_method: call_child_step,
        })
        .expect("leased-cache descriptor validates");

        let invalid =
            SemanticRegistrationV1::new(registered_probe_thunk, descriptor.clone(), 1, 0, 1)
                .with_generation_lease(crate::execution::RegistrationGenerationLease::new(999));
        assert_eq!(
            runtime.register_stackless_resumable(invalid),
            Err(crate::execution::RegistrationError::InvalidIdentity),
            "registration rejects a lease for a different generation"
        );

        let generation_lease = crate::execution::RegistrationGenerationLease::new(31);
        let lease_observer = generation_lease.clone();
        runtime
            .register_stackless_resumable(
                SemanticRegistrationV1::new(registered_probe_thunk, descriptor, 1, 0, 1)
                    .with_generation_lease(generation_lease),
            )
            .expect("matching generation lease transfers into registration");
        let _ = runtime.add_method(
            runtime.globals().object_vt,
            selector,
            registered_probe_thunk,
        );
        let method = runtime
            .lookup_method(runtime.globals().object_vt, selector)
            .expect("direct thunk binding resolves");
        let cache_lease = runtime
            .acquire_stackless_generation_lease(method)
            .expect("live entry lends an explicit generation lease");
        let diagnostics = runtime.stackless_registration_diagnostics();
        assert_eq!(diagnostics.registration_generation_leases, 1);
        assert_eq!(diagnostics.explicit_generation_leases, 1);
        assert_eq!(lease_observer.holder_count(), 3);

        CUSTOM_LOOKUP_SELECTOR.set(selector);
        CUSTOM_LOOKUP_RESULT.set(method.as_oop());
        let _ = runtime.add_method(
            runtime.globals().vtable_vt,
            runtime.globals().lookup,
            late_bound_custom_lookup,
        );
        assert!(
            runtime
                .remove_method(runtime.globals().object_vt, selector)
                .is_some(),
            "the cache becomes the only source of the carrier"
        );
        let diagnostics = runtime.stackless_registration_diagnostics();
        assert_eq!(diagnostics.direct_binding_references, 0);
        assert_eq!(diagnostics.live_entries, 1);
        assert_eq!(diagnostics.explicit_generation_leases, 1);

        assert_eq!(
            runtime.enter(|| runtime.send_slice(receiver, selector, &[expected])),
            expected,
            "the custom lookup cache remains resumable while explicitly leased"
        );
        assert_eq!(REGISTERED_THUNK_CALLS.get(), 1);
        let diagnostics = runtime.stackless_registration_diagnostics();
        assert_eq!(diagnostics.acquired_entries, 0);
        assert_eq!(diagnostics.active_frame_pins, 0);
        assert_eq!(diagnostics.explicit_generation_leases, 1);

        runtime
            .release_stackless_generation_lease(cache_lease)
            .expect("cache releases its explicit generation lease");
        let diagnostics = runtime.stackless_registration_diagnostics();
        assert_eq!(diagnostics.live_entries, 0);
        assert_eq!(diagnostics.retired_entries, 1);
        assert_eq!(diagnostics.explicit_generation_leases, 0);
        assert_eq!(diagnostics.registration_generation_leases, 0);
        assert_eq!(lease_observer.holder_count(), 1);
        assert_eq!(
            runtime.release_stackless_generation_lease(cache_lease),
            Err(crate::execution::RegistrationError::StaleCapability),
            "generation lease capabilities are one-shot"
        );

        let stale = runtime.enter(|| runtime.resolve_stackless_target(receiver, selector));
        assert!(
            matches!(
                stale,
                Err(crate::execution::ExecutionError::Registration(
                    crate::execution::RegistrationError::EntryRetired
                ))
            ),
            "the same unleased cache is stale, never synchronous"
        );
        assert_eq!(CUSTOM_LOOKUP_CALLS.get(), 2);
    }

    #[test]
    fn driver_failure_crossing_thunk_boundary_is_fatal_and_kinded() {
        const CHILD_ENV: &str = "BIRNAM_TEST_STACKLESS_FATAL_CHILD";
        if std::env::var_os(CHILD_ENV).is_some() {
            let bootstrap = build_bootstrap();
            let runtime = &bootstrap.runtime;
            runtime.publish_canonical_exports();
            runtime.install_execution_objects();
            let receiver = runtime.canonical().object_prototype;
            runtime.enter(|| unsafe {
                runtime.enter_registered_resumable_or_fatal(
                    registered_probe_thunk,
                    receiver,
                    core::ptr::null(),
                    0,
                )
            });
            unreachable!("fatal thunk boundary returned")
        }

        let output = std::process::Command::new(
            std::env::current_exe().expect("current test executable resolves"),
        )
        .args([
            "--exact",
            "runtime::tests::driver_failure_crossing_thunk_boundary_is_fatal_and_kinded",
            "--nocapture",
        ])
        .env(CHILD_ENV, "1")
        .output()
        .expect("fatal-boundary child starts");
        assert!(!output.status.success(), "fatal boundary cannot be caught");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("birnam-id invariant failed: missingImplementation"),
            "fatal boundary reports its out-of-band kind: {stderr}"
        );
    }

    #[test]
    fn two_stepmethods_mutually_tail_call_one_million_times_at_constant_depth() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        TAIL_CALL_SITE_PROBE_SELECTOR.set(
            runtime
                .intern(b"tailCallSite:")
                .expect("static selector contains no NUL"),
        );
        RETURN_PROBE_SELECTOR.set(
            runtime
                .intern(b"returnAt:")
                .expect("static selector contains no NUL"),
        );
        TAIL_PROBE_TRANSITIONS.set(0);
        TAIL_PROBE_STACK_MIN.set(usize::MAX);
        TAIL_PROBE_STACK_MAX.set(0);
        let selector_a = runtime
            .intern(b"s1TailA:")
            .expect("static selector contains no NUL");
        let selector_b = runtime
            .intern(b"s1TailB:")
            .expect("static selector contains no NUL");
        let receiver = runtime.canonical().object_prototype;
        let expected = runtime.make_integer(77);
        let descriptor_for = |implementation, step_method| {
            FrameDescriptor::validate(FrameDescriptorSpec {
                identity: DescriptorIdentity {
                    implementation,
                    generation: 1,
                    source_map: implementation,
                },
                layout_version: FRAME_LAYOUT_VERSION_V1,
                program_counter_count: 1,
                driver_argument_count: 0,
                slot_kinds: vec![SlotKind::Oop; 3],
                call_sites: vec![CallSiteDescriptor {
                    kind: CallSiteKind::TailCall,
                    receiver_slot: 0,
                    selector_slot: 1,
                    argument_start: 2,
                    argument_count: 1,
                    result_slot: None,
                    continuation_pc: None,
                }],
                step_method,
            })
            .expect("tail descriptor validates")
        };
        let descriptor_a = descriptor_for(21, alternating_tail_step_a);
        let descriptor_b = descriptor_for(22, alternating_tail_step_b);
        let machine = runtime
            .create_stackless_machine()
            .expect("stackless Machine creates");

        let answer = runtime
            .drive_stackless_with_resolver(
                machine,
                descriptor_a.clone(),
                &[(0, receiver), (1, selector_b), (2, expected)],
                None,
                |_, resolved_receiver, selector, arguments| {
                    let (descriptor, next_selector) = if selector == selector_a {
                        (descriptor_a.clone(), selector_b)
                    } else if selector == selector_b {
                        (descriptor_b.clone(), selector_a)
                    } else {
                        panic!("unexpected tail probe selector")
                    };
                    Ok(StacklessTarget::Resumable {
                        descriptor,
                        initial_oops: vec![
                            (0, resolved_receiver),
                            (1, next_selector),
                            (2, arguments[0]),
                        ],
                        entry: None,
                    })
                },
            )
            .expect("mutual tail recursion completes");

        assert_eq!(answer, expected);
        assert_eq!(TAIL_PROBE_TRANSITIONS.get(), CONTINUE_PROBE_TRANSITIONS);
        assert!(
            TAIL_PROBE_STACK_MAX
                .get()
                .saturating_sub(TAIL_PROBE_STACK_MIN.get())
                <= 4096,
            "tail transitions keep bounded native-stack movement"
        );
        let capability = runtime
            .stackless_machine_capability(machine)
            .expect("completed Machine remains inspectable");
        let diagnostics = unsafe {
            (*runtime.state.get())
                .execution
                .machine_diagnostics(current_thread_token(), capability)
                .expect("diagnostics remain available")
        };
        assert_eq!(diagnostics.high_water_live_frames, 1);
        let tail_action = unsafe {
            (*runtime.state.get())
                .execution
                .machine(current_thread_token(), capability)
                .expect("completed Machine remains live")
                .scratch_actions()
                .2
        };
        let action_kind = runtime.intern(b"actionKind").expect("static selector");
        let tail_kind = runtime.intern(b"tailCall").expect("static symbol");
        assert_eq!(
            runtime.enter(|| runtime.send_slice(tail_action, action_kind, &[])),
            tail_kind
        );
    }

    #[test]
    fn malformed_step_action_fails_machine_without_applying_control_transition() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        let machine = runtime
            .create_stackless_machine()
            .expect("stackless Machine creates");
        let live_value = runtime.make_integer(9);
        let descriptor = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 3,
                generation: 1,
                source_map: 3,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 1,
            driver_argument_count: 0,
            slot_kinds: vec![SlotKind::Oop],
            call_sites: Vec::new(),
            step_method: malformed_action_step,
        })
        .expect("malformed-action probe descriptor validates");

        assert_eq!(
            runtime.drive_stackless(machine, descriptor, &[(0, live_value)]),
            Err(crate::execution::ExecutionError::InvalidAction)
        );
        assert_eq!(
            crate::execution::ExecutionError::InvalidAction
                .failure_kind()
                .symbol(),
            "invalidStepAction"
        );
        let capability = runtime
            .stackless_machine_capability(machine)
            .expect("failed Machine capability remains inspectable");
        unsafe {
            let state = &*runtime.state.get();
            let machine_state = state
                .execution
                .machine(current_thread_token(), capability)
                .expect("failed Machine state remains live");
            assert_eq!(
                machine_state.lifecycle(),
                crate::execution::MachineLifecycle::Failed
            );
            assert_eq!(machine_state.arena().diagnostics().live_frames, 1);
            let frame = machine_state
                .arena()
                .current_handle()
                .expect("failed control transition leaves current frame attached");
            assert_eq!(machine_state.arena().read_oop(frame, 0), Ok(live_value));
        }
    }

    #[test]
    fn step_action_failures_are_kinded_before_operand_decoding() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        RETURN_PROBE_SELECTOR.set(runtime.intern(b"returnAt:").expect("static selector"));
        RETAINED_STEP_ACTION.set(core::ptr::null_mut());
        let live_value = runtime.make_integer(127);

        let descriptor = |implementation, slot_kinds, step_method| {
            FrameDescriptor::validate(FrameDescriptorSpec {
                identity: DescriptorIdentity {
                    implementation,
                    generation: 37,
                    source_map: implementation,
                },
                layout_version: FRAME_LAYOUT_VERSION_V1,
                program_counter_count: 1,
                driver_argument_count: 0,
                slot_kinds,
                call_sites: Vec::new(),
                step_method,
            })
            .expect("failure descriptor validates")
        };

        let source_machine = runtime
            .create_stackless_machine()
            .expect("source Machine creates");
        assert_eq!(
            runtime
                .drive_stackless(
                    source_machine,
                    descriptor(101, vec![SlotKind::Oop], capture_return_action_step),
                    &[(0, live_value)],
                )
                .expect("source action completes its own Machine"),
            live_value
        );
        assert!(!RETAINED_STEP_ACTION.get().is_null());

        let wrong_machine = runtime
            .create_stackless_machine()
            .expect("wrong-Machine probe creates");
        let wrong_machine_error = runtime
            .drive_stackless(
                wrong_machine,
                descriptor(102, vec![SlotKind::Oop], return_retained_step_action),
                &[(0, live_value)],
            )
            .expect_err("foreign action must fail");
        assert_eq!(
            wrong_machine_error,
            crate::execution::ExecutionError::ActionWrongMachine
        );
        assert_eq!(
            wrong_machine_error.failure_kind().symbol(),
            "stepActionWrongMachine"
        );

        let stale_machine = runtime
            .create_stackless_machine()
            .expect("stale-action probe creates");
        let stale_error = runtime
            .drive_stackless(
                stale_machine,
                descriptor(103, vec![SlotKind::Oop], stale_return_action_step),
                &[(0, live_value)],
            )
            .expect_err("expired epoch must fail");
        assert_eq!(stale_error, crate::execution::ExecutionError::StaleAction);
        assert_eq!(stale_error.failure_kind().symbol(), "staleStepAction");

        let wrong_slot_machine = runtime
            .create_stackless_machine()
            .expect("wrong-slot probe creates");
        let wrong_slot_error = runtime
            .drive_stackless(
                wrong_slot_machine,
                descriptor(
                    104,
                    vec![SlotKind::Oop, SlotKind::Raw],
                    wrong_slot_return_action_step,
                ),
                &[(0, live_value)],
            )
            .expect_err("raw value slot must fail");
        assert_eq!(
            wrong_slot_error,
            crate::execution::ExecutionError::Frame(crate::execution::FrameError::WrongSlotKind)
        );
        assert_eq!(wrong_slot_error.failure_kind().symbol(), "invalidFrameSlot");

        for machine in [wrong_machine, stale_machine, wrong_slot_machine] {
            let capability = runtime
                .stackless_machine_capability(machine)
                .expect("failed Machine remains inspectable");
            let machine_state = unsafe {
                (*runtime.state.get())
                    .execution
                    .machine(current_thread_token(), capability)
                    .expect("failed Machine state remains live")
            };
            assert_eq!(
                machine_state.lifecycle(),
                crate::execution::MachineLifecycle::Failed
            );
            assert_eq!(machine_state.arena().diagnostics().live_frames, 1);
            let frame = machine_state
                .arena()
                .current_handle()
                .expect("rejected transition leaves its Frame attached");
            assert_eq!(machine_state.arena().read_oop(frame, 0), Ok(live_value));
        }

        runtime
            .destroy_stackless_machine(source_machine)
            .expect("completed source Machine destroys");
        for machine in [wrong_machine, stale_machine, wrong_slot_machine] {
            runtime
                .destroy_stackless_machine(machine)
                .expect("failed probe Machine destroys");
        }
    }

    #[test]
    fn rejected_initial_frame_push_fails_machine_without_allocating_arena_storage() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        let machine = runtime
            .create_stackless_machine()
            .expect("stackless Machine creates");
        let value = runtime.make_integer(5);
        let descriptor = FrameDescriptor::validate(FrameDescriptorSpec {
            identity: DescriptorIdentity {
                implementation: 4,
                generation: 1,
                source_map: 4,
            },
            layout_version: FRAME_LAYOUT_VERSION_V1,
            program_counter_count: 1,
            driver_argument_count: 0,
            slot_kinds: vec![SlotKind::Oop],
            call_sites: Vec::new(),
            step_method: inert_step,
        })
        .expect("setup-failure descriptor validates");

        assert_eq!(
            runtime.drive_stackless(machine, descriptor, &[(1, value)]),
            Err(crate::execution::ExecutionError::Frame(
                crate::execution::FrameError::InvalidSlot
            ))
        );
        let capability = runtime
            .stackless_machine_capability(machine)
            .expect("failed Machine capability remains inspectable");
        unsafe {
            let state = &*runtime.state.get();
            let machine_state = state
                .execution
                .machine(current_thread_token(), capability)
                .expect("failed Machine state remains live");
            assert_eq!(
                machine_state.lifecycle(),
                crate::execution::MachineLifecycle::Failed
            );
            assert_eq!(machine_state.arena().diagnostics().live_frames, 0);
            assert_eq!(machine_state.arena().diagnostics().reserved_bytes, 0);
        }
    }
}
