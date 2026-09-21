//! Single-thread-owned process state and short-borrow accessors.
#![allow(dead_code)] // R2 harness precedes public id_init wiring.

use core::cell::{Cell, UnsafeCell};
use std::sync::atomic::{AtomicU64, Ordering};

use birnam_artifact::stackless_selection::ImplementationKind;

pub(crate) mod id_protocol;
mod id_storage;

use crate::abi::SLOT_OOP;
use crate::abi::{MarkFn, Method, Oop};
use crate::boundary;
use crate::env::Environment;
use crate::execution::{
    CallSiteKind, DecodedAction, ExecutionError, ExecutionObjects, ExecutionState, FrameDescriptor,
    FrameViewError, GenerationLeaseCapability, MachineCapability, MachineError, MaterializedEntry,
    ReturnDestination, RuntimeExecutionDiagnostics, SemanticRegistrationV1, SlotKind,
    StacklessTarget,
};
use crate::gc::{MetadataShell, with_native_root_range};
use crate::heap::{Heap, Permanence};
use crate::raw::MethodAddress;
use crate::registration::{
    ABORT_V1, DescriptorAdmission, PREPARE_V1, ResumableRegistrationEnvelope, StagedUnit,
};
use crate::symbol::{EmbeddedNul, Symbols};
use crate::behaviors::AddMethod;
use crate::abi::id_protocol::NativeEntry;
use crate::behaviors::Behaviors;

#[cfg(all(test, target_os = "macos"))]
mod architectural_fit_experiment;
#[cfg(all(test, target_os = "macos"))]
mod boundary_control_tests;
mod task_control;
mod scheduling;
#[cfg(test)]
mod scheduling_tests;
#[cfg(test)]
mod turn_tests;
#[cfg(feature = "ffi-test-hooks")]
mod closure_action_probe;
#[cfg(feature = "ffi-test-hooks")]
mod closure_api_probe;
#[cfg(feature = "ffi-test-hooks")]
mod closure_budget_probe;
#[cfg(feature = "ffi-test-hooks")]
mod closure_control_probe;
#[cfg(feature = "ffi-test-hooks")]
mod closure_journal_probe;
#[cfg(feature = "ffi-test-hooks")]
mod closure_sequence_probe;
#[cfg(feature = "ffi-test-hooks")]
mod closure_timing_probe;
#[cfg(all(feature = "ffi-test-hooks", feature = "allocation-metrics"))]
mod consumer_lifetime_probe;
#[cfg(test)]
mod deopt_experiment;
mod dispatch_cache;
pub(crate) mod storage;
#[cfg(all(test, target_os = "macos"))]
mod generated_deopt_experiment;
mod guarded_dispatch;
mod guarded_lookup;
mod native_tier;
mod numeric;
mod integer;
mod float;
mod word;
mod string;
mod native_allocation;
mod tracing;
mod collect;
mod recovery;
#[cfg(feature = "ffi-test-hooks")]
mod source_probe_roots;
#[cfg(feature = "ffi-test-hooks")]
mod unboxed_numeric_probe;
#[cfg(feature = "ffi-test-hooks")]
use source_probe_roots::with_source_probe_native_roots;
#[cfg(all(test, target_os = "macos"))]
mod v2_control_tests;

static FRAME_VIEW_LAYOUT: [u8; 1] = [SLOT_OOP];

#[derive(Clone, Copy)]
pub(crate) struct DispatchGlobals {
    pub(crate) vtable_vt: Oop,
    pub(crate) object_vt: Oop,
    pub(crate) symbol_vt: Oop,
    pub(crate) lookup: Oop,
    pub(crate) does_not_understand: Oop,
}

impl DispatchGlobals {
    fn roots(self) -> [Oop; 5] {
        [
            self.vtable_vt,
            self.object_vt,
            self.symbol_vt,
            self.lookup,
            self.does_not_understand,
        ]
    }
}

/// Identities fixed by the one-way bootstrap transition.
#[derive(Clone, Copy)]
pub(crate) struct CanonicalObjects {
    pub(crate) symbol_list: Oop,
    pub(crate) environment: Oop,
    pub(crate) nil_vt: Oop,
    pub(crate) bool_vt: Oop,
    pub(crate) nil: Oop,
    pub(crate) true_object: Oop,
    pub(crate) false_object: Oop,
    pub(crate) object_prototype: Oop,
    pub(crate) symbol_prototype: Oop,
}

impl CanonicalObjects {
    fn roots(self) -> [Oop; 9] {
        [
            self.symbol_list,
            self.environment,
            self.nil_vt,
            self.bool_vt,
            self.nil,
            self.true_object,
            self.false_object,
            self.object_prototype,
            self.symbol_prototype,
        ]
    }
}

struct RuntimeState {
    method_observation: crate::method_observation::Service,
    root_registry: crate::root_registry::Service,
    vtable_enumeration: crate::vtable_enumeration::Service,
    // Ordered authorities are dropped before the raw heap owner at harness
    // teardown. No destructor dereferences a heap payload.
    symbols: crate::owned::Owned<Symbols>,
    id_protocol: Option<Box<crate::native_id::State>>,
    vtables: Behaviors,
    heap: Heap,
    globals: DispatchGlobals,
    canonical: Option<CanonicalObjects>,
    internal_dictionaries: Option<(Oop, Oop)>,
    symbol_compaction_enabled: bool,
    symbol_compatibility_limits: Option<SymbolCompatibilityLimits>,
    metadata: MetadataShell,
    environment: Environment,
    execution: ExecutionState,
    staged_resumable_unit: Option<StagedUnit>,
    staged_resumable_claims: Option<Vec<Option<RuntimeRegistrationClaim>>>,
    staged_resumable_publications: Option<Vec<StagedResumablePublication>>,
    staged_catalog_drift: Option<Vec<bool>>,
    committed_resumable_units: Vec<CommittedResumableUnit>,
    block_vtable_catalog: Vec<BlockVtableCatalogEntry>,
    registration_tokens: Vec<RuntimeRegistrationToken>,
    next_registration_token: u64,
    suppressed_registration_generation: Option<u64>,
    pending_stackless_retirements: Vec<u64>,
}

#[derive(Clone, Copy)]
struct SymbolCompatibilityLimits {
    count: usize,
    owned_name_bytes: usize,
}

#[derive(Clone, Copy)]
struct RuntimeRegistrationToken {
    token: u64,
    manifest_entry_index: u32,
    thunk: MethodAddress,
    capability: crate::execution::ResumableEntryCapability,
    code_generation_identity: u64,
}

#[derive(Clone, Copy)]
struct RuntimeRegistrationClaim {
    thunk: MethodAddress,
    capability: crate::execution::ResumableEntryCapability,
}

#[derive(Clone, Copy)]
struct StagedResumablePublication {
    vtable: Oop,
    selector: Oop,
    installed: Option<NativeEntry>,
    previous: Option<NativeEntry>,
}

struct CommittedResumableEntry {
    manifest_entry_index: u32,
    implementation_kind: ImplementationKind,
    thunk: MethodAddress,
    capability: crate::execution::ResumableEntryCapability,
}

struct CommittedResumableUnit {
    authority: [u8; 32],
    manifest_bytes: Vec<u8>,
    code_generation_identity: u64,
    entries: Vec<CommittedResumableEntry>,
}

#[derive(Clone, Copy)]
struct BlockVtableCatalogEntry {
    vtable: Oop,
    authority: [u8; 32],
    manifest_entry_index: u32,
    capability: crate::execution::ResumableEntryCapability,
    code_generation_identity: u64,
    drifted: bool,
}

/// One runtime owned by the thread that constructed it.
pub(crate) struct Runtime {
    id_storage: Option<id_storage::Storage>,
    owner: u64,
    observation: storage::ObservationState,
    collection_phase: crate::collection_phase::Service,
    state: UnsafeCell<RuntimeState>,
    entry_scopes: UnsafeCell<Vec<MachineCapability>>,
    native_retirements: core::cell::RefCell<Vec<NativeRetirement>>,
    draining_retirements: Cell<bool>,
    // Design 74A §3.2: the vtable mutation epoch at which the guarded-slot
    // conditions 2–4 were last verified, and their verdict at that epoch.
    slot_guard_cache: crate::guard_verdict::Service,
    lookup_guard_cache: crate::guard_verdict::Service,
    dispatch_cache: dispatch_cache::Cache,
    native_tier: core::cell::RefCell<native_tier::Registry>,
    numeric: core::cell::RefCell<numeric::Registry>,
    integer: Cell<Option<integer::Binding>>,
    float: Cell<Option<float::Binding>>,
    word: Cell<Option<word::Binding>>,
    string: Cell<Option<string::Binding>>,
    trace: Cell<Option<birnam_abi::trace_live::TraceLiveV1>>,
    collect: Cell<Option<birnam_abi::collect::CollectV1>>,
    #[cfg(test)]
    test_integer: UnsafeCell<Option<integer::TestOwner>>,
    #[cfg(test)]
    test_float: UnsafeCell<Option<float::TestOwner>>,
    #[cfg(test)]
    test_word: UnsafeCell<Option<word::TestOwner>>,
    #[cfg(test)]
    test_string: UnsafeCell<Option<string::TestOwner>>,
    recovery_requests: core::cell::RefCell<Vec<recovery::Request>>,
    #[cfg(test)]
    deopt_trial: core::cell::RefCell<Option<deopt_experiment::Trial>>,
    #[cfg(all(test, target_os = "macos"))]
    generated_deopt_trial: core::cell::RefCell<Option<generated_deopt_experiment::Trial>>,
    #[cfg(all(test, target_os = "macos"))]
    read_reuse_gc_epoch: Cell<u64>,
}

use crate::collection_phase::Phase as CollectionPhase;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CodeGenerationRetirement {
    Rejected,
    Retired,
    Deferred,
}

impl CodeGenerationRetirement {
    pub(crate) fn abi_code(self) -> u32 {
        match self {
            Self::Rejected => 0,
            Self::Retired => 1,
            Self::Deferred => 2,
        }
    }
}

struct CollectionGuard<'a> {
    phase: &'a crate::collection_phase::Service,
}

struct StacklessDriveRootGuard<'a> {
    runtime: &'a Runtime,
    machine: Oop,
}

/// Design 74D: one Oop held on the scoped host-root stack for a dynamic
/// extent, popped in LIFO order on drop even if the extent unwinds.
struct ScopedOopRootGuard<'a> {
    runtime: &'a Runtime,
    oop: Oop,
}

impl<'a> ScopedOopRootGuard<'a> {
    fn new(runtime: &'a Runtime, oop: Oop) -> Self {
        runtime.push_stackless_scoped_root(oop);
        Self { runtime, oop }
    }
}

impl Drop for ScopedOopRootGuard<'_> {
    fn drop(&mut self) {
        self.runtime.pop_stackless_scoped_root(self.oop);
    }
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

pub(crate) struct ManagedTurnGuard<'a> {
    runtime: &'a Runtime,
}

struct NativeRetirement {
    context: *mut core::ffi::c_void,
    release: birnam_abi::private::NativeRetirementFn,
}

struct RetirementDrainGuard<'a>(&'a Cell<bool>);

impl Drop for RetirementDrainGuard<'_> {
    fn drop(&mut self) { self.0.set(false); }
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

impl Drop for ManagedTurnGuard<'_> {
    fn drop(&mut self) {
        // End logical protection only after returning to pinned code. Physical
        // release requires a separate checkpoint with the outward result rooted.
        unsafe {
            (&mut *self.runtime.state.get())
                .execution
                .end_managed_turn()
                .unwrap_or_else(|_| boundary::fatal_invariant("managed turn underflow"));
        }
    }
}

impl Drop for CollectionGuard<'_> {
    fn drop(&mut self) {
        self.phase.set(CollectionPhase::Idle);
    }
}

impl Runtime {
    pub(crate) fn enter_managed_turn(&self) -> ManagedTurnGuard<'_> {
        self.assert_owner();
        // SAFETY: owner-thread scalar transition; returned guard owns no state
        // borrow and may span metacircular lookup and immediate invocation.
        unsafe {
            (&mut *self.state.get())
                .execution
                .begin_managed_turn()
                .unwrap_or_else(|_| boundary::fatal_invariant("managed turn depth overflow"));
        }
        ManagedTurnGuard { runtime: self }
    }

    pub(crate) fn new(
        heap: Heap,
        vtables: Behaviors,
        symbols: crate::owned::Owned<Symbols>,
        id_protocol: Option<Box<crate::native_id::State>>,
        globals: DispatchGlobals,
    ) -> Self {
        Self::new_with_owners(heap, vtables, symbols, id_protocol, globals, None, None, None)
    }
    pub(crate) fn new_with_owners(
        heap: Heap,
        vtables: Behaviors,
        symbols: crate::owned::Owned<Symbols>,
        id_protocol: Option<Box<crate::native_id::State>>,
        globals: DispatchGlobals,
        roots: Option<crate::root_registry::Service>,
        namespace: Option<Environment>,
        model: Option<crate::object_model::Shared>,
    ) -> Self {
        let id_storage = id_protocol.as_ref().map(|id| id_storage::Storage::new(id.protocol));
        Self {
            id_storage,
            owner: current_thread_token(),
            observation: storage::ObservationState::new(),
            collection_phase: crate::collection_phase::Service::new(),
            entry_scopes: UnsafeCell::new(Vec::new()),
            native_retirements: core::cell::RefCell::new(Vec::new()),
            draining_retirements: Cell::new(false),
            slot_guard_cache: crate::guard_verdict::Service::new(),
            lookup_guard_cache: crate::guard_verdict::Service::new(),
            dispatch_cache: dispatch_cache::Cache::default(),
            native_tier: core::cell::RefCell::new(native_tier::Registry::default()),
            numeric: core::cell::RefCell::new(numeric::Registry::default()),
            integer: Cell::new(None),
            float: Cell::new(None),
            word: Cell::new(None),
            string: Cell::new(None),
            #[cfg(not(test))]
            trace: Cell::new(None),
            #[cfg(test)]
            trace: Cell::new(Some(birnam_trace::live::birnam_trace_live_v1)),
            #[cfg(not(test))]
            collect: Cell::new(None),
            #[cfg(test)]
            collect: Cell::new(Some(birnam_collect::birnam_collect_v1)),
            #[cfg(test)]
            test_integer: UnsafeCell::new(None),
            #[cfg(test)]
            test_float: UnsafeCell::new(None),
            #[cfg(test)]
            test_word: UnsafeCell::new(None),
            #[cfg(test)]
            test_string: UnsafeCell::new(None),
            recovery_requests: core::cell::RefCell::new(Vec::new()),
            #[cfg(test)]
            deopt_trial: core::cell::RefCell::new(None),
            #[cfg(all(test, target_os = "macos"))]
            generated_deopt_trial: core::cell::RefCell::new(None),
            #[cfg(all(test, target_os = "macos"))]
            read_reuse_gc_epoch: Cell::new(0),
            state: UnsafeCell::new(RuntimeState {
                method_observation: crate::method_observation::Service::new(),
                root_registry: roots.unwrap_or_else(crate::root_registry::Service::new),
                vtable_enumeration: model.map(crate::vtable_enumeration::Service::from_model).unwrap_or_else(crate::vtable_enumeration::Service::new),
                symbols,
                id_protocol,
                vtables,
                heap,
                globals,
                canonical: None,
                internal_dictionaries: None,
                symbol_compaction_enabled: false,
                symbol_compatibility_limits: None,
                metadata: MetadataShell::new(),
                environment: namespace.unwrap_or_else(Environment::new),
                execution: ExecutionState::new(current_thread_token()),
                staged_resumable_unit: None,
                staged_resumable_claims: None,
                staged_resumable_publications: None,
                staged_catalog_drift: None,
                committed_resumable_units: Vec::new(),
                block_vtable_catalog: Vec::new(),
                registration_tokens: Vec::new(),
                next_registration_token: 1,
                suppressed_registration_generation: None,
                pending_stackless_retirements: Vec::new(),
            }),
        }
    }

    fn assert_owner(&self) {
        self.require_storage_access();
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

    pub(crate) unsafe fn lookup_native_method_active(&self, vtable: Oop, selector: Oop) -> Option<NativeEntry> {
        unsafe { (*self.state.get()).vtables.native_lookup(vtable, selector) }
    }

    pub(crate) fn add_method(&self, vtable: Oop, selector: Oop, method: Method) -> AddMethod {
        let entry = crate::legacy_method::encode(method, selector == self.globals().lookup);
        AddMethod::from_native(self.add_native_method(vtable, selector, entry))
    }

    pub(crate) fn add_native_method(&self, vtable: Oop, selector: Oop, entry: NativeEntry) -> crate::vtable::AddMethod {
        self.assert_owner();
        let (direct, notify, observation) = unsafe {
            let state = &*self.state.get();
            let direct = state.vtables.native_direct_lookup(vtable, selector).is_some();
            let (notify, observation) = state.method_observation.notification();
            (direct, notify, observation)
        };
        // Copy observation selection before invoking a callback. The actual
        // native entry is mutated once, preserving both code and context.
        unsafe { notify(observation, i32::from(direct), vtable, selector) };
        let result = unsafe { (&mut *self.state.get()).vtables.add_native_method(vtable, selector, entry) };
        let previous = match result { crate::vtable::AddMethod::Added => None, crate::vtable::AddMethod::Overwrote(previous) => Some(previous) };
        if previous != Some(entry) {
            let state = unsafe { &mut *self.state.get() };
            let staged = state.staged_resumable_publications.is_some();
            let previous_legacy = previous.and_then(crate::legacy_method::try_decode);
            let added_legacy = crate::legacy_method::try_decode(entry);
            if let Some(previous) = previous_legacy.filter(|previous| Some(*previous) != added_legacy) {
                let removed = if staged {
                    state.execution.note_staged_direct_binding_removed(previous)
                } else { state.execution.note_direct_binding_removed(previous) };
                removed.unwrap_or_else(|_| boundary::fatal_invariant("resumable binding removal accounting failed"));
                note_block_vtable_replaced(state, vtable, previous);
            }
            if let Some(added) = added_legacy.filter(|added| Some(*added) != previous_legacy) {
                state.execution.note_direct_binding_added(added)
                    .unwrap_or_else(|_| boundary::fatal_invariant("resumable binding addition accounting failed"));
                note_block_vtable_bound(state, vtable, added);
            }
            if let Some(publications) = state.staged_resumable_publications.as_mut() {
                publications.push(StagedResumablePublication { vtable, selector, installed: Some(entry), previous });
            }
        }
        result
    }

    pub(crate) fn remove_method(&self, vtable: Oop, selector: Oop) -> Option<MethodAddress> {
        self.remove_native_method(vtable, selector).map(crate::legacy_method::decode)
    }
    pub(crate) fn remove_native_method(&self, vtable: Oop, selector: Oop) -> Option<NativeEntry> {
        self.assert_owner();
        let (direct, notify, observation) = unsafe {
            let state = &*self.state.get();
            let direct = state.vtables.native_direct_lookup(vtable, selector).is_some();
            let (notify, observation) = state.method_observation.notification();
            (direct, notify, observation)
        };
        unsafe { notify(observation, i32::from(direct), vtable, selector) };
        let removed = unsafe { (&mut *self.state.get()).vtables.remove_native_method(vtable, selector) };
        if let Some(entry) = removed {
            let state = unsafe { &mut *self.state.get() };
            let staged = state.staged_resumable_publications.is_some();
            if let Some(method) = crate::legacy_method::try_decode(entry) {
                let result = if staged { state.execution.note_staged_direct_binding_removed(method) }
                    else { state.execution.note_direct_binding_removed(method) };
                result.unwrap_or_else(|_| boundary::fatal_invariant("resumable binding removal accounting failed"));
                note_block_vtable_replaced(state, vtable, method);
            }
            if let Some(publications) = state.staged_resumable_publications.as_mut() {
                publications.push(StagedResumablePublication { vtable, selector, installed: None, previous: Some(entry) });
            }
        }
        removed
    }

    pub(crate) fn method_observation(&self) -> *mut birnam_abi::method_observation::ApiV1 {
        self.assert_owner();
        unsafe { (*self.state.get()).method_observation.api() }
    }

    pub(crate) fn admit_method_observation(&self) { self.assert_owner(); }

    pub(crate) fn create_vtable(&self, parent: Oop) -> Oop {
        self.assert_owner();
        self.require_allocation_allowed();
        self.maybe_collect_before_allocation();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        state
            .vtables
            .create_collectable(&mut state.heap, parent, state.globals.vtable_vt)
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

    /// Preserve the allocation phase check and collection opportunity that
    /// preceded private-behavior selection before its extraction (Design 81J).
    pub(crate) fn prepare_behavior_mutation(&self, object: Oop) -> Oop {
        self.assert_owner();
        self.require_allocation_allowed();
        self.maybe_collect_before_allocation();
        let current = self.checked_vtable_of(object);
        boundary::require_invariant(
            !current.is_null(),
            "behavior mutation receiver is not a floor object",
        );
        current
    }

    /// Id owns object headers and the existing mutation epoch. No convenience
    /// policy or private-owner bookkeeping is performed by this primitive.
    pub(crate) fn reassociate_behavior(&self, object: Oop, behavior: Oop) {
        self.assert_owner();
        self.require_allocation_allowed();
        // SAFETY: owner-thread access, with no callback or allocation in scope.
        let state = unsafe { &mut *self.state.get() };
        boundary::require_invariant(
            state.heap.payload_size(behavior).is_some(),
            "behavior is not a floor object",
        );
        boundary::require_invariant(
            state.heap.set_vtable(object, behavior),
            "behavior receiver is not a floor object",
        );
        state.vtables.note_mutation();
    }

    #[cfg(test)]
    pub(crate) fn ensure_private_vtable(&self, object: Oop) -> Oop {
        birnam_object_model::ensure_private_vtable(self, object)
    }

    /// Design 74A §3: may generated code read or write `machine`'s current
    /// Frame slots directly instead of sending `oopAt:` / `oopAt:put:`?
    ///
    /// Conditions 2–4 (canonical Machine vtable entries are the floor Methods,
    /// no custom lookup on that vtable, and the floor lookup primitive on
    /// `vtable_vt`) depend only on vtable state and are re-verified whenever
    /// the vtable mutation epoch moved. Condition 1 (the receiver's own vtable
    /// is the canonical Machine vtable) is read on every call. The reads are
    /// private and non-effectful; an ineligible receiver takes the ordinary
    /// send, whose own checks then decide.
    pub(crate) fn guarded_slot_access_eligible(&self, machine: Oop) -> bool {
        self.assert_owner();
        // SAFETY: owner-thread serialization and immutable access only.
        let state = unsafe { &*self.state.get() };
        let Some(objects) = state.execution.objects() else {
            return false;
        };
        if Behaviors::checked_vtable_of(&state.heap, machine) != objects.machine_vtable {
            return false;
        }
        // Native registration can change invocation classification without a
        // dictionary mutation. Never bypass a registered resumable carrier,
        // including one using a canonical floor function's address.
        if [
            crate::execution::machine_oop_at as Method,
            crate::execution::machine_oop_at_put as Method,
            crate::dispatch::vtable_lookup_method as Method,
        ]
        .iter()
        .any(|method| {
            state
                .execution
                .has_resumable_thunk(MethodAddress::from_method(*method))
        }) {
            return false;
        }
        let epoch = state.vtables.mutation_epoch();
        let (verified_epoch, verified) = self.slot_guard_cache.get();
        let verified = if epoch != u64::MAX && verified_epoch == epoch {
            verified
        } else {
            let verdict = Self::slot_guard_conditions_hold(state, &objects);
            self.slot_guard_cache.set((epoch, verdict));
            verdict
        };
        verified
    }

    fn slot_guard_conditions_hold(
        state: &RuntimeState,
        objects: &crate::execution::ExecutionObjects,
    ) -> bool {
        let globals = state.globals;
        let machine_vt = objects.machine_vtable;
        let canonical_read =
            MethodAddress::from_method(crate::execution::machine_oop_at as crate::abi::Method);
        let canonical_write =
            MethodAddress::from_method(crate::execution::machine_oop_at_put as crate::abi::Method);
        let floor_lookup =
            MethodAddress::from_method(crate::dispatch::vtable_lookup_method as crate::abi::Method);
        state.vtables.direct_lookup(machine_vt, objects.oop_at) == Some(canonical_read)
            && state.vtables.direct_lookup(machine_vt, objects.oop_at_put) == Some(canonical_write)
            && Behaviors::checked_vtable_of(&state.heap, machine_vt) == globals.vtable_vt
            && Behaviors::checked_vtable_of(&state.heap, globals.vtable_vt) == globals.vtable_vt
            && state
                .vtables
                .direct_lookup(globals.vtable_vt, globals.lookup)
                == Some(floor_lookup)
    }

    /// Guarded direct slot read: `None` when ineligible (the caller performs
    /// the ordinary send exactly once); otherwise exactly what the canonical
    /// `oopAt:` Method would have answered, including nil on every failure.
    pub(crate) fn guarded_frame_oop_at(&self, machine: Oop, operand: Oop) -> Option<Oop> {
        if !self.guarded_slot_access_eligible(machine) {
            return None;
        }
        let nil = self.canonical().nil;
        let Some(index) = crate::execution::nonnegative_index(self, operand) else {
            return Some(nil);
        };
        Some(self.stackless_oop_at(machine, index).unwrap_or(nil))
    }

    /// Guarded direct slot write; same contract as `guarded_frame_oop_at`.
    pub(crate) fn guarded_frame_oop_at_put(
        &self,
        machine: Oop,
        operand: Oop,
        value: Oop,
    ) -> Option<Oop> {
        if !self.guarded_slot_access_eligible(machine) {
            return None;
        }
        let nil = self.canonical().nil;
        let Some(index) = crate::execution::nonnegative_index(self, operand) else {
            return Some(nil);
        };
        Some(
            self.stackless_oop_at_put(machine, index, value)
                .unwrap_or(nil),
        )
    }

    /// Design 74D (E4): guarded direct slot read with an unboxed index. `None`
    /// when ineligible; otherwise exactly what `guarded_frame_oop_at` answers
    /// for a boxed Integer carrying `index`, without creating that box.
    pub(crate) fn guarded_frame_slot_at(&self, machine: Oop, index: i64) -> Option<Oop> {
        if !self.guarded_slot_access_eligible(machine) {
            return None;
        }
        let nil = self.canonical().nil;
        let Ok(index) = usize::try_from(index) else {
            return Some(nil);
        };
        Some(self.stackless_oop_at(machine, index).unwrap_or(nil))
    }

    /// Design 74D (E4): write form of `guarded_frame_slot_at`.
    pub(crate) fn guarded_frame_slot_at_put(
        &self,
        machine: Oop,
        index: i64,
        value: Oop,
    ) -> Option<Oop> {
        if !self.guarded_slot_access_eligible(machine) {
            return None;
        }
        let nil = self.canonical().nil;
        let Ok(index) = usize::try_from(index) else {
            return Some(nil);
        };
        Some(
            self.stackless_oop_at_put(machine, index, value)
                .unwrap_or(nil),
        )
    }

    /// The canonical `oopAt:` / `oopAt:put:` selectors: the execution objects'
    /// Symbols (strong roots) when installed, else the interned name.
    fn slot_access_selector(&self, write: bool) -> Oop {
        // SAFETY: owner-thread serialization and immutable access only.
        let objects = unsafe { (*self.state.get()).execution.objects() };
        match objects {
            Some(objects) if write => objects.oop_at_put,
            Some(objects) => objects.oop_at,
            None => self
                .intern(if write { b"oopAt:put:" } else { b"oopAt:" })
                .unwrap_or_else(|_| boundary::fatal_invariant("static selector interns")),
        }
    }

    /// Design 74D §2.2: the ordinary `oopAt:` send with the baseline's
    /// operands, performed inside the floor on a guard miss. The box is created
    /// from the same constant generated code would have boxed, immediately
    /// before the send, and reaches the send's argument buffer as the
    /// baseline's `MKV` result does.
    pub(crate) fn fallback_frame_oop_at(&self, machine: Oop, index: i64) -> Oop {
        let selector = self.slot_access_selector(false);
        let operand = self.make_integer(index);
        self.send_slice(machine, selector, &[operand])
    }

    /// Design 74D §2.4: the ordinary `oopAt:put:` send on a guard miss. The
    /// incoming value is only a generated-code local, so it is rooted on the
    /// scoped host-root stack before anything here can allocate; the fresh
    /// index box is rooted the same way across the send. Both roots unwind
    /// through `Drop`, as every other user of that stack does.
    pub(crate) fn fallback_frame_oop_at_put(&self, machine: Oop, index: i64, value: Oop) -> Oop {
        let _value_root = ScopedOopRootGuard::new(self, value);
        #[cfg(test)]
        if FORCE_COLLECT_IN_SLOT_FALLBACK.with(|flag| flag.replace(false)) {
            let _ = self.gc_collect_active();
        }
        let selector = self.slot_access_selector(true);
        let operand = self.make_integer(index);
        let _operand_root = ScopedOopRootGuard::new(self, operand);
        self.send_slice(machine, selector, &[operand, value])
    }

    pub(crate) fn allocate_empty(&self, vtable: Oop, permanence: Permanence) -> Oop {
        self.assert_owner();
        self.require_allocation_allowed();
        self.maybe_collect_before_allocation();
        // SAFETY: owner-thread serialization and no foreign call in this scope.
        let state = unsafe { &mut *self.state.get() };
        crate::values::allocate_empty(&mut state.heap, vtable, permanence)
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
        // Initialize the legacy weak view from the already populated owner.
        for binding in state.environment.roots().chunks_exact(2) {
            state.vtables.add_weak_opaque_binding(environment, binding[0], binding[1]);
        }
        for index in 0..state.symbols.count() {
            let symbol = state.symbols.at(index);
            state
                .vtables
                .add_weak_opaque_binding(symbols, symbol, core::ptr::null_mut());
        }
    }

    /// Add completed observation/mirror policy to the native publication method.
    /// The early Namespace already owns and roots the same binding table.
    pub(crate) fn install_namespace_runtime_extension(&self) {
        let Some(receiver) = (unsafe { (*self.state.get()).environment.native_receiver() }) else { return; };
        unsafe extern "C" fn publish(context: *mut core::ffi::c_void, _: Oop, args: *const birnam_abi::id_protocol::Operand, count: usize) -> birnam_abi::id_protocol::Outcome {
            use birnam_abi::id_protocol::{self as native, Outcome, Operand};
            let invalid = || Outcome::failed(native::INVALID_REQUEST, 0);
            let Some([name, value]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid(); };
            let Some(name) = name.as_object() else { return invalid(); };
            let object = if value.kind() == native::UNIT { core::ptr::null_mut() }
                else { let Some(object) = value.as_object().filter(|object| !object.is_null()) else { return invalid(); }; object };
            let runtime = unsafe { &*context.cast::<Runtime>() };
            let id = unsafe { (*runtime.state.get()).id_protocol.as_ref().unwrap().protocol };
            let selector = runtime.intern(b"symbolBytesOf:").unwrap();
            let argument = Operand::object(name);
            let result = unsafe { id.message.unwrap()(id.context, id.receiver, selector, &argument, 1) };
            if result.status != native::LOADED { return result; }
            let Some(bytes) = result.value.as_bytes() else { return invalid(); };
            let Some(bytes) = (unsafe { birnam_abi::borrowed_slice(bytes.pointer, bytes.length) }) else { return invalid(); };
            let bytes = bytes.to_vec();
            match runtime.export(&bytes, object) {
                Ok(object) => Outcome::loaded(if object.is_null() { Operand::unit() } else { Operand::object(object) }),
                Err(_) => invalid(),
            }
        }
        let selector = self.intern(b"bindingAt:put:").unwrap();
        self.add_native_method(self.checked_vtable_of(receiver), selector,
            NativeEntry::new(publish, core::ptr::from_ref(self).cast_mut().cast()));
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
        // SAFETY: owner-thread serialization. The returned bytes are borrowed
        // from the Symbol record and remain valid only while its Oop is rooted.
        unsafe { (*self.state.get()).symbols.c_name_of(symbol) }
    }

    /// Borrow a Symbol name after `enter` has already established this runtime
    /// as active on the owner thread.
    pub(crate) unsafe fn symbol_name_active(&self, symbol: Oop) -> *const core::ffi::c_char {
        // SAFETY: the active-runtime guard proves owner-thread serialization.
        // The caller must keep the Symbol Oop rooted for the pointer's use.
        unsafe { (*self.state.get()).symbols.c_name_of(symbol) }
    }

    pub(crate) fn import(&self, bytes: &[u8]) -> Result<Oop, EmbeddedNul> {
        self.assert_owner();
        // Copy selection before entering Namespace. No RuntimeState/Environment
        // borrow survives its restricted interner callback, and no movable
        // Runtime address is retained by the service.
        let (lookup, namespace, find) = unsafe { (*self.state.get()).environment.lookup() };
        let mut value = core::ptr::null_mut();
        let status = unsafe {
            lookup(namespace, find, self.state.get().cast(), namespace_intern,
                   bytes.as_ptr(), bytes.len(), &mut value)
        };
        match status {
            0 => Err(EmbeddedNul),
            1 => Ok(value),
            _ => boundary::fatal_invariant("Namespace lookup returned invalid status"),
        }
    }

    pub(crate) fn export(&self, bytes: &[u8], value: Oop) -> Result<Oop, EmbeddedNul> {
        self.assert_owner();
        // Copy selection before either restricted primitive borrows RuntimeState.
        // No movable Runtime pointer or state reference crosses the call.
        let (publish, namespace, bind) = unsafe { (*self.state.get()).environment.publication() };
        let mut exported = core::ptr::null_mut();
        let status = unsafe {
            publish(namespace, bind, self.state.get().cast(), namespace_intern,
                    namespace_mirror, bytes.as_ptr(), bytes.len(), value, &mut exported)
        };
        match status {
            0 => Err(EmbeddedNul),
            1 => Ok(exported),
            _ => boundary::fatal_invariant("Namespace publication returned invalid status"),
        }
    }

    pub(crate) fn namespace_enumeration(&self) -> *const birnam_abi::namespace_enumeration::ApiV1 {
        self.assert_owner();
        unsafe { (*self.state.get()).environment.enumeration() }
    }
    pub(crate) fn namespace_interner_context(&self) -> *mut core::ffi::c_void {
        self.assert_owner();
        self.state.get().cast()
    }
    pub(crate) fn namespace_export(&self) -> *mut birnam_abi::namespace_export::ApiV1 {
        self.assert_owner();
        unsafe { (*self.state.get()).environment.exports() }
    }
    pub(crate) fn canonical_namespace_export(&self) -> birnam_abi::namespace_export::ApiV1 {
        self.assert_owner();
        unsafe { (*self.state.get()).environment.canonical_exports() }
    }
    pub(crate) fn namespace_import(&self) -> *mut birnam_abi::namespace_import::ApiV1 {
        self.assert_owner();
        unsafe { (*self.state.get()).environment.imports() }
    }
    pub(crate) fn canonical_namespace_import(&self) -> birnam_abi::namespace_import::ApiV1 {
        self.assert_owner();
        unsafe { (*self.state.get()).environment.canonical_imports() }
    }
    pub(crate) fn namespace_presence(&self) -> *mut birnam_abi::namespace_presence::ApiV1 {
        self.assert_owner();
        unsafe { (*self.state.get()).environment.presence() }
    }
    pub(crate) fn canonical_namespace_presence(&self) -> birnam_abi::namespace_presence::ApiV1 {
        self.assert_owner();
        unsafe { (*self.state.get()).environment.canonical_presence() }
    }
    pub(crate) fn namespace_names(&self) -> *mut birnam_abi::namespace_names::ApiV1 {
        self.assert_owner();
        unsafe { (*self.state.get()).environment.names() }
    }
    pub(crate) fn canonical_namespace_names(&self) -> birnam_abi::namespace_names::ApiV1 {
        self.assert_owner();
        unsafe { (*self.state.get()).environment.canonical_names() }
    }
    pub(crate) fn namespace_name_preflight(&self) {
        self.assert_owner();
    }
    /// Caller has performed preflight, with no intervening Id call or callback.
    pub(crate) unsafe fn namespace_symbol_bytes(&self, symbol: Oop) -> *const core::ffi::c_char {
        unsafe { (*self.state.get()).symbols.c_name_of(symbol) }
    }

    #[cfg(test)]
    pub(crate) fn exported_name(&self, object: Oop) -> *const core::ffi::c_char {
        self.assert_owner();
        // SAFETY: owner-thread serialization. Environment strongly owns the
        // returned name Symbol while the export remains installed.
        let state = unsafe { &*self.state.get() };
        let symbol = state.environment.exported_name_of(object);
        state.symbols.c_name_of(symbol)
    }

    pub(crate) fn publish_canonical_exports(&self) {
        let globals = self.globals();
        let canonical = self.canonical();
        for (name, value) in [
            (&b"Object"[..], canonical.object_prototype),
            (&b"Boolean"[..], canonical.true_object),
            (&b"False"[..], canonical.false_object),
            (&b"Vtable"[..], globals.vtable_vt),
            (&b"Symbol"[..], canonical.symbol_prototype),
            (&b"SymbolVt"[..], globals.symbol_vt),
            (&b"ObjectVt"[..], globals.object_vt),
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
        let continuation_vtable = self.create_vtable(globals.object_vt);
        let continue_action_vtable = self.create_vtable(globals.object_vt);
        let call_action_vtable = self.create_vtable(globals.object_vt);
        let tail_call_action_vtable = self.create_vtable(globals.object_vt);
        let return_action_vtable = self.create_vtable(globals.object_vt);
        let suspend_action_vtable = self.create_vtable(globals.object_vt);
        let capture_action_vtable = self.create_vtable(globals.object_vt);
        let restore_action_vtable = self.create_vtable(globals.object_vt);
        let abort_action_vtable = self.create_vtable(globals.object_vt);
        let boundary_call_action_vtable = self.create_vtable(globals.object_vt);
        let exit_boundary_action_vtable = self.create_vtable(globals.object_vt);
        let boundary_call_action_prototype =
            self.allocate_empty(boundary_call_action_vtable, Permanence::Permanent);
        let exit_boundary_action_prototype =
            self.allocate_empty(exit_boundary_action_vtable, Permanence::Permanent);
        let boundary_call_kind = self.intern(b"boundaryCall").expect("static symbol");
        let exit_boundary_kind = self.intern(b"exitBoundary").expect("static symbol");
        let machine_prototype = self.allocate_empty(machine_vtable, Permanence::Permanent);
        let frame_view_prototype = self.allocate_empty(frame_view_vtable, Permanence::Permanent);
        let continuation_prototype =
            self.allocate_empty(continuation_vtable, Permanence::Permanent);
        let continue_action_prototype =
            self.allocate_empty(continue_action_vtable, Permanence::Permanent);
        let call_action_prototype = self.allocate_empty(call_action_vtable, Permanence::Permanent);
        let tail_call_action_prototype =
            self.allocate_empty(tail_call_action_vtable, Permanence::Permanent);
        let return_action_prototype =
            self.allocate_empty(return_action_vtable, Permanence::Permanent);
        let suspend_action_prototype =
            self.allocate_empty(suspend_action_vtable, Permanence::Permanent);
        let capture_action_prototype =
            self.allocate_empty(capture_action_vtable, Permanence::Permanent);
        let restore_action_prototype =
            self.allocate_empty(restore_action_vtable, Permanence::Permanent);
        let abort_action_prototype =
            self.allocate_empty(abort_action_vtable, Permanence::Permanent);
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
        let delimiter_index_selector = self
            .intern(b"delimiterIndex")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let delimiter_generation_selector = self
            .intern(b"delimiterGeneration")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let continuation_slot_selector = self
            .intern(b"continuationSlot")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let supplied_value_slot_selector = self
            .intern(b"suppliedValueSlot")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let reason_slot_selector = self
            .intern(b"reasonSlot")
            .unwrap_or_else(|_| boundary::fatal_invariant("static selector contains NUL"));
        let has_delimiter_selector = self
            .intern(b"hasDelimiter")
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
        let suspend_kind = self
            .intern(b"suspend")
            .unwrap_or_else(|_| boundary::fatal_invariant("static symbol contains NUL"));
        let capture_kind = self
            .intern(b"capture")
            .unwrap_or_else(|_| boundary::fatal_invariant("static symbol contains NUL"));
        let restore_kind = self
            .intern(b"restore")
            .unwrap_or_else(|_| boundary::fatal_invariant("static symbol contains NUL"));
        let abort_kind = self
            .intern(b"abort")
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
            (suspend_action_vtable, suspend_action_prototype),
            (capture_action_vtable, capture_action_prototype),
            (restore_action_vtable, restore_action_prototype),
            (abort_action_vtable, abort_action_prototype),
            (boundary_call_action_vtable, boundary_call_action_prototype),
            (exit_boundary_action_vtable, exit_boundary_action_prototype),
            (continuation_vtable, continuation_prototype),
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
        self.set_trace_hook(
            continuation_vtable,
            crate::execution::continuation_trace_hook,
        );
        self.gc_register_root_provider(crate::execution::execution_root_provider);
        for (name, object) in [
            (&b"Machine"[..], machine_prototype),
            (&b"FrameView"[..], frame_view_prototype),
            (&b"Continuation"[..], continuation_prototype),
            (&b"ContinueAction"[..], continue_action_prototype),
            (&b"CallAction"[..], call_action_prototype),
            (&b"TailCallAction"[..], tail_call_action_prototype),
            (&b"ReturnAction"[..], return_action_prototype),
            (&b"SuspendAction"[..], suspend_action_prototype),
            (&b"CaptureAction"[..], capture_action_prototype),
            (&b"RestoreAction"[..], restore_action_prototype),
            (&b"AbortAction"[..], abort_action_prototype),
            (&b"BoundaryCallAction"[..], boundary_call_action_prototype),
            (&b"ExitBoundaryAction"[..], exit_boundary_action_prototype),
        ] {
            self.export(name, object).unwrap_or_else(|_| {
                boundary::fatal_invariant("static execution export name contains NUL")
            });
        }
        for (selector, method) in [
            (
                self.intern(b"boundaryCallSite:").expect("static selector"),
                crate::execution::boundary::machine_boundary_call as crate::abi::Method,
            ),
            (
                self.intern(b"exitBoundaryAt:").expect("static selector"),
                crate::execution::boundary::machine_exit_boundary as crate::abi::Method,
            ),
            (
                self.intern(b"boundaryDeliveryPending")
                    .expect("static selector"),
                crate::execution::boundary::machine_delivery_pending as crate::abi::Method,
            ),
            (
                self.intern(b"acknowledgeBoundaryDelivery")
                    .expect("static selector"),
                crate::execution::boundary::machine_acknowledge_delivery as crate::abi::Method,
            ),
            (
                new_selector,
                crate::execution::machine_new as crate::abi::Method,
            ),
            (
                self.intern(b"recoverAt:with:").expect("static selector"),
                recovery::machine_recover as crate::abi::Method,
            ),
            (
                self.intern(b"recoverAt:withGraph:")
                    .expect("static selector"),
                recovery::machine_recover_graph as crate::abi::Method,
            ),
            (
                self.intern(b"suspendAt:continueAt:")
                    .expect("static selector"),
                crate::execution::machine_suspend as crate::abi::Method,
            ),
            (
                self.intern(b"captureAt:continueAt:")
                    .expect("static selector"),
                crate::execution::machine_capture as crate::abi::Method,
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
            suspend_action_vtable,
            capture_action_vtable,
            restore_action_vtable,
            abort_action_vtable,
            boundary_call_action_vtable,
            exit_boundary_action_vtable,
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
        let _ = self.add_method(
            exit_boundary_action_vtable,
            value_slot_selector,
            crate::execution::return_action_value_slot,
        );
        let _ = self.add_method(
            boundary_call_action_vtable,
            result_slot_selector,
            crate::execution::call_action_result_slot,
        );
        for vtable in [
            call_action_vtable,
            tail_call_action_vtable,
            boundary_call_action_vtable,
        ] {
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
        for (vtable, fields) in [
            (
                suspend_action_vtable,
                [
                    (
                        result_slot_selector,
                        crate::execution::action_word_four as crate::abi::Method,
                    ),
                    (
                        delimiter_index_selector,
                        crate::execution::action_word_five as crate::abi::Method,
                    ),
                    (
                        delimiter_generation_selector,
                        crate::execution::action_word_six as crate::abi::Method,
                    ),
                ],
            ),
            (
                capture_action_vtable,
                [
                    (
                        delimiter_index_selector,
                        crate::execution::action_word_four as crate::abi::Method,
                    ),
                    (
                        delimiter_generation_selector,
                        crate::execution::action_word_five as crate::abi::Method,
                    ),
                    (
                        result_slot_selector,
                        crate::execution::action_word_six as crate::abi::Method,
                    ),
                ],
            ),
        ] {
            for (selector, method) in fields {
                let _ = self.add_method(vtable, selector, method);
            }
        }
        for (selector, method) in [
            (
                continuation_slot_selector,
                crate::execution::action_word_four as crate::abi::Method,
            ),
            (
                supplied_value_slot_selector,
                crate::execution::action_word_five as crate::abi::Method,
            ),
        ] {
            let _ = self.add_method(restore_action_vtable, selector, method);
        }
        for (selector, method) in [
            (
                has_delimiter_selector,
                crate::execution::abort_action_has_delimiter as crate::abi::Method,
            ),
            (
                delimiter_index_selector,
                crate::execution::abort_action_delimiter_index as crate::abi::Method,
            ),
            (
                delimiter_generation_selector,
                crate::execution::abort_action_delimiter_generation as crate::abi::Method,
            ),
            (
                reason_slot_selector,
                crate::execution::action_word_six as crate::abi::Method,
            ),
        ] {
            let _ = self.add_method(abort_action_vtable, selector, method);
        }
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
                    continuation_vtable,
                    continuation_prototype,
                    continue_action_vtable,
                    continue_action_prototype,
                    call_action_vtable,
                    call_action_prototype,
                    tail_call_action_vtable,
                    tail_call_action_prototype,
                    return_action_vtable,
                    return_action_prototype,
                    suspend_action_vtable,
                    suspend_action_prototype,
                    capture_action_vtable,
                    capture_action_prototype,
                    restore_action_vtable,
                    restore_action_prototype,
                    abort_action_vtable,
                    abort_action_prototype,
                    boundary_call_action_vtable,
                    boundary_call_action_prototype,
                    exit_boundary_action_vtable,
                    exit_boundary_action_prototype,
                    boundary_call_kind,
                    exit_boundary_kind,
                    continue_kind,
                    call_kind,
                    tail_call_kind,
                    return_kind,
                    suspend_kind,
                    capture_kind,
                    restore_kind,
                    abort_kind,
                    oop_slot_kind,
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
        task_control::install(self, machine_vtable);
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
        let suspend_action =
            self.allocate_instance(objects.suspend_action_vtable, 7 * size_of::<usize>());
        let capture_action =
            self.allocate_instance(objects.capture_action_vtable, 7 * size_of::<usize>());
        let restore_action =
            self.allocate_instance(objects.restore_action_vtable, 6 * size_of::<usize>());
        let abort_action =
            self.allocate_instance(objects.abort_action_vtable, 7 * size_of::<usize>());
        let boundary_call_action =
            self.allocate_instance(objects.boundary_call_action_vtable, 9 * size_of::<usize>());
        let exit_boundary_action =
            self.allocate_instance(objects.exit_boundary_action_vtable, 5 * size_of::<usize>());
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
                suspend_action,
                capture_action,
                restore_action,
                abort_action,
            )?;
            let machine_state = state.execution.machine_mut(owner, capability)?;
            machine_state.boundary_call_action = boundary_call_action;
            machine_state.exit_boundary_action = exit_boundary_action;
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
        // Copy roots under the registry borrow. Foreign admission may mutate
        // heap marks and must run only after the state/heap references expire.
        let roots = unsafe {
            let state = &*self.state.get();
            let mut roots = Vec::new();
            if let Some(objects) = state.execution.objects() {
                state.execution.trace_object_roots(
                    &state.heap,
                    machine,
                    objects.machine_vtable,
                    |root| roots.push(root),
                );
            }
            roots
        };
        for root in roots {
            unsafe { mark(root) };
        }
    }

    pub(crate) fn trace_stackless_scoped_roots(&self, mark: MarkFn) {
        // The mark callback can write heap metadata; no state borrow crosses it.
        let roots = unsafe {
            let mut roots = Vec::new();
            (&*self.state.get())
                .execution
                .trace_scoped_host_roots(|root| roots.push(root));
            roots
        };
        for root in roots {
            unsafe { mark(root) };
        }
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

    pub(crate) fn trace_stackless_continuation(&self, continuation: Oop, mark: MarkFn) {
        // Snapshot the detached Frame roots before invoking foreign admission.
        let roots = unsafe {
            let state = &*self.state.get();
            let mut roots = Vec::new();
            state.execution.trace_continuation_roots(
                &state.heap,
                continuation,
                |root| roots.push(root),
            );
            roots
        };
        for root in roots {
            unsafe { mark(root) };
        }
    }

    /// Finalize a detached Continuation payload under the active Runtime.
    ///
    /// # Safety
    ///
    /// The heap must still own the detached allocation until this call returns.
    pub(crate) unsafe fn finalize_stackless_continuation(&self, continuation: Oop) {
        // SAFETY: owner-thread finalization provides exclusive execution-state
        // access, and the caller forwards the detached-payload lifetime.
        unsafe {
            (&mut *self.state.get())
                .execution
                .finalize_detached_continuation(continuation)
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

    pub(crate) fn stackless_suspend_current(
        &self,
        machine: Oop,
        result_slot: usize,
        next_pc: usize,
    ) -> Result<Oop, ExecutionError> {
        let capability = self.stackless_machine_capability(machine)?;
        // SAFETY: delimiter lookup and scratch publication are one
        // allocation-free owner-thread operation over disjoint fields.
        unsafe {
            let state = &mut *self.state.get();
            let delimiter = state
                .execution
                .current_delimiter(current_thread_token(), capability)?;
            state.execution.publish_suspend(
                &mut state.heap,
                current_thread_token(),
                capability,
                result_slot,
                delimiter,
                next_pc,
            )
        }
    }

    pub(crate) fn stackless_capture_current(
        &self,
        machine: Oop,
        result_slot: usize,
        next_pc: usize,
    ) -> Result<Oop, ExecutionError> {
        let capability = self.stackless_machine_capability(machine)?;
        // SAFETY: same short disjoint-field borrow as suspension publication.
        unsafe {
            let state = &mut *self.state.get();
            let delimiter = state
                .execution
                .current_delimiter(current_thread_token(), capability)?;
            state.execution.publish_capture(
                &mut state.heap,
                current_thread_token(),
                capability,
                delimiter,
                result_slot,
                next_pc,
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

    pub(crate) fn stackless_boundary_action(
        &self,
        machine: Oop,
        index: usize,
        exit: bool,
    ) -> Result<Oop, ExecutionError> {
        let capability = self.stackless_machine_capability(machine)?;
        // SAFETY: owner-thread serialized, no language call under this borrow.
        unsafe {
            let state = &mut *self.state.get();
            if exit {
                state.execution.publish_boundary_exit(
                    &mut state.heap,
                    current_thread_token(),
                    capability,
                    index,
                )
            } else {
                state.execution.publish_boundary_call(
                    &mut state.heap,
                    current_thread_token(),
                    capability,
                    index,
                )
            }
        }
    }

    pub(crate) fn stackless_boundary_delivery(
        &self,
        machine: Oop,
        acknowledge: bool,
    ) -> Result<Oop, ExecutionError> {
        let capability = self.stackless_machine_capability(machine)?;
        let pending = unsafe {
            let state = &mut *self.state.get();
            if acknowledge {
                state
                    .execution
                    .acknowledge_boundary_delivery(current_thread_token(), capability)?;
            }
            state
                .execution
                .boundary_delivery_pending(current_thread_token(), capability)?
        };
        Ok(if pending {
            self.canonical().true_object
        } else {
            self.canonical().false_object
        })
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

    pub(crate) fn stackless_action_word(&self, action: Oop, index: usize) -> Option<usize> {
        self.assert_owner();
        // SAFETY: owner-thread serialization and immutable access only.
        unsafe {
            let state = &*self.state.get();
            state.execution.action_word(&state.heap, action, index)
        }
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

    pub(crate) fn stackless_block_vtable_image_reference(
        &self,
        vtable: Oop,
        authority: [u8; 32],
    ) -> Option<u32> {
        self.assert_owner();
        // SAFETY: owner-thread immutable catalog lookup.
        let state = unsafe { &*self.state.get() };
        let mut matches = state.block_vtable_catalog.iter().filter(|entry| {
            entry.vtable == vtable
                && entry.authority == authority
                && !entry.drifted
                && state
                    .committed_resumable_units
                    .iter()
                    .any(|unit| unit.code_generation_identity == entry.code_generation_identity)
        });
        let entry = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        Some(entry.manifest_entry_index)
    }

    pub(crate) fn stackless_resumable_manifest_matches(
        &self,
        authority: [u8; 32],
        code_generation_identity: u64,
        manifest_bytes: &[u8],
    ) -> bool {
        self.assert_owner();
        // SAFETY: owner-thread immutable retained-manifest comparison.
        let state = unsafe { &*self.state.get() };
        let mut matches = state.committed_resumable_units.iter().filter(|unit| {
            unit.authority == authority
                && unit.code_generation_identity == code_generation_identity
                && unit.manifest_bytes == manifest_bytes
        });
        matches.next().is_some() && matches.next().is_none()
    }

    pub(crate) fn retire_stackless_code_generation(&self, code_generation_identity: u64) -> bool {
        self.request_stackless_code_generation_retirement(code_generation_identity)
            == CodeGenerationRetirement::Retired
    }

    pub(crate) fn request_stackless_code_generation_retirement(
        &self,
        code_generation_identity: u64,
    ) -> CodeGenerationRetirement {
        self.assert_owner();
        if code_generation_identity == 0 {
            return CodeGenerationRetirement::Rejected;
        }
        // Validate before withdrawing catalog ownership. A committed block
        // catalog is a strong resolver cache while its generation is admitted,
        // but it must not keep an otherwise quiescent generation alive during
        // its irreversible retirement attempt.
        let (staged, committed_count, registered, readers) = unsafe {
            let state = &mut *self.state.get();
            (
                state.staged_resumable_unit.as_ref().is_some_and(|unit| {
                    unit.code_generation_identity() == code_generation_identity
                }),
                state
                    .committed_resumable_units
                    .iter()
                    .filter(|unit| unit.code_generation_identity == code_generation_identity)
                    .count(),
                state
                    .execution
                    .has_code_generation(code_generation_identity),
                state.execution.has_managed_turn(),
            )
        };
        if staged || committed_count > 1 {
            return CodeGenerationRetirement::Rejected;
        }
        if committed_count == 0 {
            return if registered {
                CodeGenerationRetirement::Rejected
            } else {
                CodeGenerationRetirement::Retired
            };
        }
        if readers || self.collection_phase.get() != CollectionPhase::Idle {
            unsafe {
                let pending = &mut (&mut *self.state.get()).pending_stackless_retirements;
                if !pending.contains(&code_generation_identity) {
                    pending.push(code_generation_identity);
                }
            }
            return CodeGenerationRetirement::Deferred;
        }
        let (holders_clear, permanent_binding) = unsafe {
            let state = &*self.state.get();
            let methods = state
                .committed_resumable_units
                .iter()
                .find(|unit| unit.code_generation_identity == code_generation_identity)
                .map(|unit| {
                    unit.entries
                        .iter()
                        .map(|entry| entry.thunk)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            (
                state
                    .execution
                    .can_suppress_registration_roots(code_generation_identity),
                state.vtables.has_permanent_binding(&state.heap, &methods),
            )
        };
        let suppressible = holders_clear && !permanent_binding;
        if !suppressible {
            return CodeGenerationRetirement::Rejected;
        }
        let withdrawn = unsafe {
            let state = &mut *self.state.get();
            let mut withdrawn = Vec::new();
            state.block_vtable_catalog.retain(|entry| {
                if entry.code_generation_identity == code_generation_identity {
                    withdrawn.push(*entry);
                    false
                } else {
                    true
                }
            });
            withdrawn
        };
        unsafe {
            (&mut *self.state.get()).suppressed_registration_generation =
                Some(code_generation_identity)
        };
        let _ = self.gc_collect_active();
        unsafe { (&mut *self.state.get()).suppressed_registration_generation = None };
        // SAFETY: owner-thread quiescence provides one atomic registry
        // mutation. `unregister_code_generation` validates every holder before
        // changing any slot, so a false result leaves the unit authoritative.
        let state = unsafe { &mut *self.state.get() };
        let unregistered = state
            .execution
            .unregister_code_generation(code_generation_identity)
            .unwrap_or_else(|_| boundary::fatal_invariant("code-generation unregistration failed"));
        if !unregistered {
            state.block_vtable_catalog.extend(
                withdrawn
                    .into_iter()
                    .filter(|entry| state.vtables.is_registered(entry.vtable)),
            );
            if !state
                .pending_stackless_retirements
                .contains(&code_generation_identity)
            {
                state
                    .pending_stackless_retirements
                    .push(code_generation_identity);
            }
            return CodeGenerationRetirement::Rejected;
        }
        state
            .committed_resumable_units
            .retain(|unit| unit.code_generation_identity != code_generation_identity);
        state
            .registration_tokens
            .retain(|record| record.code_generation_identity != code_generation_identity);
        state
            .block_vtable_catalog
            .retain(|entry| entry.code_generation_identity != code_generation_identity);
        // Descriptor owners can outlive registration. Remove retired candidates
        // explicitly, and release native/loader owners outside the registry borrow.
        let retired = self
            .native_tier
            .borrow_mut()
            .retire_generation(code_generation_identity);
        drop(retired);
        CodeGenerationRetirement::Retired
    }

    fn drain_pending_stackless_retirements(&self) {
        self.assert_owner();
        let pending = unsafe {
            let state = &mut *self.state.get();
            if state.execution.has_managed_turn() {
                return;
            }
            core::mem::take(&mut state.pending_stackless_retirements)
        };
        for code_generation_identity in pending {
            if self.request_stackless_code_generation_retirement(code_generation_identity)
                != CodeGenerationRetirement::Retired
            {
                unsafe {
                    let pending = &mut (&mut *self.state.get()).pending_stackless_retirements;
                    if !pending.contains(&code_generation_identity) {
                        pending.push(code_generation_identity);
                    }
                }
            }
        }
    }

    pub(crate) fn stackless_block_vtable_for_manifest_index(
        &self,
        authority: [u8; 32],
        manifest_entry_index: u32,
    ) -> Option<Oop> {
        self.assert_owner();
        // SAFETY: owner-thread immutable catalog lookup.
        let state = unsafe { &*self.state.get() };
        let mut matches = state.block_vtable_catalog.iter().filter(|entry| {
            entry.authority == authority
                && entry.manifest_entry_index == manifest_entry_index
                && !entry.drifted
                && state
                    .committed_resumable_units
                    .iter()
                    .any(|unit| unit.code_generation_identity == entry.code_generation_identity)
        });
        let entry = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        Some(entry.vtable)
    }

    /// Resolve a BlockBody vtable created by the currently staged portable
    /// unit. Image-v7 recovery needs this before commit so it can allocate and
    /// patch cyclic Block graphs without publishing a partially reconstructed
    /// generation.
    pub(crate) fn staged_portable_block_vtable_for_manifest_index(
        &self,
        authority: [u8; 32],
        manifest_entry_index: u32,
    ) -> Option<Oop> {
        self.assert_owner();
        // SAFETY: owner-thread immutable catalog lookup while the staging
        // transaction excludes every other staged unit.
        let state = unsafe { &*self.state.get() };
        let staged = state.staged_resumable_unit.as_ref()?;
        if staged.authority() != authority
            || !staged.is_portable_entry(manifest_entry_index as usize)
        {
            return None;
        }
        let generation = staged.code_generation_identity();
        let mut matches = state.block_vtable_catalog.iter().filter(|entry| {
            entry.authority == authority
                && entry.manifest_entry_index == manifest_entry_index
                && entry.code_generation_identity == generation
                && !entry.drifted
        });
        let entry = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        Some(entry.vtable)
    }

    pub(crate) fn begin_staged_resumable_unit(
        &self,
        manifest: &[u8],
        authority: [u8; 32],
        code_generation_identity: u64,
        code_generation_lease: u64,
    ) -> bool {
        self.assert_owner();
        let Ok(staged) = StagedUnit::new(
            manifest,
            authority,
            code_generation_identity,
            code_generation_lease,
        ) else {
            return false;
        };
        // SAFETY: owner-thread point mutation with no callback.
        let state = unsafe { &mut *self.state.get() };
        if state.staged_resumable_unit.is_some()
            || state.staged_resumable_claims.is_some()
            || state.staged_resumable_publications.is_some()
            || state.staged_catalog_drift.is_some()
            || state
                .committed_resumable_units
                .iter()
                .any(|unit| unit.code_generation_identity == code_generation_identity)
        {
            return false;
        }
        state.staged_catalog_drift = Some(
            state
                .block_vtable_catalog
                .iter()
                .map(|entry| entry.drifted)
                .collect(),
        );
        state.staged_resumable_publications = Some(Vec::new());
        state.staged_resumable_claims = Some(vec![None; staged.entry_count()]);
        state.staged_resumable_unit = Some(staged);
        self.native_tier.borrow_mut().end_stage();
        true
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn begin_staged_portable_resumable_unit(
        &self,
        programs: &[u8],
        bindings: &[u8],
        selection: &[u8],
        manifest: &[u8],
        authority: [u8; 32],
        code_generation_identity: u64,
        code_generation_lease: u64,
    ) -> bool {
        self.assert_owner();
        let Ok(staged) = StagedUnit::new_portable(
            programs,
            bindings,
            selection,
            manifest,
            authority,
            code_generation_identity,
            code_generation_lease,
        ) else {
            return false;
        };
        // SAFETY: identical owner-thread staging transaction used by the
        // legacy source-manifest profile.
        let state = unsafe { &mut *self.state.get() };
        if state.staged_resumable_unit.is_some()
            || state.staged_resumable_claims.is_some()
            || state.staged_resumable_publications.is_some()
            || state.staged_catalog_drift.is_some()
            || state
                .committed_resumable_units
                .iter()
                .any(|unit| unit.code_generation_identity == code_generation_identity)
        {
            return false;
        }
        state.staged_catalog_drift = Some(
            state
                .block_vtable_catalog
                .iter()
                .map(|entry| entry.drifted)
                .collect(),
        );
        state.staged_resumable_publications = Some(Vec::new());
        state.staged_resumable_claims = Some(vec![None; staged.entry_count()]);
        state.staged_resumable_unit = Some(staged);
        self.native_tier
            .borrow_mut()
            .stage(programs, bindings, selection, manifest, authority);
        true
    }

    pub(crate) fn finish_staged_resumable_unit(&self) -> bool {
        self.close_staged_resumable_unit(true)
    }

    pub(crate) fn cancel_staged_resumable_unit(&self) -> bool {
        self.close_staged_resumable_unit(false)
    }

    fn close_staged_resumable_unit(&self, commit: bool) -> bool {
        self.assert_owner();
        self.native_tier.borrow_mut().end_stage();
        // SAFETY: owner-thread point mutation with no callback.
        let state = unsafe { &mut *self.state.get() };
        let Some(staged) = state.staged_resumable_unit.take() else {
            return false;
        };
        let claims = state.staged_resumable_claims.take();
        let publications = state.staged_resumable_publications.take();
        let catalog_drift = state.staged_catalog_drift.take();
        let code_generation_identity = staged.code_generation_identity();
        let authority = staged.authority();
        let manifest_bytes = staged.manifest_bytes().to_vec();
        let implementation_kinds = (0..staged.entry_count())
            .map(|index| staged.implementation_kind(index))
            .collect::<Option<Vec<_>>>();
        let complete_claims = claims
            .as_ref()
            .is_some_and(|claims| claims.iter().all(Option::is_some));
        if commit
            && staged.finish().is_ok()
            && complete_claims
            && implementation_kinds.is_some()
            && publications.is_some()
            && catalog_drift.is_some()
        {
            let claims = claims.expect("complete staged claims checked");
            let claim_thunks = claims
                .iter()
                .filter_map(|claim| claim.as_ref().map(|claim| claim.thunk))
                .collect::<Vec<_>>();
            for publication in publications.as_ref().expect("publication journal checked") {
                if let Some(previous) = publication.previous.and_then(crate::legacy_method::try_decode)
                    && publication.installed.and_then(crate::legacy_method::try_decode) != Some(previous)
                    && !claim_thunks.contains(&previous)
                {
                    state
                        .execution
                        .commit_staged_binding_removal(previous)
                        .unwrap_or_else(|_| {
                            boundary::fatal_invariant(
                                "staged prior binding retirement could not commit",
                            )
                        });
                }
            }
            for claim in claims.iter().filter_map(Option::as_ref) {
                state
                    .execution
                    .settle_staged_registration(claim.capability)
                    .unwrap_or_else(|_| {
                        boundary::fatal_invariant("staged registration could not commit")
                    });
            }
            let entries: Vec<CommittedResumableEntry> = claims
                .into_iter()
                .zip(implementation_kinds.expect("staged kinds checked"))
                .enumerate()
                .map(|(index, (claim, implementation_kind))| {
                    let claim = claim.expect("complete staged claim checked");
                    CommittedResumableEntry {
                        manifest_entry_index: index as u32,
                        implementation_kind,
                        thunk: claim.thunk,
                        capability: claim.capability,
                    }
                })
                .collect();
            state
                .registration_tokens
                .retain(|record| record.code_generation_identity != code_generation_identity);
            if !entries.is_empty() {
                state
                    .committed_resumable_units
                    .push(CommittedResumableUnit {
                        authority,
                        manifest_bytes,
                        code_generation_identity,
                        entries,
                    });
            }
            return true;
        }
        let Some(publications) = publications else {
            boundary::fatal_invariant("staged publication journal disappeared");
        };
        for publication in publications.into_iter().rev() {
            let installed_legacy = publication.installed.and_then(crate::legacy_method::try_decode);
            let previous_legacy = publication.previous.and_then(crate::legacy_method::try_decode);
            match publication.installed {
                Some(installed) => {
                    let removed = state.vtables.remove_native_method(publication.vtable, publication.selector)
                        .unwrap_or_else(|| boundary::fatal_invariant("staged installed binding disappeared during rollback"));
                    if removed != installed { boundary::fatal_invariant("staged installed binding changed before rollback"); }
                    if let Some(method) = installed_legacy.filter(|method| Some(*method) != previous_legacy) {
                        state.execution.note_staged_direct_binding_removed(method)
                            .unwrap_or_else(|_| boundary::fatal_invariant("staged installed binding accounting could not roll back"));
                    }
                }
                None => {
                    if state.vtables.native_direct_lookup(publication.vtable, publication.selector).is_some() {
                        boundary::fatal_invariant("staged removed binding was replaced before rollback");
                    }
                }
            }
            if let Some(previous) = publication.previous {
                if state.vtables.add_native_method(publication.vtable, publication.selector, previous) != crate::vtable::AddMethod::Added {
                    boundary::fatal_invariant("staged prior binding could not be restored during rollback");
                }
                if let Some(method) = previous_legacy.filter(|method| Some(*method) != installed_legacy) {
                    state.execution.note_direct_binding_added(method)
                        .unwrap_or_else(|_| boundary::fatal_invariant("staged prior binding accounting could not roll back"));
                }
            }
        }
        let Some(catalog_drift) = catalog_drift else {
            boundary::fatal_invariant("staged catalog snapshot disappeared");
        };
        state.block_vtable_catalog.truncate(catalog_drift.len());
        for (entry, drifted) in state.block_vtable_catalog.iter_mut().zip(catalog_drift) {
            entry.drifted = drifted;
        }
        if let Some(claims) = claims.as_ref() {
            for claim in claims.iter().filter_map(Option::as_ref) {
                state
                    .execution
                    .settle_staged_registration(claim.capability)
                    .unwrap_or_else(|_| {
                        boundary::fatal_invariant(
                            "staged registration could not return to unpublished state",
                        )
                    });
            }
        }
        let mut index = state.registration_tokens.len();
        while index != 0 {
            index -= 1;
            if state.registration_tokens[index].code_generation_identity != code_generation_identity
            {
                continue;
            }
            let record = state.registration_tokens.swap_remove(index);
            let _ = state
                .execution
                .unregister_unpublished_resumable(current_thread_token(), record.capability);
        }
        state
            .block_vtable_catalog
            .retain(|entry| entry.code_generation_identity != code_generation_identity);
        !commit
    }

    /// Consume one exact C registration carrier while a Runtime is active.
    ///
    /// # Safety
    ///
    /// The caller supplies exclusive writable access to one complete carrier;
    /// its descriptor pointer follows the synchronous Prepare contract.
    pub(crate) unsafe fn register_resumable_envelope<E: ResumableRegistrationEnvelope>(
        &self,
        envelope: &mut E,
    ) -> u32 {
        self.assert_owner();
        match envelope.operation() {
            PREPARE_V1 => {
                let (descriptor, optimizer_descriptor_bytes) = {
                    // SAFETY: immutable owner-thread access. The returned
                    // descriptor borrow is bounded by this synchronous call.
                    let state = unsafe { &*self.state.get() };
                    let Some(staged) = state.staged_resumable_unit.as_ref() else {
                        return crate::registration::REJECTED_V1;
                    };
                    // SAFETY: forwarded envelope contract.
                    let Ok((bytes, admission)) = (unsafe { staged.validate_prepare(envelope) })
                    else {
                        return crate::registration::REJECTED_V1;
                    };
                    let Some(step_method) = envelope.step_method() else {
                        return crate::registration::REJECTED_V1;
                    };
                    let descriptor = match admission {
                        DescriptorAdmission::Legacy(selection) => FrameDescriptor::admit_v2(
                            bytes,
                            selection,
                            envelope.code_generation_identity(),
                            step_method,
                        ),
                        DescriptorAdmission::Portable(row) => FrameDescriptor::admit_portable_v2(
                            bytes,
                            row,
                            envelope.code_generation_identity(),
                            step_method,
                        ),
                    };
                    let Ok(descriptor) = descriptor else {
                        return crate::registration::REJECTED_V1;
                    };
                    (descriptor, bytes.to_vec())
                };
                let optimizer_descriptor = descriptor.clone();
                let Some(binding) = descriptor.v2_entry_binding() else {
                    return crate::registration::REJECTED_V1;
                };
                if binding.argument_count != envelope.language_arity() as usize {
                    return crate::registration::REJECTED_V1;
                }
                let Some(thunk) = envelope.entry_thunk() else {
                    return crate::registration::REJECTED_V1;
                };
                let mut carrier = SemanticRegistrationV1::new(
                    thunk,
                    descriptor,
                    envelope.language_arity() as usize,
                    binding.receiver_slot,
                    binding.argument_start,
                )
                .with_generation_lease(crate::execution::RegistrationGenerationLease::new(
                    envelope.code_generation_identity(),
                ))
                .with_receiver_capture_offset_words(envelope.receiver_capture_offset_words());
                if !envelope.implementation_oop().is_null() {
                    carrier = carrier.with_implementation_anchor(envelope.implementation_oop());
                }

                // SAFETY: the immutable validation borrow ended above; this is
                // one owner-thread transfer into Runtime-owned state.
                let state = unsafe { &mut *self.state.get() };
                let Ok(capability) = state
                    .execution
                    .register_resumable(current_thread_token(), carrier)
                else {
                    return crate::registration::REJECTED_V1;
                };
                let Some(next_token) = state.next_registration_token.checked_add(1) else {
                    let _ = state
                        .execution
                        .unregister_unpublished_resumable(current_thread_token(), capability);
                    return crate::registration::REJECTED_V1;
                };
                let claim = state
                    .staged_resumable_unit
                    .as_mut()
                    .ok_or(())
                    .and_then(|staged| {
                        staged
                            .claim(envelope.manifest_entry_index())
                            .map_err(|_| ())
                    });
                if claim.is_err() {
                    let _ = state
                        .execution
                        .unregister_unpublished_resumable(current_thread_token(), capability);
                    return crate::registration::REJECTED_V1;
                }
                let claim_slot = state
                    .staged_resumable_claims
                    .as_mut()
                    .and_then(|claims| claims.get_mut(envelope.manifest_entry_index() as usize));
                let Some(claim_slot) = claim_slot else {
                    let _ = state
                        .staged_resumable_unit
                        .as_mut()
                        .and_then(|staged| staged.release(envelope.manifest_entry_index()).ok());
                    let _ = state
                        .execution
                        .unregister_unpublished_resumable(current_thread_token(), capability);
                    return crate::registration::REJECTED_V1;
                };
                if claim_slot.is_some() {
                    let _ = state
                        .staged_resumable_unit
                        .as_mut()
                        .and_then(|staged| staged.release(envelope.manifest_entry_index()).ok());
                    let _ = state
                        .execution
                        .unregister_unpublished_resumable(current_thread_token(), capability);
                    return crate::registration::REJECTED_V1;
                }
                *claim_slot = Some(RuntimeRegistrationClaim {
                    thunk: MethodAddress::from_method(thunk),
                    capability,
                });
                let token = state.next_registration_token;
                state.next_registration_token = next_token;
                state.registration_tokens.push(RuntimeRegistrationToken {
                    token,
                    manifest_entry_index: envelope.manifest_entry_index(),
                    thunk: MethodAddress::from_method(thunk),
                    capability,
                    code_generation_identity: envelope.code_generation_identity(),
                });
                envelope.set_registration_token(token);
                self.native_tier.borrow_mut().register(
                    envelope.manifest_entry_index() as usize,
                    &optimizer_descriptor_bytes,
                    &optimizer_descriptor,
                );
                crate::registration::OK_V1
            }
            ABORT_V1 => {
                if envelope.validate_abort_shape().is_err() {
                    return crate::registration::REJECTED_V1;
                }
                // SAFETY: owner-thread point mutation after complete shape
                // validation; token lookup occurs before registry mutation.
                let state = unsafe { &mut *self.state.get() };
                let Some(index) = state.registration_tokens.iter().position(|record| {
                    record.token == envelope.registration_token()
                        && record.manifest_entry_index == envelope.manifest_entry_index()
                }) else {
                    return crate::registration::REJECTED_V1;
                };
                let record = state.registration_tokens[index];
                let claim_matches = state
                    .staged_resumable_claims
                    .as_ref()
                    .and_then(|claims| claims.get(record.manifest_entry_index as usize))
                    .and_then(Option::as_ref)
                    .is_some_and(|claim| claim.capability == record.capability);
                if !claim_matches {
                    return crate::registration::REJECTED_V1;
                }
                if state
                    .execution
                    .unregister_unpublished_resumable(current_thread_token(), record.capability)
                    .is_err()
                {
                    return crate::registration::REJECTED_V1;
                }
                if state
                    .staged_resumable_unit
                    .as_mut()
                    .is_none_or(|staged| staged.release(record.manifest_entry_index).is_err())
                {
                    boundary::fatal_invariant("staged registration claim could not be released");
                }
                *state
                    .staged_resumable_claims
                    .as_mut()
                    .and_then(|claims| claims.get_mut(record.manifest_entry_index as usize))
                    .unwrap_or_else(|| {
                        boundary::fatal_invariant("staged Runtime claim disappeared")
                    }) = None;
                state.registration_tokens.swap_remove(index);
                crate::registration::OK_V1
            }
            _ => crate::registration::REJECTED_V1,
        }
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

    pub(crate) fn try_acquire_stackless_generation_lease(
        &self,
        method: MethodAddress,
    ) -> Result<Option<GenerationLeaseCapability>, crate::execution::RegistrationError> {
        self.assert_owner();
        // SAFETY: optional explicit lease acquisition is one owner-thread point
        // mutation. A method outside the resumable registry needs no lease.
        unsafe {
            (&mut *self.state.get())
                .execution
                .try_acquire_explicit_generation_lease(method)
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

    pub(crate) fn resolve_stackless_target(
        &self,
        receiver: Oop,
        selector: Oop,
    ) -> Result<StacklessTarget, ExecutionError> {
        // SAFETY: callers establish Runtime::enter and pass live values copied
        // from traced Frame storage.
        let method = match unsafe { self.bind_active(receiver, selector) } {
            Ok(Some(method)) => method,
            Ok(None) => return Ok(StacklessTarget::Unbound),
            Err(outcome) => return Ok(StacklessTarget::NativeFailure(outcome)),
        };
        // Classification uses the exact carrier returned by metacircular
        // lookup; there is no second vtable or parent-chain walk.
        let resumable = match crate::legacy_method::try_decode(method) {
            Some(legacy) => unsafe { (&mut *self.state.get()).execution.classify_resumable(legacy) }?,
            None => None,
        };
        Ok(resumable.unwrap_or(StacklessTarget::Synchronous(method)))
    }

    fn materialize_registered_target(
        &self,
        entry: crate::execution::ResumableEntryCapability,
        receiver: Oop,
        arguments: &[Oop],
    ) -> Result<MaterializedEntry, ExecutionError> {
        let result = (|| {
            // SAFETY: Runtime owner-thread serialization permits one bounded
            // immutable inspection of both the registry and capture carrier.
            let state = unsafe { &*self.state.get() };
            let (capture_offset, capture_count) = state
                .execution
                .acquired_entry_capture_layout(entry)
                .map_err(ExecutionError::from)?;
            let payload_word_count = capture_offset
                .checked_add(capture_count)
                .ok_or(ExecutionError::InvalidAction)?;
            let captures = if payload_word_count == 0 {
                Vec::new()
            } else {
                let expected_size = payload_word_count
                    .checked_mul(core::mem::size_of::<usize>())
                    .ok_or(ExecutionError::InvalidAction)?;
                if state.heap.payload_size(receiver) != Some(expected_size) {
                    return Err(ExecutionError::InvalidAction);
                }
                let mut captures = Vec::with_capacity(capture_count);
                for index in 0..capture_count {
                    let word = state
                        .heap
                        .read_payload_word(receiver, capture_offset + index)
                        .ok_or(ExecutionError::InvalidAction)?;
                    captures.push(core::ptr::with_exposed_provenance_mut(word));
                }
                captures
            };
            state
                .execution
                .materialize_acquired_entry(
                    entry,
                    receiver,
                    arguments,
                    &captures,
                    self.canonical().nil,
                )
                .map_err(ExecutionError::from)
        })();
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
        self.enter_acquired_resumable(entry, receiver, arguments)
    }

    pub(crate) fn enter_acquired_resumable(
        &self,
        entry: crate::execution::ResumableEntryCapability,
        receiver: Oop,
        arguments: &[Oop],
    ) -> Result<Oop, ExecutionError> {
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
            let machine = match self.create_stackless_machine() {
                Ok(machine) => machine,
                Err(error) => {
                    // Materialization has not transferred the acquired entry
                    // to a Frame yet, so a failed Machine allocation must
                    // release it here.
                    unsafe {
                        (&mut *self.state.get())
                            .execution
                            .release_acquired_entry(entry)
                    }?;
                    return Err(error.into());
                }
            };
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
        method: NativeEntry,
        receiver: Oop,
        arguments: &[Oop],
        numeric: &mut Option<numeric::Ticket>,
        location: (usize, usize),
    ) -> Result<Oop, ExecutionError> {
        let _boundary = self.enter_stackless_boundary(owner, capability)?;
        let _scope = self.enter_stackless_scope(capability);
        Ok(self.enter(|| {
            let (key, pc) = location;
            if pc == 0 {
                *numeric = self.prepare_numeric_chain(capability, owner, key);
            }
            let prediction = numeric.as_mut().and_then(|ticket| ticket.take(key, pc));
            if let Some(legacy) = crate::legacy_method::try_decode(method)
                && let Some(result) = self.try_numeric_call_with_prediction(legacy.to_method(), receiver, arguments, prediction)
            {
                return result;
            }
            self.invoke_native_from_objects(method, receiver, arguments)
        }))
    }

    fn invoke_stackless_native_failure(&self, owner: u64, capability: MachineCapability,
        outcome: crate::abi::id_protocol::Outcome) -> Result<Oop, ExecutionError> {
        let _boundary = self.enter_stackless_boundary(owner, capability)?;
        let _scope = self.enter_stackless_scope(capability);
        Ok(self.enter(|| self.native_language_outcome(outcome)))
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
            // synchronous adapter. Lookup already missed in the driver; resolve
            // only DNU now, rather than looking up the original selector twice.
            unsafe { self.send_unbound_abi(receiver, selector, args, arguments.len()) }
        }))
    }

    fn finish_stackless_return(
        &self,
        owner: u64,
        capability: MachineCapability,
        result: Oop,
    ) -> Result<Option<Oop>, ExecutionError> {
        let nil = self.canonical().nil;
        // SAFETY: return application is one allocation-free owner-thread state
        // transition and invokes no generated code while borrowed.
        let answer = unsafe {
            let state = &mut *self.state.get();
            let (answer, entry) = {
                let machine = state.execution.machine_mut(owner, capability)?;
                let frame = machine
                    .arena()
                    .current_handle()
                    .ok_or(crate::execution::FrameError::StaleHandle)?;
                let destination = machine.arena().return_destination(frame)?;
                let entry = machine.arena().resumable_entry(frame)?;
                machine.arena_mut().inherit_return_delivery(frame)?;
                machine.arena_mut().pop()?;
                let answer = if let Some(destination) = destination {
                    let caller = machine
                        .arena()
                        .current_handle()
                        .ok_or(crate::execution::FrameError::StaleHandle)?;
                    machine.arena_mut().commit_call_transition(
                        caller,
                        destination.result_slot,
                        nil,
                        result,
                    )?;
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
            answer
        };
        #[cfg(feature = "ffi-test-hooks")]
        closure_action_probe::after_return(self, owner, capability);
        Ok(answer)
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

    pub(crate) fn resume_stackless_machine(
        &self,
        machine: Oop,
        supplied_value: Oop,
    ) -> Result<Oop, ExecutionError> {
        self.assert_owner();
        let capability = self.stackless_machine_capability(machine)?;
        let owner = current_thread_token();
        let nil = self.canonical().nil;
        unsafe {
            (&mut *self.state.get())
                .execution
                .resume_suspended_machine(owner, capability, supplied_value, nil)?;
        }
        self.run_resumed_stackless(machine, capability, owner)
    }

    pub(crate) fn restore_stackless_continuation(
        &self,
        machine: Oop,
        continuation: Oop,
        supplied_value: Oop,
    ) -> Result<Oop, ExecutionError> {
        self.assert_owner();
        let capability = self.stackless_machine_capability(machine)?;
        let owner = current_thread_token();
        let nil = self.canonical().nil;
        unsafe {
            let state = &mut *self.state.get();
            let continuation_capability = state
                .execution
                .continuation_capability_for_object(&state.heap, continuation)?;
            state.execution.restore_continuation(
                owner,
                capability,
                continuation_capability,
                supplied_value,
                nil,
            )?;
        }
        self.run_resumed_stackless(machine, capability, owner)
    }

    fn run_resumed_stackless(
        &self,
        machine: Oop,
        capability: MachineCapability,
        owner: u64,
    ) -> Result<Oop, ExecutionError> {
        let nil = self.canonical().nil;
        self.push_stackless_scoped_root(machine);
        let _root_guard = StacklessDriveRootGuard {
            runtime: self,
            machine,
        };
        let mut resolve = |runtime: &Runtime, receiver: Oop, selector: Oop, _arguments: &[Oop]| {
            runtime.resolve_stackless_target(receiver, selector)
        };
        let result = self.run_stackless_loop(machine, capability, owner, nil, &mut resolve);
        self.fail_stackless_drive_on_error(owner, capability, result)
    }

    fn drive_stackless_with_resolver(
        &self,
        machine: Oop,
        descriptor: std::sync::Arc<FrameDescriptor>,
        initial_oops: &[(usize, Oop)],
        initial_entry: Option<crate::execution::ResumableEntryCapability>,
        resolve: impl FnMut(&Runtime, Oop, Oop, &[Oop]) -> Result<StacklessTarget, ExecutionError>,
    ) -> Result<Oop, ExecutionError> {
        self.drive_stackless_with_resolver_budget(
            machine, descriptor, initial_oops, initial_entry, None, resolve,
        )
    }

    fn drive_stackless_with_resolver_budget(
        &self,
        machine: Oop,
        descriptor: std::sync::Arc<FrameDescriptor>,
        initial_oops: &[(usize, Oop)],
        initial_entry: Option<crate::execution::ResumableEntryCapability>,
        budget: Option<usize>,
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

        let result = self.run_stackless_loop_budget(machine, capability, owner, nil, budget, &mut resolve);
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
        self.run_stackless_loop_budget(machine, capability, owner, nil, None, resolve)
    }

    fn run_stackless_loop_budget(
        &self,
        machine: Oop,
        capability: MachineCapability,
        owner: u64,
        nil: Oop,
        mut budget: Option<usize>,
        resolve: &mut impl FnMut(&Runtime, Oop, Oop, &[Oop]) -> Result<StacklessTarget, ExecutionError>,
    ) -> Result<Oop, ExecutionError> {
        // Covers initial, nested, resumed and bounded drives, including direct
        // native driver callers. An enclosing application turn remains active
        // after this guard ends; a driver return never drains physical owners.
        let _turn = self.enter_managed_turn();
        let mut numeric = None;
        loop {
            // Only between fully committed transitions, after native activations,
            // optimizer scratch, and entry scopes from the previous turn ended.
            if let Some(remaining) = budget.as_mut() {
                if *remaining == 0 {
                    let state = unsafe { &mut *self.state.get() };
                    let execution = state.execution.machine_mut(owner, capability)?;
                    if execution.control_eligible() {
                        execution.stop_runnable(owner)?;
                        return Ok(machine);
                    }
                } else {
                    *remaining -= 1;
                }
            }
            // Copy all invocation inputs before crossing into generated code.
            let (method, program_counter, argument_count, descriptor_key) = unsafe {
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
                    descriptor as *const FrameDescriptor as usize,
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
            // Design 74F: selection is test-only and happens after the resume
            // token allocation, which may collect and invalidate dependencies.
            // Both entries use the unchanged descriptor and committed Frame.
            #[cfg(test)]
            let method = deopt_experiment::select(self, capability, owner, machine, method)?;
            #[cfg(all(test, target_os = "macos"))]
            let selected =
                generated_deopt_experiment::select(self, capability, owner, machine, method)?;
            #[cfg(all(test, target_os = "macos"))]
            let method = selected.method;
            let native_selected =
                native_tier::select(self, machine, descriptor_key, program_counter);
            let (action, optimized_return) = {
                let _scope = self.enter_stackless_scope(capability);
                self.enter(|| {
                    let (arguments, count) = match &resume_token {
                        Some(token) => (core::ptr::from_ref(token), 1),
                        None => (core::ptr::null(), 0),
                    };
                    // SAFETY: descriptor validation freezes a Method-shaped entry;
                    // the optional one-Oop argument buffer lives through the call.
                    unsafe {
                        if let Some(selected) = &native_selected {
                            if let Some(result) = selected.try_unboxed_numeric(
                                self,
                                machine,
                                descriptor_key,
                                program_counter,
                                arguments,
                                count,
                            ) {
                                match result {
                                    Ok(native_tier::unboxed_numeric::Outcome::Action(action)) => {
                                        (action, None)
                                    }
                                    Ok(native_tier::unboxed_numeric::Outcome::Return(value)) => {
                                        (core::ptr::null_mut(), Some(Ok(value)))
                                    }
                                    Err(error) => (core::ptr::null_mut(), Some(Err(error))),
                                }
                            } else if let Some(result) = selected
                                .try_numeric(
                                    self,
                                    machine,
                                    descriptor_key,
                                    program_counter,
                                    arguments,
                                    count,
                                )
                                .or_else(|| {
                                    selected.try_discarded_clone(
                                        self,
                                        machine,
                                        descriptor_key,
                                        program_counter,
                                        arguments,
                                        count,
                                    )
                                })
                            {
                                (core::ptr::null_mut(), Some(result))
                            } else if let Some(result) = selected.try_virtual_aggregate(
                                self,
                                machine,
                                descriptor_key,
                                program_counter,
                                arguments,
                                count,
                            ) {
                                match result {
                                    Ok(action) => (action, None),
                                    Err(error) => (core::ptr::null_mut(), Some(Err(error))),
                                }
                            } else {
                                (selected.invoke(self, machine, arguments, count), None)
                            }
                        } else {
                            (method(machine, arguments, count), None)
                        }
                    }
                })
            };

            // The selected executable must stay mapped until native return.
            #[cfg(all(test, target_os = "macos"))]
            drop(selected);
            drop(native_selected);
            if self.finish_closure_inline(machine, capability, action)? {
                numeric = None;
                continue;
            }
            if let Some(result) = optimized_return {
                numeric = None;
                if let Some(answer) = self.finish_stackless_return(owner, capability, result?)? {
                    return Ok(answer);
                }
                continue;
            }

            // Test-floor observation occurs only after native execution and
            // optimizer scratch ownership have unwound. Production has no hook.
            #[cfg(feature = "ffi-test-hooks")]
            let action = unboxed_numeric_probe::after_action(self, machine, action);

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
                DecodedAction::Continue => unsafe {
                    (&mut *self.state.get())
                        .execution
                        .commit_current_continue(owner, capability, nil)?;
                },
                DecodedAction::BoundaryCall {
                    receiver_slot,
                    selector_slot,
                    argument_start,
                    argument_count,
                    result_slot,
                }
                | DecodedAction::Call {
                    receiver_slot,
                    selector_slot,
                    argument_start,
                    argument_count,
                    result_slot,
                } => {
                    let protected = matches!(decoded, DecodedAction::BoundaryCall { .. });
                    if protected {
                        // The StepMethod has returned: no native wrapper spans
                        // the invocation. Establish before lookup/body execution.
                        unsafe {
                            (&mut *self.state.get()).execution.establish_boundary(
                                owner,
                                capability,
                                result_slot,
                            )?;
                        }
                    }
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
                            if !protected
                                && native_tier::inline_call(
                                    self,
                                    machine,
                                    capability,
                                    entry,
                                    result_slot,
                                    &descriptor,
                                    &initial_oops,
                                )?
                            {
                                continue;
                            }
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
                                &mut numeric,
                                (descriptor_key, program_counter),
                            )?;
                            unsafe {
                                (&mut *self.state.get())
                                    .execution
                                    .commit_current_call_result(
                                        owner,
                                        capability,
                                        result_slot,
                                        nil,
                                        result,
                                    )?;
                            }
                        }
                        StacklessTarget::NativeFailure(outcome) => {
                            let result = self.invoke_stackless_native_failure(owner, capability, outcome)?;
                            unsafe { (&mut *self.state.get()).execution.commit_current_call_result(
                                owner, capability, result_slot, nil, result)?; }
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
                                (&mut *self.state.get())
                                    .execution
                                    .commit_current_call_result(
                                        owner,
                                        capability,
                                        result_slot,
                                        nil,
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
                            let restarted = unsafe {
                                let state = &mut *self.state.get();
                                state.execution.try_restart_registered_tail(
                                    &state.heap,
                                    owner,
                                    capability,
                                    entry,
                                    operands.receiver,
                                    &operands.arguments,
                                    nil,
                                )
                            }?;
                            if restarted {
                                continue;
                            }
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
                                &mut numeric,
                                (descriptor_key, program_counter),
                            )?;
                            if let Some(answer) =
                                self.finish_stackless_return(owner, capability, result)?
                            {
                                return Ok(answer);
                            }
                        }
                        StacklessTarget::NativeFailure(outcome) => {
                            let result = self.invoke_stackless_native_failure(owner, capability, outcome)?;
                            if let Some(answer) = self.finish_stackless_return(owner, capability, result)? {
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
                DecodedAction::Suspend {
                    result_slot,
                    delimiter,
                } => {
                    unsafe {
                        (&mut *self.state.get()).execution.apply_suspend(
                            owner,
                            capability,
                            result_slot,
                            delimiter,
                        )?;
                    }
                    return Ok(machine);
                }
                DecodedAction::Capture {
                    delimiter,
                    result_slot,
                } => {
                    let objects = unsafe { (*self.state.get()).execution.objects() }
                        .ok_or(ExecutionError::InvalidAction)?;
                    let continuation =
                        self.allocate_instance(objects.continuation_vtable, 3 * size_of::<usize>());
                    unsafe {
                        let state = &mut *self.state.get();
                        state.execution.capture_continuation(
                            &mut state.heap,
                            owner,
                            capability,
                            delimiter,
                            result_slot,
                            continuation,
                        )?;
                    }
                    self.set_finalizer(continuation, crate::execution::continuation_finalizer);
                    return Ok(continuation);
                }
                DecodedAction::Restore {
                    continuation_slot,
                    supplied_value_slot,
                } => {
                    let (continuation, supplied_value) = unsafe {
                        let state = &*self.state.get();
                        let machine_state = state.execution.machine(owner, capability)?;
                        let frame = machine_state
                            .arena()
                            .current_handle()
                            .ok_or(crate::execution::FrameError::StaleHandle)?;
                        (
                            machine_state.arena().read_oop(frame, continuation_slot)?,
                            machine_state.arena().read_oop(frame, supplied_value_slot)?,
                        )
                    };
                    let continuation_capability = unsafe {
                        let state = &*self.state.get();
                        state
                            .execution
                            .continuation_capability_for_object(&state.heap, continuation)?
                    };
                    unsafe {
                        (&mut *self.state.get()).execution.restore_continuation(
                            owner,
                            capability,
                            continuation_capability,
                            supplied_value,
                            nil,
                        )?;
                    }
                }
                DecodedAction::ExitBoundary { value_slot } => unsafe {
                    let state = &mut *self.state.get();
                    let value = state
                        .execution
                        .read_current_oop(owner, capability, value_slot)?;
                    state
                        .execution
                        .exit_boundary(owner, capability, value, nil)?;
                },
                DecodedAction::Abort {
                    delimiter: _,
                    reason_slot,
                } => {
                    let reason = unsafe {
                        let state = &*self.state.get();
                        let machine_state = state.execution.machine(owner, capability)?;
                        let frame = machine_state
                            .arena()
                            .current_handle()
                            .ok_or(crate::execution::FrameError::StaleHandle)?;
                        machine_state.arena().read_oop(frame, reason_slot)?
                    };
                    unsafe {
                        (&mut *self.state.get())
                            .execution
                            .machine_mut(owner, capability)?
                            .fail(owner)?;
                    }
                    return Ok(reason);
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

    pub(crate) fn export_name_at(&self, index: usize) -> Oop {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied identity.
        unsafe { (*self.state.get()).environment.name_at(index) }
    }

    pub(crate) fn vtable_for_valid_receiver(&self, receiver: Oop) -> Oop {
        self.assert_owner();
        // SAFETY: dispatch callers guarantee a live floor receiver. No runtime
        // collection borrow is needed for the header fast path.
        unsafe { Behaviors::dispatch_vtable_of(receiver) }
    }

    /// Read a dispatch header after `enter` established owner-thread authority.
    ///
    /// # Safety
    ///
    /// The runtime must be active and `receiver` must be an exact live object.
    pub(crate) unsafe fn vtable_for_valid_receiver_active(&self, receiver: Oop) -> Oop {
        // SAFETY: the caller supplies the live receiver precondition.
        unsafe { Behaviors::dispatch_vtable_of(receiver) }
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
            let (dictionaries, globals) = unsafe {
                let state = self.state.get();
                (Behaviors::dictionaries_ptr(core::ptr::addr_of_mut!((*state).vtables)), (*state).globals)
            };
            // Id receives only its own dictionary registry and real selectors.
            let result = unsafe { crate::id_dispatch::enter(dictionaries,
                globals.vtable_vt, globals.lookup, globals.does_not_understand, operation) };
            drop(guard);
            result
        })
    }

    /// Pinned application entry. Nested sends and FFI reentry share the outer
    /// turn. Its result stays rooted while checkpoint callbacks collect/reenter.
    pub(crate) fn enter_application(&self, operation: impl FnOnce() -> Oop) -> Oop {
        self.enter(|| {
            if unsafe { (&*self.state.get()).execution.has_managed_turn() } {
                return operation();
            }
            let turn = self.enter_managed_turn();
            let result = operation();
            let _root = ScopedOopRootGuard::new(self, result);
            drop(turn);
            self.retirement_checkpoint();
            result
        })
    }

    pub(crate) fn queue_native_retirement(
        &self,
        context: *mut core::ffi::c_void,
        release: birnam_abi::private::NativeRetirementFn,
    ) {
        self.assert_owner();
        self.native_retirements.borrow_mut().push(NativeRetirement { context, release });
    }

    fn retirement_checkpoint(&self) {
        if self.collection_phase.get() != CollectionPhase::Idle
            || unsafe { (&*self.state.get()).execution.has_managed_turn() }
            || self.draining_retirements.replace(true)
        {
            return;
        }
        let _drain = RetirementDrainGuard(&self.draining_retirements);
        self.drain_pending_stackless_retirements();
        // One batch, with no borrow across callbacks. Newly queued work waits
        // for the next checkpoint rather than creating an unbounded drain.
        let pending = core::mem::take(&mut *self.native_retirements.borrow_mut());
        for entry in pending {
            if unsafe { (entry.release)(entry.context) } != 1 {
                self.native_retirements.borrow_mut().push(entry);
            }
        }
    }

    pub(crate) fn symbol_count(&self) -> usize {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied count.
        unsafe { (*self.state.get()).symbols.count() }
    }

    pub(crate) fn symbol_owned_name_bytes(&self) -> usize {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied byte count.
        unsafe { (*self.state.get()).symbols.owned_name_bytes() }
    }

    #[cfg(test)]
    pub(crate) fn enable_symbol_compaction_for_test(&self) {
        self.assert_owner();
        // SAFETY: tests perform this owner-thread transition before collecting.
        unsafe { (*self.state.get()).symbol_compaction_enabled = true };
    }

    pub(crate) fn configure_symbol_compaction(
        &self,
        enabled: bool,
        compatibility_count_limit: usize,
        compatibility_name_byte_limit: usize,
    ) -> bool {
        self.assert_owner();
        // SAFETY: this private loader seam runs on the owner thread and invokes
        // no foreign code while the state transition is applied.
        let state = unsafe { &mut *self.state.get() };
        if enabled {
            state.symbol_compaction_enabled = true;
            state.symbol_compatibility_limits = None;
            return true;
        }
        if compatibility_count_limit == 0
            || compatibility_name_byte_limit == 0
            || state.symbols.count() > compatibility_count_limit
            || state.symbols.owned_name_bytes() > compatibility_name_byte_limit
        {
            return false;
        }
        state.symbol_compaction_enabled = false;
        state.symbol_compatibility_limits = Some(SymbolCompatibilityLimits {
            count: compatibility_count_limit,
            owned_name_bytes: compatibility_name_byte_limit,
        });
        true
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

    pub(crate) fn vtable_enumeration(&self) -> *mut birnam_abi::vtable_enumeration::ApiV2 {
        self.assert_owner();
        unsafe { (*self.state.get()).vtable_enumeration.api() }
    }

    pub(crate) fn canonical_vtable_enumeration(&self) -> birnam_abi::vtable_enumeration::ApiV2 {
        self.assert_owner();
        unsafe { (*self.state.get()).vtable_enumeration.canonical() }
    }

    pub(crate) fn vtable_selector_view(&self, vtable: Oop) -> birnam_abi::vtable_enumeration::ViewV1 {
        self.assert_owner();
        // Borrow only until dictionary mutation; the consumer copies immediately.
        unsafe { (*self.state.get()).vtables.selector_view(vtable) }
    }

    pub(crate) fn vtable_view(&self) -> birnam_abi::vtable_enumeration::ViewV1 {
        self.assert_owner();
        // Immediate borrowed representation only; no callbacks retain this view.
        unsafe { (*self.state.get()).vtables.view() }
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

    /// Pure facts for the existing warm Block optimization. Ordinary consumers
    /// send the owner/parent protocols; admission must never execute replacements.
    /// None withdraws the optimization, so the original constructor performs
    /// its ordinary sends. This supplies no new public representation interface.
    pub(crate) fn default_block_behavior_owner(&self, block_vtable: Oop) -> Option<Oop> {
        self.assert_owner();
        // SAFETY: owner-thread inspection; no callback, allocation or mutation.
        let state = unsafe { &*self.state.get() };
        let meta = state.globals.vtable_vt;
        if Behaviors::checked_vtable_of(&state.heap, meta) != meta {
            return None;
        }
        for (selector, method) in [
            (
                state.globals.lookup,
                crate::dispatch::vtable_lookup_method as Method,
            ),
            (
                state.symbols.existing(b"ownerOf:")?,
                crate::dispatch::vtable_owner_of as Method,
            ),
            (
                state.symbols.existing(b"parentOf:")?,
                crate::dispatch::vtable_parent_of as Method,
            ),
        ] {
            let method = MethodAddress::from_method(method);
            if state.vtables.direct_lookup(meta, selector) != Some(method)
                || state.execution.has_resumable_thunk(method)
            {
                return None;
            }
        }
        let mut ancestor = block_vtable;
        for _ in 0..4096 {
            if ancestor.is_null() {
                break;
            }
            let name = state.environment.exported_name_of(ancestor);
            if state.symbols.bytes_of(name) == Some(b"BlockVt".as_slice()) {
                return Some(state.vtables.owner_of(block_vtable));
            }
            ancestor = state.vtables.parent_of(ancestor);
        }
        None
    }

    pub(crate) fn vtable_selector_at(&self, vtable: Oop, index: usize) -> Oop {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied identity.
        unsafe { (*self.state.get()).vtables.selector_at(vtable, index) }
    }

    pub(crate) fn checked_vtable_of(&self, object: Oop) -> Oop {
        self.assert_owner();
        // SAFETY: owner-thread serialization; checked lookup copies the header.
        unsafe { Behaviors::checked_vtable_of(&(*self.state.get()).heap, object) }
    }

    /// Perform the same exact-membership vtable lookup after `enter` has already
    /// established this runtime as active on the owner thread.
    pub(crate) unsafe fn checked_vtable_of_active(&self, object: Oop) -> Oop {
        // SAFETY: the active-runtime guard proves owner-thread serialization;
        // checked_vtable_of still validates exact heap membership before reading.
        unsafe { Behaviors::checked_vtable_of(&(*self.state.get()).heap, object) }
    }

    pub(crate) fn payload_size(&self, object: Oop) -> Option<usize> {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied size.
        unsafe { (*self.state.get()).heap.payload_size(object) }
    }

    pub(crate) fn native_stack(&self) -> *mut birnam_abi::native_stack::ApiV1 {
        self.assert_owner();
        unsafe { (*self.state.get()).metadata.native_stack() }
    }

    pub(crate) fn admit_native_stack(&self) { self.assert_owner(); }

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
        // Canonical append invokes no callbacks and does not borrow RuntimeState.
        unsafe { (*self.state.get()).root_registry.register(provider) };
    }

    pub(crate) fn root_registry(&self) -> *mut birnam_abi::root_registry::ApiV1 {
        self.assert_owner();
        unsafe { (*self.state.get()).root_registry.api() }
    }

    pub(crate) fn admit_root_registration(&self) { self.assert_owner(); }

    pub(crate) fn gc_settings(&self) -> *mut birnam_abi::gc_settings::ApiV1 {
        self.assert_owner();
        unsafe { (*self.state.get()).metadata.settings() }
    }

    pub(crate) fn admit_gc_settings(&self) { self.assert_owner(); }

    pub(crate) fn gc_collect(&self) -> usize {
        self.enter(|| {
            let collected = self.gc_collect_active();
            self.retirement_checkpoint();
            collected
        })
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

    pub(crate) fn gc_live_count(&self) -> usize {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied count.
        unsafe { (*self.state.get()).heap.allocation_count() }
    }

    pub(crate) fn gc_live_bytes(&self) -> usize {
        self.assert_owner();
        // SAFETY: owner-thread serialization and a copied aggregate.
        unsafe { (*self.state.get()).heap.allocated_bytes() }
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

    pub(crate) fn trace_hooks(&self) -> *mut birnam_abi::trace_hooks::ApiV1 {
        self.assert_owner();
        unsafe { (*self.state.get()).vtables.trace_hooks_api() }
    }

    pub(crate) fn admit_trace_hooks(&self) { self.assert_owner(); }

    pub(crate) fn replace_trace_hook(&self, vtable: Oop, slot: birnam_abi::trace_hooks::SlotV1) {
        unsafe { (&mut *self.state.get()).vtables.replace_trace_hook(vtable, slot) };
    }

    pub(crate) fn finalizers(&self) -> *mut birnam_abi::finalizers::ApiV1 {
        self.assert_owner();
        unsafe { (*self.state.get()).heap.finalizers_api() }
    }

    pub(crate) fn admit_finalizers(&self) { self.assert_owner(); }

    pub(crate) fn finalizer_permanence(&self, object: Oop) -> Option<crate::heap::Permanence> {
        unsafe { (*self.state.get()).heap.permanence_of(object) }
    }

    pub(crate) fn replace_finalizer(&self, object: Oop, slot: birnam_abi::finalizers::SlotV1) {
        unsafe { (&mut *self.state.get()).heap.replace_finalizer(object, slot) };
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

    pub(crate) fn payload_layout(&self) -> *mut birnam_abi::payload_layout::ApiV1 {
        self.assert_owner();
        unsafe { (*self.state.get()).vtables.layout_api() }
    }

    pub(crate) fn admit_payload_layout(&self) { self.assert_owner(); }
    pub(crate) fn lifetime_facts(&self, object: Oop) -> u32 {
        self.assert_owner();
        let state = unsafe { &*self.state.get() };
        let Some(permanence) = state.heap.permanence_of(object) else { return 0; };
        birnam_abi::lifetime::ALLOCATED
            | if state.vtables.is_registered(object) { birnam_abi::lifetime::BEHAVIOR } else { 0 }
            | if permanence == crate::heap::Permanence::Permanent { birnam_abi::lifetime::PERMANENT } else { 0 }
    }


    // The public capability has already admitted access; these floor operations
    // neither call out nor repeat admission. No view survives replacement.
    pub(crate) fn replace_payload_layout(&self, vtable: Oop, owned: birnam_abi::payload_layout::OwnedV1) {
        unsafe { (&mut *self.state.get()).vtables.replace_layout(vtable, owned) };
    }

    pub(crate) fn payload_layout_view(&self, vtable: Oop) -> birnam_abi::payload_layout::ViewV1 {
        unsafe { (*self.state.get()).vtables.layout_view(vtable) }
    }

    #[cfg(test)]
    pub(crate) fn permanence_of(&self, object: Oop) -> Option<Permanence> {
        // SAFETY: test calls remain owner-thread and copy only the class.
        unsafe { (*self.state.get()).heap.permanence_of(object) }
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
        self.run_mark_phase((0, 0), &[]);
    }

    #[cfg(test)]
    pub(crate) fn is_marked_for_test(&self, object: Oop) -> Option<bool> {
        self.assert_owner();
        // SAFETY: test calls remain owner-thread and copy one metadata bit.
        unsafe { (*self.state.get()).heap.is_marked(object) }
    }
}

fn upper_roots(state: &RuntimeState) -> Vec<Oop> {
    let mut roots = state.root_registry.retained_roots();
    let export_roots = state.environment.roots();
    let globals = state.globals.roots();
    let canonical = state
        .canonical
        .into_iter()
        .flat_map(CanonicalObjects::roots);
    let execution_objects = state
        .execution
        .objects()
        .into_iter()
        .flat_map(ExecutionObjects::roots);
    let compatibility_symbols = (!state.symbol_compaction_enabled)
        .then(|| {
            (0..state.symbols.count())
                .map(|index| state.symbols.at(index))
                .collect::<Vec<_>>()
        })
        .into_iter()
        .flatten();
    for root in export_roots
        .into_iter()
        .chain(globals)
        .chain(canonical)
        .chain(execution_objects)
        .chain(compatibility_symbols)
    {
        roots.push(root);
    }
    for root in state
        .execution
        .registration_roots_excluding_generation(state.suppressed_registration_generation)
    {
        roots.push(root);
    }
    for entry in &state.block_vtable_catalog {
        roots.push(entry.vtable);
    }
    roots
}

// Diagnostic snapshots still use their separately admitted storage view. The
// live collector obtains the Id portion through its native message protocol.
fn floor_roots(state: &RuntimeState) -> Vec<Oop> {
    let mut roots = upper_roots(state);
    roots.extend(state.id_protocol.as_ref().into_iter().flat_map(|id| id.roots()));
    roots.extend(state.heap.permanent_vtables());
    roots.extend(state.vtables.permanent_strong_edges(&state.heap));
    roots
}

fn block_registration_for_method(
    state: &RuntimeState,
    method: MethodAddress,
) -> Option<(
    [u8; 32],
    u32,
    crate::execution::ResumableEntryCapability,
    u64,
)> {
    if let (Some(staged), Some(claims)) = (
        state.staged_resumable_unit.as_ref(),
        state.staged_resumable_claims.as_ref(),
    ) {
        for (index, claim) in claims.iter().enumerate() {
            let Some(claim) = claim else { continue };
            if claim.thunk == method
                && staged.implementation_kind(index) == Some(ImplementationKind::BlockBody)
            {
                return Some((
                    staged.authority(),
                    index as u32,
                    claim.capability,
                    staged.code_generation_identity(),
                ));
            }
        }
    }
    let mut found = None;
    for unit in &state.committed_resumable_units {
        for entry in &unit.entries {
            if entry.thunk != method || entry.implementation_kind != ImplementationKind::BlockBody {
                continue;
            }
            let candidate = (
                unit.authority,
                entry.manifest_entry_index,
                entry.capability,
                unit.code_generation_identity,
            );
            if found.replace(candidate).is_some() {
                return None;
            }
        }
    }
    found
}

fn note_block_vtable_bound(state: &mut RuntimeState, vtable: Oop, method: MethodAddress) {
    if vtable.is_null() {
        return;
    }
    let Some((authority, manifest_entry_index, capability, code_generation_identity)) =
        block_registration_for_method(state, method)
    else {
        return;
    };
    let mut drifted = false;
    for entry in &mut state.block_vtable_catalog {
        if entry.capability == capability || entry.vtable == vtable {
            if entry.capability == capability && entry.vtable == vtable && !entry.drifted {
                return;
            }
            entry.drifted = true;
            drifted = true;
        }
    }
    state.block_vtable_catalog.push(BlockVtableCatalogEntry {
        vtable,
        authority,
        manifest_entry_index,
        capability,
        code_generation_identity,
        drifted,
    });
}

fn note_block_vtable_replaced(state: &mut RuntimeState, vtable: Oop, method: MethodAddress) {
    let capability = block_registration_for_method(state, method).map(|entry| entry.2);
    for entry in &mut state.block_vtable_catalog {
        if entry.vtable == vtable
            && capability.is_none_or(|capability| entry.capability == capability)
        {
            entry.drifted = true;
        }
    }
}

/// Namespace borrows this primitive only during owner-checked namespace calls.
/// It must not collect, send, reenter, or keep the transient RuntimeState pointer.
pub(crate) unsafe extern "C" fn namespace_intern(context: *mut core::ffi::c_void, bytes: *const u8, length: usize) -> Oop {
    let bytes = unsafe { birnam_abi::borrowed_slice(bytes, length) }
        .expect("Namespace interner received invalid bytes");
    unsafe { intern_in_state(&mut *context.cast::<RuntimeState>(), bytes) }
        .unwrap_or(core::ptr::null_mut())
}

/// Update the existing representation after raw binding, without collection,
/// sends or reentry. The state pointer is borrowed only for this call.
pub(crate) unsafe extern "C" fn namespace_mirror(context: *mut core::ffi::c_void, symbol: Oop, value: Oop) {
    let state = unsafe { &mut *context.cast::<RuntimeState>() };
    if let Some((_, environment)) = state.internal_dictionaries {
        state.vtables.add_weak_opaque_binding(environment, symbol, value);
    }
}

fn intern_in_state(state: &mut RuntimeState, bytes: &[u8]) -> Result<Oop, EmbeddedNul> {
    if !state.symbol_compaction_enabled
        && !state.symbols.contains_bytes(bytes)
        && let Some(limits) = state.symbol_compatibility_limits
    {
        let next_count = state.symbols.count().checked_add(1).unwrap_or_else(|| {
            boundary::fatal_resource_limit("compatibility Symbol count overflow")
        });
        let next_name_bytes = state
            .symbols
            .owned_name_bytes()
            .checked_add(bytes.len().saturating_add(1))
            .unwrap_or_else(|| {
                boundary::fatal_resource_limit("compatibility Symbol name bytes overflow")
            });
        if next_count > limits.count {
            boundary::fatal_resource_limit("compatibility Symbol count ceiling");
        }
        if next_name_bytes > limits.owned_name_bytes {
            boundary::fatal_resource_limit("compatibility Symbol name-byte ceiling");
        }
    }
    let before = state.symbols.count();
    let symbol = state.heap.intern_symbol(&mut state.symbols, state.globals.symbol_vt, bytes)?;
    if state.symbols.count() != before
        && let Some((symbols, _)) = state.internal_dictionaries
    {
        state
            .vtables
            .add_weak_opaque_binding(symbols, symbol, core::ptr::null_mut());
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

#[cfg(test)]
thread_local! {
    /// Design 74D test hook: force one collection inside the store fallback
    /// after the value is rooted and before the index box is allocated.
    pub(crate) static FORCE_COLLECT_IN_SLOT_FALLBACK: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
}

pub(crate) fn with_active<R>(operation: impl FnOnce(&Runtime) -> R) -> R {
    ACTIVE_RUNTIME.with(|active| {
        let runtime = active.get();
        boundary::require_invariant(!runtime.is_null(), "no active Rust floor runtime");
        // SAFETY: enter installs a pointer to a live owner-thread Runtime for
        // this dynamic extent, including synchronous reentrant callbacks.
        let runtime = unsafe { &*runtime };
        runtime.require_storage_access();
        operation(runtime)
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
        let runtime = unsafe { &*runtime };
        runtime.require_storage_access();
        Some(operation(runtime))
    })
}

#[cfg(test)]
mod tests {
    use super::{CodeGenerationRetirement, current_thread_token, with_active_if_present};
    use crate::abi::{Method, Oop, SLOT_OOP};
    use crate::dispatch::build_bootstrap;
    use crate::execution::{
        CallSiteDescriptor, CallSiteKind, DescriptorIdentity, ExecutionError, FrameDescriptor,
        FrameDescriptorV2Spec, SemanticRegistrationV1, SlotKind, StacklessTarget,
    };
    use crate::execution::{probe_call, probe_continue, probe_control, probe_spec};
    use crate::heap::Permanence;
    use crate::raw::MethodAddress;
    use crate::registration::{
        ABORT_V1, BirnamResumableRegistrationV1, CARRIER_VERSION_V1, PREPARE_V1,
        birnam_id_cancel_resumable_unit_v1, birnam_id_stackless_block_vtable_image_reference_v1,
        birnam_id_stackless_block_vtable_matches_v1,
        birnam_id_stackless_resumable_manifest_matches_v1, birnam_register_resumable_v1,
    };
    use crate::behaviors::AddMethod;
    use core::cell::Cell;
    use sha2::{Digest, Sha256};

    const CONTINUE_PROBE_TRANSITIONS: usize = 1_000_000;

    #[inline(never)]
    fn export_zero_payload_instance_with_method(
        runtime: &super::Runtime,
        name: &[u8],
        selector: Oop,
        method: Method,
    ) {
        let vtable = runtime.create_vtable(runtime.globals().object_vt);
        runtime.set_layout(vtable, core::ptr::null(), 0, SLOT_OOP);
        assert_eq!(
            runtime.add_method(vtable, selector, method),
            AddMethod::Added
        );
        let object = runtime.allocate_instance(vtable, 0);
        runtime.export(name, object).expect("static export");
    }

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
        static MEASURE_CHILD_VALUE_SLOT: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
        static DEPTH_BASELINE_SELECTOR: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
        static DEPTH_MIGRATED_SELECTOR: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
        static DEPTH_SYNCHRONOUS_SELECTOR: Cell<Oop> = const { Cell::new(core::ptr::null_mut()) };
    }

    unsafe extern "C" fn inert_finalizer(_: Oop) {}

    unsafe extern "C" fn inert_step(_: Oop, _: *const Oop, _: usize) -> Oop {
        core::ptr::null_mut()
    }

    unsafe extern "C" fn capture_once_step(machine: Oop, args: *const Oop, count: usize) -> Oop {
        with_active_if_present(|runtime| {
            if count != 1 || args.is_null() {
                return runtime.canonical().nil;
            }
            // SAFETY: this descriptor declares exactly one resume token.
            let token = unsafe { args.read() };
            match runtime.integer_value(token) {
                0 => runtime
                    .stackless_capture_current(machine, 0, 1)
                    .unwrap_or_else(|_| runtime.canonical().nil),
                1 => runtime
                    .stackless_return_at(machine, 0)
                    .unwrap_or_else(|_| runtime.canonical().nil),
                _ => runtime.canonical().nil,
            }
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn suspend_once_step(machine: Oop, args: *const Oop, count: usize) -> Oop {
        with_active_if_present(|runtime| {
            if count != 1 || args.is_null() {
                return runtime.canonical().nil;
            }
            // SAFETY: this descriptor declares exactly one resume token.
            let token = unsafe { args.read() };
            match runtime.integer_value(token) {
                0 => runtime
                    .stackless_suspend_current(machine, 0, 1)
                    .unwrap_or_else(|_| runtime.canonical().nil),
                1 => runtime
                    .stackless_return_at(machine, 0)
                    .unwrap_or_else(|_| runtime.canonical().nil),
                _ => runtime.canonical().nil,
            }
        })
        .unwrap_or(core::ptr::null_mut())
    }

    fn frozen_registration_envelope(
        manifest_name: &str,
        descriptor_name: &str,
        implementation: Oop,
    ) -> (Vec<u8>, [u8; 32], Vec<u8>, BirnamResumableRegistrationV1) {
        let fixture = |name: &str| {
            std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../abi/fixtures/stackless-s2")
                    .join(name),
            )
            .unwrap()
        };
        let manifest_bytes = fixture(manifest_name);
        let authority: [u8; 32] = manifest_bytes[0xc0..0xe0].try_into().unwrap();
        let descriptor = fixture(descriptor_name);
        let manifest =
            birnam_artifact::resumable_manifest::inspect(&manifest_bytes, authority).unwrap();
        let entry = manifest.entry(0).unwrap();
        let driver_argument_count = birnam_artifact::frame_descriptor::inspect(
            &descriptor,
            birnam_artifact::stackless_selection::SelectionSpec {
                source_revision: entry.source_revision,
                root_kind: entry.root_kind,
                root_index: entry.root_index,
                path: &entry.path.iter().collect::<Vec<_>>(),
                implementation_kind: entry.implementation_kind,
                disposition: birnam_artifact::stackless_selection::Disposition::ResumableRequired,
            },
        )
        .unwrap()
        .metadata()
        .driver_argument_count;
        let envelope = BirnamResumableRegistrationV1 {
            carrier_version: CARRIER_VERSION_V1,
            struct_size: core::mem::size_of::<BirnamResumableRegistrationV1>() as u32,
            operation: PREPARE_V1,
            flags: 0,
            manifest_entry_index: 0,
            reserved0: 0,
            registration_token: 0,
            descriptor_bytes: descriptor.as_ptr(),
            descriptor_length: descriptor.len() as u64,
            entry_thunk: Some(registered_probe_thunk),
            step_method: Some(inert_step),
            implementation_oop: implementation,
            code_generation_identity: 101,
            code_generation_lease: 103,
            portable_implementation_identity: entry.portable_implementation_identity,
            language_arity: entry.language_arity,
            driver_argument_count,
            descriptor_sha256: Sha256::digest(&descriptor).into(),
        };
        (manifest_bytes, authority, descriptor, envelope)
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

    unsafe extern "C" fn measured_registered_child_step(
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
            REGISTERED_FRAME_PIN_HIGH_WATER.set(
                REGISTERED_FRAME_PIN_HIGH_WATER.get().max(
                    runtime
                        .stackless_registration_diagnostics()
                        .active_frame_pins,
                ),
            );
            runtime.send_slice(
                machine,
                RETURN_PROBE_SELECTOR.get(),
                &[MEASURE_CHILD_VALUE_SLOT.get()],
            )
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn depth_baseline_method(
        receiver: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        with_active_if_present(|runtime| {
            if count != 1 || args.is_null() {
                return runtime.canonical().nil;
            }
            let depth = runtime.integer_value(unsafe { args.read() });
            if depth == 0 {
                return runtime.make_integer(0);
            }
            let next = runtime.make_integer(depth - 1);
            runtime.send_slice(receiver, DEPTH_BASELINE_SELECTOR.get(), &[next])
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn depth_synchronous_barrier_method(
        receiver: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        with_active_if_present(|runtime| {
            if count != 1 || args.is_null() {
                return runtime.canonical().nil;
            }
            let depth = runtime.integer_value(unsafe { args.read() });
            if depth == 0 {
                return runtime.make_integer(0);
            }
            let next = runtime.make_integer(depth - 1);
            runtime.send_slice(receiver, DEPTH_MIGRATED_SELECTOR.get(), &[next])
        })
        .unwrap_or(core::ptr::null_mut())
    }

    unsafe extern "C" fn depth_migrated_step(machine: Oop, args: *const Oop, count: usize) -> Oop {
        with_active_if_present(|runtime| {
            if count != 1 || args.is_null() {
                return runtime.canonical().nil;
            }
            let pc = runtime.integer_value(unsafe { args.read() });
            if pc == 0 {
                let selector_slot = runtime.make_integer(1);
                let oop_at_put = runtime.intern(b"oopAt:put:").expect("static selector");
                let stored = runtime.send_slice(
                    machine,
                    oop_at_put,
                    &[selector_slot, DEPTH_SYNCHRONOUS_SELECTOR.get()],
                );
                if stored != DEPTH_SYNCHRONOUS_SELECTOR.get() {
                    return runtime.canonical().nil;
                }
                runtime.send_slice(
                    machine,
                    CALL_SITE_PROBE_SELECTOR.get(),
                    &[runtime.make_integer(0)],
                )
            } else {
                runtime.send_slice(
                    machine,
                    RETURN_PROBE_SELECTOR.get(),
                    &[runtime.make_integer(3)],
                )
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

    // Address identity is asserted across codegen units by the redefinition probe.
    #[inline(never)]
    unsafe extern "C" fn synchronous_identity(_: Oop, args: *const Oop, count: usize) -> Oop {
        if count != 1 || args.is_null() {
            return core::ptr::null_mut();
        }
        // SAFETY: the probe requires one argument.
        unsafe { args.read() }
    }

    /// Design 74D probe: an `oopAt:put:` replacement that answers its value
    /// argument, so a store miss can be seen to deliver the rooted value.
    unsafe extern "C" fn synchronous_second_argument(
        _: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        if count != 2 || args.is_null() {
            return core::ptr::null_mut();
        }
        // SAFETY: the probe requires two arguments.
        unsafe { args.add(1).read() }
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

                let nil = runtime.canonical().nil;
                unsafe {
                    (&mut *runtime.state.get())
                        .execution
                        .commit_current_continue(current_thread_token(), capability, nil)
                        .unwrap();
                }
                let replacement_value = runtime.make_integer(211);
                assert_eq!(
                    runtime.send_slice(machine, oop_at_put, &[value_slot, replacement_value]),
                    replacement_value
                );
                let missing_slot = runtime.make_integer(5);
                assert_eq!(
                    runtime.send_slice(machine, oop_at_put, &[missing_slot, replacement_value]),
                    runtime.canonical().nil,
                    "out-of-range write is rejected"
                );

                let call_action = runtime.send_slice(machine, call_site, &[index]);
                assert_eq!(
                    runtime.send_slice(call_action, action_kind, &[]),
                    runtime.intern(b"call").expect("static symbol")
                );
                unsafe {
                    let state = &mut *runtime.state.get();
                    let arena = state
                        .execution
                        .machine_mut(current_thread_token(), capability)
                        .unwrap()
                        .arena_mut();
                    let frame = arena.current_handle().unwrap();
                    arena
                        .commit_call_transition(frame, 2, nil, replacement_value)
                        .unwrap();
                }
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
    fn host_capture_restores_once_and_supplies_the_saved_result_slot() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        let machine = runtime
            .create_stackless_machine()
            .expect("stackless Machine creates");
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 301,
                generation: 1,
                source_map: 301,
            },
            program_counter_count: 2,
            driver_argument_count: 1,
            call_sites: vec![probe_control(0, 1)],
            step_method: capture_once_step,
            edges: vec![probe_call(0, 1, 0, 0, 1)],
            ..probe_spec(1, 2)
        })
        .expect("capture descriptor validates");
        let initial = runtime.make_integer(7);
        let continuation = runtime
            .drive_stackless(machine, descriptor, &[(0, initial)])
            .expect("capture returns a process-local handle");
        let objects = unsafe { (*runtime.state.get()).execution.objects() }.unwrap();
        assert_eq!(
            runtime.checked_vtable_of(continuation),
            objects.continuation_vtable
        );
        let capability = runtime.stackless_machine_capability(machine).unwrap();
        assert_eq!(
            unsafe {
                (*runtime.state.get())
                    .execution
                    .machine(current_thread_token(), capability)
                    .unwrap()
                    .lifecycle()
            },
            crate::execution::MachineLifecycle::Suspended
        );

        let supplied = runtime.make_integer(42);
        assert_eq!(
            runtime
                .restore_stackless_continuation(machine, continuation, supplied)
                .expect("first restore resumes and completes"),
            supplied
        );
        assert_eq!(
            runtime.restore_stackless_continuation(machine, continuation, supplied),
            Err(ExecutionError::ContinuationConsumed)
        );
    }

    #[test]
    fn host_suspension_resumes_with_one_supplied_answer() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        let machine = runtime
            .create_stackless_machine()
            .expect("stackless Machine creates");
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 302,
                generation: 1,
                source_map: 302,
            },
            program_counter_count: 2,
            driver_argument_count: 1,
            call_sites: vec![probe_control(0, 1)],
            step_method: suspend_once_step,
            edges: vec![probe_call(0, 1, 0, 0, 1)],
            ..probe_spec(1, 2)
        })
        .expect("suspension descriptor validates");
        let initial = runtime.make_integer(5);
        assert_eq!(
            runtime
                .drive_stackless(machine, descriptor, &[(0, initial)])
                .expect("suspension returns its Machine handle"),
            machine
        );
        let supplied = runtime.make_integer(99);
        assert_eq!(
            runtime
                .resume_stackless_machine(machine, supplied)
                .expect("resume completes"),
            supplied
        );
        assert_eq!(
            runtime.resume_stackless_machine(machine, supplied),
            Err(ExecutionError::MachineNotSuspended)
        );
    }

    #[test]
    fn capture_at_nested_entry_barrier_is_rejected_without_disturbing_outer_frame() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        let machine = runtime.create_stackless_machine().unwrap();
        let capability = runtime.stackless_machine_capability(machine).unwrap();
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 303,
                generation: 1,
                source_map: 303,
            },
            program_counter_count: 1,
            driver_argument_count: 0,
            call_sites: Vec::new(),
            step_method: inert_step,
            edges: vec![],
            ..probe_spec(1, 1)
        })
        .unwrap();
        let nil = runtime.canonical().nil;
        unsafe {
            let state = &mut *runtime.state.get();
            let owner = current_thread_token();
            let outer = {
                let machine_state = state.execution.machine_mut(owner, capability).unwrap();
                machine_state.begin_initial_drive(owner).unwrap();
                machine_state
                    .arena_mut()
                    .push(descriptor.clone(), 0, nil, &[(0, nil)])
                    .unwrap()
            };
            {
                let machine_state = state.execution.machine_mut(owner, capability).unwrap();
                machine_state.begin_nested_drive(owner).unwrap();
                machine_state
                    .arena_mut()
                    .push(descriptor, 0, nil, &[(0, nil)])
                    .unwrap();
            }
            let delimiter = state
                .execution
                .current_delimiter(owner, capability)
                .unwrap();
            assert_eq!(
                state.execution.publish_capture(
                    &mut state.heap,
                    owner,
                    capability,
                    delimiter,
                    0,
                    0,
                ),
                Err(ExecutionError::Machine(
                    crate::execution::MachineError::ContinuationBarrier
                ))
            );
            let machine_state = state.execution.machine_mut(owner, capability).unwrap();
            machine_state.arena_mut().pop().unwrap();
            machine_state.complete_nested_drive(owner).unwrap();
            assert_eq!(machine_state.arena().current_handle(), Some(outer));
            assert_eq!(
                machine_state.lifecycle(),
                crate::execution::MachineLifecycle::Running
            );
        }
    }

    #[test]
    fn continuation_owns_the_registered_generation_pin_until_restore() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        let selector = runtime.intern(b"capturedRegisteredFrame").unwrap();
        let receiver = runtime.canonical().object_prototype;
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 304,
                generation: 17,
                source_map: 304,
            },
            program_counter_count: 2,
            driver_argument_count: 1,
            call_sites: vec![probe_control(0, 1)],
            step_method: capture_once_step,
            edges: vec![probe_call(0, 1, 0, 0, 1)],
            ..probe_spec(1, 2)
        })
        .unwrap();
        runtime
            .register_stackless_resumable(SemanticRegistrationV1::new(
                registered_probe_thunk,
                descriptor,
                0,
                0,
                1,
            ))
            .unwrap();
        let _ = runtime.add_method(
            runtime.globals().object_vt,
            selector,
            registered_probe_thunk,
        );
        let read = runtime.enter_managed_turn();
        let method = runtime
            .lookup_method(runtime.globals().object_vt, selector)
            .unwrap();
        let target = unsafe {
            (&mut *runtime.state.get())
                .execution
                .classify_resumable(method)
                .unwrap()
                .unwrap()
        };
        drop(read);
        let StacklessTarget::Registered { entry } = target else {
            panic!("registered carrier must classify")
        };
        let materialized = runtime
            .materialize_registered_target(entry, receiver, &[])
            .unwrap();
        let machine = runtime.create_stackless_machine().unwrap();
        let continuation = runtime
            .drive_stackless_with_resolver(
                machine,
                materialized.descriptor,
                &materialized.initial_oops,
                Some(entry),
                |runtime, receiver, selector, _| {
                    runtime.resolve_stackless_target(receiver, selector)
                },
            )
            .unwrap();
        let captured = runtime.stackless_registration_diagnostics();
        assert_eq!(captured.active_frame_pins, 0);
        assert_eq!(captured.continuation_frame_pins, 1);

        let answer = runtime.make_integer(123);
        assert_eq!(
            runtime
                .restore_stackless_continuation(machine, continuation, answer)
                .unwrap(),
            answer
        );
        let completed = runtime.stackless_registration_diagnostics();
        assert_eq!(completed.active_frame_pins, 0);
        assert_eq!(completed.continuation_frame_pins, 0);
    }

    #[test]
    fn continuation_finalization_releases_detached_pin_exactly_once() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        let selector = runtime.intern(b"finalizedCapturedFrame").unwrap();
        let receiver = runtime.canonical().object_prototype;
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 305,
                generation: 18,
                source_map: 305,
            },
            program_counter_count: 2,
            driver_argument_count: 1,
            call_sites: vec![probe_control(0, 1)],
            step_method: capture_once_step,
            edges: vec![probe_call(0, 1, 0, 0, 1)],
            ..probe_spec(1, 2)
        })
        .unwrap();
        runtime
            .register_stackless_resumable(SemanticRegistrationV1::new(
                registered_probe_thunk,
                descriptor,
                0,
                0,
                1,
            ))
            .unwrap();
        let _ = runtime.add_method(
            runtime.globals().object_vt,
            selector,
            registered_probe_thunk,
        );
        let read = runtime.enter_managed_turn();
        let method = runtime
            .lookup_method(runtime.globals().object_vt, selector)
            .unwrap();
        let StacklessTarget::Registered { entry } = (unsafe {
            (&mut *runtime.state.get())
                .execution
                .classify_resumable(method)
                .unwrap()
                .unwrap()
        }) else {
            panic!("registered carrier must classify")
        };
        drop(read);
        let materialized = runtime
            .materialize_registered_target(entry, receiver, &[])
            .unwrap();
        let machine = runtime.create_stackless_machine().unwrap();
        let continuation = runtime
            .drive_stackless_with_resolver(
                machine,
                materialized.descriptor,
                &materialized.initial_oops,
                Some(entry),
                |runtime, receiver, selector, _| {
                    runtime.resolve_stackless_target(receiver, selector)
                },
            )
            .unwrap();
        assert_eq!(
            runtime
                .stackless_registration_diagnostics()
                .continuation_frame_pins,
            1
        );
        unsafe { runtime.finalize_stackless_continuation(continuation) };
        unsafe { runtime.finalize_stackless_continuation(continuation) };
        assert_eq!(
            runtime
                .stackless_registration_diagnostics()
                .continuation_frame_pins,
            0
        );
        assert_eq!(
            runtime.restore_stackless_continuation(machine, continuation, runtime.canonical().nil,),
            Err(ExecutionError::ContinuationConsumed)
        );
    }

    #[test]
    fn reachable_continuation_traces_detached_frame_roots_and_owning_machine() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        let machine = runtime.create_stackless_machine().unwrap();
        let selected = runtime.allocate_instance(runtime.globals().object_vt, 0);
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 306,
                generation: 1,
                source_map: 306,
            },
            program_counter_count: 2,
            driver_argument_count: 1,
            call_sites: vec![probe_control(0, 1)],
            step_method: capture_once_step,
            edges: vec![probe_call(0, 1, 0, 0, 1)],
            ..probe_spec(1, 2)
        })
        .unwrap();
        let continuation = runtime
            .drive_stackless(machine, descriptor, &[(0, selected)])
            .unwrap();

        let initial_roots = unsafe {
            let state = &mut *runtime.state.get();
            state.heap.clear_marks();
            [continuation]
        };
        runtime.enter(|| runtime.run_mark_phase((0, 0), &initial_roots));
        assert_eq!(runtime.is_marked_for_test(machine), Some(true));
        assert_eq!(runtime.is_marked_for_test(selected), Some(true));

        let initial_roots = unsafe {
            let state = &mut *runtime.state.get();
            state.heap.clear_marks();
            [machine]
        };
        runtime.enter(|| runtime.run_mark_phase((0, 0), &initial_roots));
        assert_eq!(runtime.is_marked_for_test(selected), Some(false));
        assert_eq!(runtime.is_marked_for_test(continuation), Some(false));
    }

    #[test]
    fn namespace_lookup_preserves_interning_rejection_and_runtime_isolation() {
        let runtimes = vec![build_bootstrap(), build_bootstrap()];
        let name = b"namespace-lookup-absent-\xff";
        for bootstrap in &runtimes {
            let runtime = &bootstrap.runtime;
            let before = unsafe { (*runtime.state.get()).symbols.count() };
            assert_eq!(runtime.import(b"namespace-lookup-invalid\0name"), Err(crate::symbol::EmbeddedNul));
            assert_eq!(unsafe { (*runtime.state.get()).symbols.count() }, before);
            assert_eq!(runtime.import(name), Ok(core::ptr::null_mut()));
            assert_eq!(unsafe { (*runtime.state.get()).symbols.count() }, before + 1);
            let symbol = runtime.intern(name).unwrap();
            assert_eq!(runtime.intern(name).unwrap(), symbol);
            assert_eq!(unsafe { (*runtime.state.get()).symbols.count() }, before + 1);
            let value = runtime.canonical().nil;
            runtime.export(name, value).unwrap();
            assert_eq!(runtime.import(name), Ok(value));
            runtime.export(b"", value).unwrap();
            assert_eq!(runtime.import(b""), Ok(value));
            assert_eq!(runtime.import(name), Ok(runtime.canonical().nil));
        }
        assert_ne!(runtimes[0].runtime.intern(name).unwrap(), runtimes[1].runtime.intern(name).unwrap());
    }

    #[test]
    fn namespace_publication_preserves_rejection_mirrors_and_runtime_isolation() {
        #[repr(C)]
        struct DictionaryView {
            capacity: usize,
            tally: usize,
            keys: *const Oop,
            values: *const crate::abi::id_protocol::Operand,
        }
        let runtimes = vec![build_bootstrap(), build_bootstrap()];
        for bootstrap in &runtimes {
            let runtime = &bootstrap.runtime;
            let name = b"namespace-publication-\xff";
            let value = runtime.canonical().nil;
            let snapshot = || unsafe {
                let state = &*runtime.state.get();
                (state.symbols.count(), state.environment.count(), state.vtables.mutation_epoch())
            };
            let before = snapshot();
            assert_eq!(runtime.export(b"invalid\0publication", value), Err(crate::symbol::EmbeddedNul));
            assert_eq!(snapshot(), before);
            let symbol = runtime.intern(name).unwrap();
            let mirror = || unsafe {
                let view = &*runtime.canonical().environment.cast::<DictionaryView>();
                (0..view.tally).find(|&i| *view.keys.add(i) == symbol)
                    .and_then(|i| (*view.values.add(i)).as_object())
            };
            for value in [value, core::ptr::null_mut(), value] {
                let before = snapshot();
                assert_eq!(runtime.export(name, value), Ok(value));
                assert_eq!(runtime.import(name), Ok(value));
                assert_eq!(mirror(), Some(value));
                assert_eq!(snapshot().2, before.2 + 1);
            }
            // Early bootstrap publication still works without a mirror.
            let dictionaries = unsafe { (*runtime.state.get()).internal_dictionaries.take() };
            let before = snapshot();
            assert_eq!(runtime.export(name, core::ptr::null_mut()), Ok(core::ptr::null_mut()));
            assert_eq!(runtime.import(name), Ok(core::ptr::null_mut()));
            assert_eq!(snapshot().2, before.2);
            assert_eq!(mirror(), Some(value));
            unsafe { (*runtime.state.get()).internal_dictionaries = dictionaries };
            assert_eq!(runtime.export(name, value), Ok(value));
        }
        assert_ne!(runtimes[0].runtime.intern(b"namespace-publication-\xff").unwrap(),
                   runtimes[1].runtime.intern(b"namespace-publication-\xff").unwrap());
    }

    #[test]
    fn namespace_export_follows_independent_moved_runtime_state() {
        let runtimes = vec![build_bootstrap(), build_bootstrap()];
        assert_ne!(runtimes[0].runtime.namespace_export(), runtimes[1].runtime.namespace_export());
        let name = c"independent-export";
        for bootstrap in &runtimes {
            let runtime = &bootstrap.runtime;
            runtime.enter(|| {
                let exports = runtime.namespace_export();
                let before = unsafe { (*runtime.state.get()).symbols.count() };
                for value in [runtime.canonical().nil, runtime.canonical().object_prototype, core::ptr::null_mut()] {
                    assert_eq!(unsafe { birnam_abi::namespace_export::export(exports, name.as_ptr(), value) }, value);
                    assert_eq!(runtime.import(name.to_bytes()), Ok(value));
                }
                assert_eq!(unsafe { (*runtime.state.get()).symbols.count() }, before + 1);
                let canonical = runtime.canonical_namespace_export();
                unsafe { (*exports).context = core::ptr::null_mut(); (*exports).export = None; }
                assert_eq!(runtime.canonical_namespace_export().context, canonical.context);
                let value = runtime.canonical().true_object;
                assert_eq!(unsafe { birnam_abi::namespace_export::export(&canonical, name.as_ptr(), value) }, value);
                assert_eq!(runtime.import(name.to_bytes()), Ok(value));
                // Destruction uses the owned allocation, not the mutable context.
            });
        }
        assert_ne!(runtimes[0].runtime.intern(name.to_bytes()).unwrap(), runtimes[1].runtime.intern(name.to_bytes()).unwrap());
    }

    #[test]
    fn namespace_import_follows_independent_moved_runtime_state() {
        let runtimes = vec![build_bootstrap(), build_bootstrap()];
        let first = runtimes[0].runtime.namespace_import();
        let second = runtimes[1].runtime.namespace_import();
        assert_ne!(first, second);
        let name = c"independent-import";
        for bootstrap in &runtimes {
            let runtime = &bootstrap.runtime;
            runtime.enter(|| {
                let imports = runtime.namespace_import();
                let before = unsafe { (*runtime.state.get()).symbols.count() };
                assert_eq!(unsafe { birnam_abi::namespace_import::import(imports, name.as_ptr()) }, core::ptr::null_mut());
                assert_eq!(unsafe { (*runtime.state.get()).symbols.count() }, before + 1);
                for value in [runtime.canonical().nil, runtime.canonical().object_prototype, core::ptr::null_mut()] {
                    runtime.export(name.to_bytes(), value).unwrap();
                    assert_eq!(unsafe { birnam_abi::namespace_import::import(imports, name.as_ptr()) }, value);
                }
                let canonical = runtime.canonical_namespace_import();
                unsafe { (*imports).context = core::ptr::null_mut(); (*imports).import = None; }
                assert_eq!(runtime.canonical_namespace_import().context, canonical.context);
                assert_eq!(unsafe { birnam_abi::namespace_import::import(&canonical, name.as_ptr()) }, core::ptr::null_mut());
                // Destruction must still use the original owned address.
            });
        }
        assert_ne!(runtimes[0].runtime.intern(name.to_bytes()).unwrap(), runtimes[1].runtime.intern(name.to_bytes()).unwrap());
    }

    #[test]
    fn namespace_presence_follows_independent_moved_runtime_state() {
        let runtimes = vec![build_bootstrap(), build_bootstrap()];
        let first = runtimes[0].runtime.namespace_presence();
        let second = runtimes[1].runtime.namespace_presence();
        assert_ne!(first, second);
        let name = c"independent-presence";
        for bootstrap in &runtimes {
            let runtime = &bootstrap.runtime;
            runtime.enter(|| {
                let presence = runtime.namespace_presence();
                let before = unsafe { (*runtime.state.get()).symbols.count() };
                assert_eq!(unsafe { birnam_abi::namespace_presence::contains(presence, name.as_ptr()) }, 0);
                assert_eq!(unsafe { (*runtime.state.get()).symbols.count() }, before + 1);
                runtime.export(name.to_bytes(), runtime.canonical().nil).unwrap();
                assert_eq!(unsafe { birnam_abi::namespace_presence::contains(presence, name.as_ptr()) }, 1);
                assert_eq!(unsafe { (*runtime.state.get()).symbols.count() }, before + 1);
                runtime.export(name.to_bytes(), core::ptr::null_mut()).unwrap();
                assert_eq!(unsafe { birnam_abi::namespace_presence::contains(presence, name.as_ptr()) }, 0);
            });
        }
    }

    #[test]
    fn namespace_names_follow_independent_moved_runtime_state() {
        // Moving completed bootstraps must not leave a captured Runtime pointer.
        let runtimes = vec![build_bootstrap(), build_bootstrap()];
        assert_ne!(runtimes[0].runtime.namespace_names(), runtimes[1].runtime.namespace_names());
        for (index, bootstrap) in runtimes.iter().enumerate() {
            let runtime = &bootstrap.runtime;
            let object = runtime.allocate_instance(runtime.globals().object_vt, 0);
            let alias = format!("independent-{index}");
            runtime.export(alias.as_bytes(), object).unwrap();
            runtime.enter(|| {
                let enumeration = unsafe { &*runtime.namespace_enumeration() };
                assert_eq!(unsafe { enumeration.count.count.unwrap()(enumeration.count.context) }, runtime.export_count());
                for index in 0..=runtime.export_count() {
                    assert_eq!(unsafe { enumeration.name_at.name_at.unwrap()(enumeration.name_at.context, index) }, runtime.export_name_at(index));
                }
                let names = unsafe { *runtime.namespace_names() };
                for value in [object, core::ptr::null_mut()] {
                    assert_eq!(unsafe { names.name_of.unwrap()(names.context, value) }, runtime.exported_name(value));
                }
                assert_eq!(unsafe { core::ffi::CStr::from_ptr(names.name_of.unwrap()(names.context, object)) }.to_bytes(), alias.as_bytes());
            });
        }
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
            assert_eq!(runtime.checked_vtable_of(string), runtime.string_behavior());
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
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 1,
                generation: 1,
                source_map: 1,
            },
            program_counter_count: 1,
            driver_argument_count: 0,
            call_sites: Vec::new(),
            step_method: inert_step,
            edges: vec![],
            ..probe_spec(3, 1)
        })
        .expect("integration descriptor validates");

        let (frame, continue_action, call_action, tail_call_action, return_action, s3_actions) = unsafe {
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
            (
                frame,
                actions.0,
                actions.1,
                actions.2,
                actions.3,
                machine_state.s3_scratch_actions(),
            )
        };

        let initial_roots = unsafe {
            let state = &mut *runtime.state.get();
            state.heap.clear_marks();
            [machine]
        };
        runtime.enter(|| runtime.run_mark_phase((0, 0), &initial_roots));
        assert_eq!(runtime.is_marked_for_test(rooted), Some(true));
        assert_eq!(runtime.is_marked_for_test(popped_only), Some(true));
        assert_eq!(runtime.is_marked_for_test(continue_action), Some(true));
        assert_eq!(runtime.is_marked_for_test(call_action), Some(true));
        assert_eq!(runtime.is_marked_for_test(tail_call_action), Some(true));
        assert_eq!(runtime.is_marked_for_test(return_action), Some(true));
        for action in s3_actions {
            assert_eq!(runtime.is_marked_for_test(action), Some(true));
        }

        let initial_roots = unsafe {
            let state = &mut *runtime.state.get();
            let machine_state = state
                .execution
                .machine_mut(current_thread_token(), capability)
                .expect("Machine state remains live");
            assert_eq!(machine_state.arena_mut().pop(), Ok(frame));
            state.heap.clear_marks();
            [machine]
        };
        runtime.enter(|| runtime.run_mark_phase((0, 0), &initial_roots));
        assert_eq!(runtime.is_marked_for_test(rooted), Some(false));
        assert_eq!(runtime.is_marked_for_test(popped_only), Some(false));
        assert_eq!(runtime.is_marked_for_test(continue_action), Some(true));
        assert_eq!(runtime.is_marked_for_test(call_action), Some(true));
        assert_eq!(runtime.is_marked_for_test(tail_call_action), Some(true));
        assert_eq!(runtime.is_marked_for_test(return_action), Some(true));
        for action in s3_actions {
            assert_eq!(runtime.is_marked_for_test(action), Some(true));
        }
    }

    #[test]
    fn compatibility_mode_roots_the_complete_symbol_interner() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let symbol = runtime
            .intern(b"legacy-native-retained-name")
            .expect("compatibility Symbol interns");

        runtime.gc_mark_floor_roots_for_test();
        assert_eq!(runtime.is_marked_for_test(symbol), Some(true));
    }

    #[test]
    fn compatibility_limits_are_explicit_nonzero_and_cover_current_usage() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let (count, name_bytes) = unsafe {
            let state = &*runtime.state.get();
            (state.symbols.count(), state.symbols.owned_name_bytes())
        };

        assert!(!runtime.configure_symbol_compaction(false, 0, name_bytes));
        assert!(!runtime.configure_symbol_compaction(false, count, 0));
        assert!(!runtime.configure_symbol_compaction(false, count.saturating_sub(1), name_bytes));
        assert!(runtime.configure_symbol_compaction(false, count, name_bytes));
        assert!(runtime.configure_symbol_compaction(true, 0, 0));
    }

    #[test]
    fn collection_prunes_unreachable_symbols_and_the_weak_dictionary_mirror() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.enable_symbol_compaction_for_test();
        let baseline = runtime.symbol_count();
        let enumeration = runtime.vtable_enumeration();
        let observed = |index| runtime.enter(|| unsafe {
            birnam_abi::vtable_enumeration::selector_at(enumeration, runtime.canonical().symbol_list, index)
        });
        let symbols_dictionary = runtime.canonical().symbol_list;
        let ephemeral = runtime
            .intern(b"j2-ephemeral-symbol")
            .expect("ephemeral Symbol interns");
        assert_eq!(runtime.symbol_count(), baseline + 1);
        assert_eq!(
            runtime.vtable_selector_at(symbols_dictionary, baseline),
            ephemeral
        );
        assert_eq!(
            runtime.symbol_bytes(ephemeral),
            Some(&b"j2-ephemeral-symbol"[..])
        );

        assert_eq!(observed(baseline), ephemeral);
        let reclaimed = runtime.enter(|| runtime.gc_collect_from_native_range(0, 0));
        assert!(reclaimed >= 1);
        assert_eq!(runtime.symbol_count(), baseline);
        assert_eq!(runtime.symbol_bytes(ephemeral), None);
        assert!(
            runtime
                .vtable_selector_at(symbols_dictionary, baseline)
                .is_null()
        );

        assert!(observed(baseline).is_null());
        let replacement = runtime
            .intern(b"j2-ephemeral-symbol")
            .expect("collected Symbol value re-interns");
        assert_eq!(runtime.symbol_count(), baseline + 1);
        assert_eq!(
            runtime.symbol_bytes(replacement),
            Some(&b"j2-ephemeral-symbol"[..])
        );
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
    fn rooted_private_behavior_preserves_its_existing_strong_owner_edge() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let object = runtime.allocate_instance(runtime.globals().object_vt, 0);
        let private = runtime.ensure_private_vtable(object);
        runtime.export(b"RootedPrivateBehavior", private).unwrap();
        runtime.enter(|| runtime.gc_collect_from_native_range(0, 0));
        assert_eq!(runtime.checked_vtable_of(object), private);
        assert_eq!(runtime.vtable_owner(private), object);
    }

    #[test]
    fn owner_and_parent_messages_preserve_edges_and_release_replaced_edges() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let meta = runtime.globals().vtable_vt;
        let base = runtime.globals().object_vt;
        let behavior = runtime.create_vtable(base);
        let owner = runtime.allocate_instance(base, 0);
        let parent = runtime.create_vtable(base);
        let set_owner = runtime.intern(b"ownerOf:put:").unwrap();
        let set_parent = runtime.intern(b"parentOf:put:").unwrap();
        runtime.export(b"ProtocolRootedBehavior", behavior).unwrap();
        runtime.enter(|| {
            assert_eq!(
                runtime.send_slice(meta, set_owner, &[behavior, owner]),
                behavior
            );
            assert_eq!(
                runtime.send_slice(meta, set_parent, &[behavior, parent]),
                behavior
            );
            runtime.gc_collect_from_native_range(0, 0);
            assert_eq!(runtime.checked_vtable_of(owner), base);
            assert_eq!(runtime.vtable_parent(behavior), parent);
            assert_eq!(runtime.checked_vtable_of(parent), meta);
            let nil = core::ptr::null_mut();
            runtime.send_slice(meta, set_owner, &[behavior, nil]);
            runtime.send_slice(meta, set_parent, &[behavior, base]);
            runtime.gc_collect_from_native_range(0, 0);
            assert!(runtime.checked_vtable_of(owner).is_null());
            assert!(runtime.checked_vtable_of(parent).is_null());
        });
    }

    #[test]
    fn dynamic_vtable_registry_is_weak_and_live_headers_preserve_identity() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let globals = runtime.globals();
        let baseline = runtime.vtable_count();

        let dead_owner = runtime.allocate_instance(globals.object_vt, 0);
        let dead_vtable = runtime.ensure_private_vtable(dead_owner);
        let live_owner = runtime.allocate_instance(globals.object_vt, 0);
        let live_vtable = runtime.ensure_private_vtable(live_owner);
        runtime.export(b"WeakVtableLive", live_owner).unwrap();

        assert_eq!(runtime.vtable_count(), baseline + 2);
        assert_eq!(runtime.checked_vtable_of(live_owner), live_vtable);
        assert_eq!(runtime.vtable_owner(dead_vtable), dead_owner);
        assert_eq!(runtime.vtable_owner(live_vtable), live_owner);

        let reclaimed = runtime.enter(|| runtime.gc_collect_from_native_range(0, 0));
        assert!(reclaimed >= 2, "the dead owner/vtable cycle is reclaimed");
        assert_eq!(runtime.vtable_count(), baseline + 1);
        assert!(runtime.vtable_parent(dead_vtable).is_null());
        assert_eq!(runtime.checked_vtable_of(live_owner), live_vtable);
        assert_eq!(runtime.vtable_owner(live_vtable), live_owner);
        assert!((0..runtime.vtable_count()).any(|index| runtime.vtable_at(index) == live_vtable));
        assert!((0..runtime.vtable_count()).all(|index| runtime.vtable_at(index) != dead_vtable));
    }

    #[test]
    fn root_registry_follows_independent_moved_runtime_state() {
        thread_local! { static CALLS: Cell<usize> = const { Cell::new(0) }; }
        unsafe extern "C" fn roots(_: crate::abi::MarkFn) { CALLS.set(CALLS.get() + 1); }
        let first = build_bootstrap();
        // Internal registration also works before this Runtime has an active entry.
        first.runtime.gc_register_root_provider(roots);
        let api = first.runtime.root_registry();
        let mut runtimes = Vec::with_capacity(1);
        runtimes.push(first);
        runtimes.push(build_bootstrap());
        assert_eq!(runtimes[0].runtime.root_registry(), api);
        assert_ne!(runtimes[1].runtime.root_registry(), api);
        unsafe { (*api).context = core::ptr::null_mut(); (*api).register = None; }
        runtimes[0].runtime.gc_register_root_provider(roots);
        CALLS.set(0);
        for bootstrap in &runtimes {
            bootstrap.runtime.enter(|| bootstrap.runtime.gc_collect_from_native_range(0, 0));
        }
        assert_eq!(CALLS.get(), 2);
        drop(runtimes.remove(0));
        let second = runtimes.pop().unwrap();
        second.runtime.enter(|| unsafe {
            birnam_abi::root_registry::register(second.runtime.root_registry(), roots);
        });
        second.runtime.enter(|| second.runtime.gc_collect_from_native_range(0, 0));
        assert_eq!(CALLS.get(), 3);
    }

    #[test]
    fn gc_settings_follow_independent_moved_runtime_state() {
        let first = build_bootstrap();
        let second = build_bootstrap();
        first.runtime.gc_set_auto_requested(false);
        first.runtime.gc_set_precise(true);
        let api = first.runtime.gc_settings();
        let mut runtimes = vec![first, second];
        assert_eq!(runtimes[0].runtime.gc_settings(), api);
        assert_ne!(runtimes[1].runtime.gc_settings(), api);
        // Public instrumentation must not redirect canonical internal queries.
        unsafe { (*api).context = core::ptr::null_mut(); (*api).set_auto = None; (*api).set_precise = None; }
        for (index, bootstrap) in runtimes.iter().enumerate() {
            let metadata = unsafe { &(*bootstrap.runtime.state.get()).metadata };
            assert_eq!(metadata.precise(), index == 0);
            assert_eq!(metadata.auto_requested(), index != 0);
        }
        runtimes[0].runtime.gc_set_auto_requested(true);
        runtimes[0].runtime.gc_set_precise(false);
        let second = runtimes.pop().unwrap();
        drop(runtimes);
        second.runtime.enter(|| unsafe {
            birnam_abi::gc_settings::set_auto(second.runtime.gc_settings(), 0);
            birnam_abi::gc_settings::set_precise(second.runtime.gc_settings(), -1);
        });
        let metadata = unsafe { &(*second.runtime.state.get()).metadata };
        assert!(metadata.precise());
        assert!(!metadata.auto_requested());
    }

    #[test]
    fn payload_layout_releases_once_at_replacement_retirement_and_runtime_destruction() {
        use birnam_abi::payload_layout::{ApiV1, OwnedV1};
        use crate::heap::Heap;
        use core::cell::RefCell;
        thread_local! {
            static EVENTS: RefCell<Vec<Option<usize>>> = const { RefCell::new(Vec::new()) };
            static RETIRING: Cell<(*const Heap, Oop, usize)> = const {
                Cell::new((core::ptr::null(), core::ptr::null_mut(), 0))
            };
        }
        unsafe extern "C" fn release(owned: OwnedV1) {
            let (heap, vtable, prefix) = RETIRING.get();
            if prefix != 0 && owned.view.kinds.addr() == prefix {
                // The collector borrows Behaviors here, not this separate Heap.
                assert_eq!(unsafe { (*heap).payload_size(vtable) }, Some(40));
            }
            if owned.view.count != 0 {
                EVENTS.with_borrow_mut(|events| events.push(Some(owned.view.kinds.addr())));
            }
            let factory = unsafe { &*birnam_payload_layout::birnam_payload_layout_factory_v1() };
            unsafe { factory.release.unwrap()(owned) };
        }
        unsafe extern "C" fn destroy(api: *mut ApiV1) {
            EVENTS.with_borrow_mut(|events| events.push(None));
            let factory = unsafe { &*birnam_payload_layout::birnam_payload_layout_factory_v1() };
            unsafe { factory.destroy.unwrap()(api) };
        }
        let mut factory = unsafe { *birnam_payload_layout::birnam_payload_layout_factory_v1() };
        factory.release = Some(release);
        factory.destroy = Some(destroy);
        crate::payload_layout::TEST_FACTORY.set(Some(factory));
        let bootstrap = build_bootstrap();
        crate::payload_layout::TEST_FACTORY.set(None);
        let runtime = &bootstrap.runtime;
        let dead = runtime.create_vtable(runtime.globals().object_vt);
        let live = runtime.create_vtable(runtime.globals().object_vt);
        runtime.export(b"PayloadLayoutRetained", live).unwrap();
        runtime.set_layout(dead, [SLOT_OOP, 9].as_ptr(), 2, crate::abi::SLOT_RAW);
        let old = runtime.payload_layout_view(dead);
        EVENTS.with_borrow_mut(Vec::clear);
        runtime.set_layout(dead, old.kinds, old.count, SLOT_OOP);
        EVENTS.with_borrow(|events| assert_eq!(*events, vec![Some(old.kinds.addr())]));
        let retired = runtime.payload_layout_view(dead).kinds.addr();
        assert_ne!(retired, old.kinds.addr());
        runtime.set_layout(live, [SLOT_OOP].as_ptr(), 1, SLOT_OOP);
        let retained = runtime.payload_layout_view(live).kinds.addr();
        EVENTS.with_borrow_mut(Vec::clear);
        RETIRING.set((unsafe { &raw const (*runtime.state.get()).heap }, dead, retired));
        runtime.enter(|| runtime.gc_collect_from_native_range(0, 0));
        RETIRING.set((core::ptr::null(), core::ptr::null_mut(), 0));
        assert!(runtime.checked_vtable_of(dead).is_null());
        assert_eq!(runtime.payload_layout_view(live).kinds.addr(), retained);
        EVENTS.with_borrow(|events| assert_eq!(*events, vec![Some(retired)]));
        runtime.enter(|| runtime.gc_collect_from_native_range(0, 0));
        EVENTS.with_borrow(|events| assert_eq!(*events, vec![Some(retired)]));
        EVENTS.with_borrow_mut(Vec::clear);
        drop(bootstrap);
        EVENTS.with_borrow(|events| {
            assert_eq!(events.iter().filter(|event| **event == Some(retained)).count(), 1);
            assert_eq!(events.iter().filter(|event| event.is_none()).count(), 1);
            assert_eq!(events.last(), Some(&None), "release records before destroying the capability");
        });
    }

    #[test]
    fn payload_layout_follows_exact_records_across_runtime_moves_and_public_instrumentation() {
        let first = build_bootstrap();
        let second = build_bootstrap();
        let a = first.runtime.create_vtable(first.runtime.globals().object_vt);
        let b = second.runtime.create_vtable(second.runtime.globals().object_vt);
        first.runtime.set_layout(a, [SLOT_OOP, 9].as_ptr(), 2, crate::abi::SLOT_RAW);
        second.runtime.set_layout(b, core::ptr::null(), 0, crate::abi::SLOT_CODE);
        let api = first.runtime.payload_layout();
        let mut runtimes = vec![first, second];
        assert_eq!(runtimes[0].runtime.payload_layout(), api);
        assert_ne!(runtimes[1].runtime.payload_layout(), api);
        unsafe { (*api).context = core::ptr::null_mut(); (*api).set = None; (*api).slot_kind = None; }
        assert_eq!(runtimes[0].runtime.slot_kind(a, 1), 9);
        runtimes[0].runtime.set_layout(a, core::ptr::null(), 0, SLOT_OOP);
        assert_eq!(runtimes[0].runtime.slot_kind(a, usize::MAX), SLOT_OOP);
        assert_eq!(runtimes[1].runtime.slot_kind(b, 1), crate::abi::SLOT_CODE);
        let remaining = runtimes.pop().unwrap();
        drop(runtimes);
        remaining.runtime.enter(|| unsafe {
            let selected = remaining.runtime.payload_layout();
            birnam_abi::payload_layout::set(selected, b, [9].as_ptr(), 1, crate::abi::SLOT_RAW);
            assert_eq!(birnam_abi::payload_layout::slot_kind(selected, b, 0), 9);
        });
    }

    #[test]
    fn trace_hooks_follow_moved_runtimes_without_rooting_records_or_using_public_selection() {
        unsafe extern "C" fn first_hook(_: Oop, _: crate::abi::MarkFn) {}
        unsafe extern "C" fn second_hook(_: Oop, _: crate::abi::MarkFn) {}
        let first = build_bootstrap();
        let second = build_bootstrap();
        let a = first.runtime.create_vtable(first.runtime.globals().object_vt);
        let b = second.runtime.create_vtable(second.runtime.globals().object_vt);
        let dead = second.runtime.create_vtable(second.runtime.globals().object_vt);
        second.runtime.export(b"TraceHooksRetained", b).unwrap();
        first.runtime.set_trace_hook(a, first_hook);
        second.runtime.set_trace_hook(dead, first_hook);
        let api = first.runtime.trace_hooks();
        let mut runtimes = vec![first, second];
        assert_eq!(runtimes[0].runtime.trace_hooks(), api);
        assert_ne!(runtimes[1].runtime.trace_hooks(), api);
        unsafe { (*api).context = core::ptr::null_mut(); (*api).set = None; }
        let selected = |runtime: &super::Runtime, vt| unsafe {
            (*runtime.state.get()).vtables.trace_hook(vt).map(|hook| hook as usize)
        };
        assert_eq!(selected(&runtimes[0].runtime, a), Some(first_hook as *const () as usize));
        runtimes[0].runtime.set_trace_hook(a, second_hook);
        assert_eq!(selected(&runtimes[0].runtime, a), Some(second_hook as *const () as usize));
        let remaining = runtimes.pop().unwrap();
        drop(runtimes);
        remaining.runtime.enter(|| unsafe {
            birnam_abi::trace_hooks::set(remaining.runtime.trace_hooks(), b, second_hook);
            remaining.runtime.gc_collect_from_native_range(0, 0);
        });
        assert!(remaining.runtime.checked_vtable_of(dead).is_null());
        assert!(selected(&remaining.runtime, dead).is_none());
        assert_eq!(selected(&remaining.runtime, b), Some(second_hook as *const () as usize));
    }

    #[test]
    fn native_stack_anchor_follows_moved_runtimes_and_canonical_reads_ignore_public_selection() {
        let first = build_bootstrap();
        let second = build_bootstrap();
        let anchor = |runtime: &super::Runtime| unsafe { (*runtime.state.get()).metadata.stack_bottom() };
        assert_eq!(anchor(&first.runtime), 0);
        assert_eq!(anchor(&second.runtime), 0);
        let api = first.runtime.native_stack();
        first.runtime.enter(|| unsafe { birnam_abi::native_stack::set(api, 17) });
        second.runtime.gc_set_stack_bottom(91);
        let mut runtimes = vec![first, second];
        assert_eq!(runtimes[0].runtime.native_stack(), api);
        assert_ne!(runtimes[1].runtime.native_stack(), api);
        assert_eq!(anchor(&runtimes[0].runtime), 17);
        assert_eq!(anchor(&runtimes[1].runtime), 91);
        unsafe { (*api).context = core::ptr::null_mut(); (*api).set = None; }
        runtimes[0].runtime.gc_set_stack_bottom(0);
        assert_eq!(anchor(&runtimes[0].runtime), 0);
        assert_eq!(runtimes[0].runtime.gc_collect(), 0);
        assert!(!unsafe { (*runtimes[0].runtime.state.get()).metadata.automatic_collection_due(usize::MAX) });
        let remaining = runtimes.pop().unwrap();drop(runtimes);
        remaining.runtime.enter(|| unsafe {
            birnam_abi::native_stack::set(remaining.runtime.native_stack(), 0);
        });
        assert_eq!(anchor(&remaining.runtime), 0);
    }

    #[test]
    fn finalizers_follow_moved_runtimes_and_internal_machine_construction_is_canonical() {
        unsafe extern "C" fn forbidden(_: Oop) { panic!("Heap drop invoked finalizer"); }
        let first = build_bootstrap();
        let second = build_bootstrap();
        first.runtime.publish_canonical_exports();
        first.runtime.install_execution_objects();
        let a = first.runtime.allocate_instance(first.runtime.globals().object_vt, 0);
        let b = second.runtime.allocate_instance(second.runtime.globals().object_vt, 0);
        let api = first.runtime.finalizers();
        first.runtime.enter(|| unsafe { birnam_abi::finalizers::set(api, a, forbidden) });
        let mut runtimes = vec![first, second];
        assert_eq!(runtimes[0].runtime.finalizers(), api);
        assert_ne!(runtimes[1].runtime.finalizers(), api);
        unsafe { (*api).context = core::ptr::null_mut(); (*api).set = None; }
        let machine = runtimes[0].runtime.create_stackless_machine().unwrap();
        assert_eq!(runtimes[0].runtime.has_finalizer(machine), Some(true));
        runtimes[0].runtime.set_finalizer(a, forbidden);
        assert_eq!(runtimes[0].runtime.has_finalizer(a), Some(true));
        let remaining = runtimes.pop().unwrap();
        drop(runtimes);
        remaining.runtime.enter(|| unsafe {
            birnam_abi::finalizers::set(remaining.runtime.finalizers(), b, forbidden);
        });
        assert_eq!(remaining.runtime.has_finalizer(b), Some(true));
        // Runtime/Heap teardown releases storage without running callbacks.
    }

    #[test]
    fn finalizers_do_not_root_objects_and_later_dead_callbacks_remain_replaceable() {
        thread_local! {
            static OBJECTS: core::cell::RefCell<Vec<Oop>> = const { core::cell::RefCell::new(Vec::new()) };
            static EVENTS: core::cell::RefCell<Vec<(Oop, u8)>> = const { core::cell::RefCell::new(Vec::new()) };
        }
        unsafe extern "C" fn forbidden(_: Oop) { panic!("detached or cloned finalizer ran"); }
        unsafe extern "C" fn later(object: Oop) {
            EVENTS.with(|events| events.borrow_mut().push((object, 2)));
            crate::runtime::with_active(|runtime| assert_eq!(runtime.has_finalizer(object), None));
        }
        unsafe extern "C" fn first(object: Oop) {
            EVENTS.with(|events| events.borrow_mut().push((object, 1)));
            crate::runtime::with_active(|runtime| {
                assert_eq!(runtime.has_finalizer(object), None);
                unsafe { birnam_abi::finalizers::set(runtime.finalizers(), object, forbidden) };
                assert_eq!(runtime.has_finalizer(object), None);
                OBJECTS.with(|objects| {
                    for &other in objects.borrow().iter().filter(|&&other| other != object) {
                        assert_eq!(runtime.has_finalizer(other), Some(true));
                        unsafe { birnam_abi::finalizers::set(runtime.finalizers(), other, later) };
                    }
                });
                assert_eq!(runtime.gc_collect(), 0);
            });
        }
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let objects: Vec<_> = (0..3).map(|_| runtime.allocate_instance(runtime.globals().object_vt, 8)).collect();
        runtime.enter(|| {
            for &object in &objects {
                unsafe { birnam_abi::finalizers::set(runtime.finalizers(), object, first) };
            }
        });
        let clone = unsafe { (&mut *runtime.state.get()).heap.clone_collectable(objects[0]).unwrap() };
        assert_eq!(runtime.has_finalizer(clone), Some(false));
        OBJECTS.with(|saved| *saved.borrow_mut() = objects.clone());
        EVENTS.with(|events| events.borrow_mut().clear());
        runtime.enter(|| runtime.gc_collect_from_native_range(0, 0));
        EVENTS.with(|events| {
            let events = events.borrow();
            assert_eq!(events.iter().map(|(_, kind)| *kind).collect::<Vec<_>>(), [1, 2, 2]);
            let mut actual: Vec<_> = events.iter().map(|(object, _)| object.addr()).collect();
            let mut expected: Vec<_> = objects.iter().map(|object| object.addr()).collect();
            actual.sort_unstable(); expected.sort_unstable(); assert_eq!(actual, expected);
        });
        for object in objects.into_iter().chain([clone]) { assert_eq!(runtime.has_finalizer(object), None); }
        runtime.enter(|| runtime.gc_collect_from_native_range(0, 0));
        EVENTS.with(|events| assert_eq!(events.borrow().len(), 3));
        OBJECTS.with(|objects| objects.borrow_mut().clear());
    }

    #[test]
    fn root_registry_callback_append_is_visible_only_in_the_next_gc_snapshot() {
        thread_local! {
            static ORDER: core::cell::RefCell<Vec<usize>> = const { core::cell::RefCell::new(Vec::new()) };
            static APPENDED: Cell<bool> = const { Cell::new(false) };
        }
        unsafe extern "C" fn later(_: crate::abi::MarkFn) {
            ORDER.with(|order| order.borrow_mut().push(2));
        }
        unsafe extern "C" fn first(_: crate::abi::MarkFn) {
            ORDER.with(|order| order.borrow_mut().push(1));
            crate::runtime::with_active(|runtime| {
                // The existing collection guard rejects nested collection before callbacks.
                assert_eq!(runtime.gc_collect(), 0);
                if !APPENDED.replace(true) {
                    unsafe { birnam_abi::root_registry::register(runtime.root_registry(), later) };
                }
            });
        }
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.gc_register_root_provider(first);
        runtime.gc_register_root_provider(first);
        APPENDED.set(false);
        ORDER.with(|order| order.borrow_mut().clear());
        runtime.enter(|| runtime.gc_collect_from_native_range(0, 0));
        ORDER.with(|order| assert_eq!(*order.borrow(), [1, 1]));
        ORDER.with(|order| order.borrow_mut().clear());
        runtime.enter(|| runtime.gc_collect_from_native_range(0, 0));
        ORDER.with(|order| assert_eq!(*order.borrow(), [1, 1, 2]));
    }

    #[test]
    fn method_observation_follows_independent_moved_runtime_state() {
        thread_local! { static CALLS: Cell<usize> = const { Cell::new(0) }; }
        unsafe extern "C" fn observe(_: Oop, _: Oop) { CALLS.set(CALLS.get() + 1); }
        let first = build_bootstrap();
        let first_api = first.runtime.method_observation();
        first.runtime.enter(|| unsafe {
            birnam_abi::method_observation::replace(first_api, Some(observe));
        });
        let mut runtimes = Vec::with_capacity(1);
        runtimes.push(first);
        runtimes.push(build_bootstrap());
        assert_eq!(runtimes[0].runtime.method_observation(), first_api);
        assert_ne!(first_api, runtimes[1].runtime.method_observation());
        CALLS.set(0);
        for bootstrap in &runtimes {
            let runtime = &bootstrap.runtime;
            let vtable = runtime.create_vtable(runtime.globals().object_vt);
            let selector = runtime.intern(b"independent-observation").unwrap();
            runtime.add_method(vtable, selector, synchronous_identity);
            runtime.add_method(vtable, selector, synchronous_identity);
        }
        assert_eq!(CALLS.get(), 1);
        // Public context/function replacement does not affect Id's original
        // notification address or the service's destruction after another move.
        unsafe { (*first_api).context = core::ptr::null_mut(); (*first_api).replace = None; }
        let first = runtimes.remove(0);
        let selector = first.runtime.intern(b"afterMove").unwrap();
        let vtable = first.runtime.globals().object_vt;
        first.runtime.add_method(vtable, selector, synchronous_identity);
        first.runtime.remove_method(vtable, selector);
        assert_eq!(CALLS.get(), 2);
        drop(first);
        let second = runtimes.pop().unwrap();
        second.runtime.enter(|| unsafe {
            birnam_abi::method_observation::replace(second.runtime.method_observation(), Some(observe));
        });
        let selector = second.runtime.intern(b"afterOtherDrop").unwrap();
        let vtable = second.runtime.globals().object_vt;
        second.runtime.add_method(vtable, selector, synchronous_identity);
        second.runtime.remove_method(vtable, selector);
        assert_eq!(CALLS.get(), 3);
    }

    #[test]
    fn object_model_enumeration_observes_independent_moved_runtime_state() {
        // Each push moves the enclosing runtime; the service retains no view or
        // address into RuntimeState from its construction point.
        let mut runtimes = Vec::with_capacity(1);
        runtimes.push(build_bootstrap());
        runtimes.push(build_bootstrap());
        let mut identities = Vec::new();
        for bootstrap in &runtimes {
            let runtime = &bootstrap.runtime;
            runtime.enter(|| {
                let api = runtime.vtable_enumeration();
                let before = unsafe { birnam_abi::vtable_enumeration::count(api) };
                assert_eq!(before, runtime.vtable_count());
                let created = runtime.create_vtable(runtime.globals().object_vt);
                assert_eq!(unsafe { birnam_abi::vtable_enumeration::at(api, before) }, created);
                assert_eq!(unsafe { birnam_abi::vtable_enumeration::count(api) }, before + 1);
                assert!(unsafe { birnam_abi::vtable_enumeration::at(api, usize::MAX) }.is_null());
                identities.push((api, created));
            });
        }
        assert_ne!(identities[0].0, identities[1].0);
        assert_ne!(identities[0].1, identities[1].1);
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
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 101,
                generation: 17,
                source_map: 29,
            },
            program_counter_count: 2,
            driver_argument_count: 1,
            call_sites: Vec::new(),
            step_method: inert_step,
            edges: vec![],
            ..probe_spec(2, 2)
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
                runtime.intern(b"oop").expect("static symbol")
            );
            assert_eq!(runtime.send_slice(view, oop_at, &[zero]), selected);
            assert_eq!(
                runtime.send_slice(view, oop_at, &[runtime.make_integer(2)]),
                runtime.intern(b"invalidFrameSlot").expect("static symbol")
            );
        });

        let initial_roots = unsafe {
            let state = &mut *runtime.state.get();
            state.heap.clear_marks();
            [view]
        };
        runtime.enter(|| runtime.run_mark_phase((0, 0), &initial_roots));
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
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 2,
                generation: 1,
                source_map: 2,
            },
            program_counter_count: 1,
            driver_argument_count: 0,
            call_sites: Vec::new(),
            step_method: million_continue_step,
            edges: vec![probe_continue(0, 0, 1)],
            ..probe_spec(1, 1)
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
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 203,
                generation: 1,
                source_map: 203,
            },
            program_counter_count: 1,
            driver_argument_count: 0,
            call_sites: Vec::new(),
            step_method: recursive_drive_probe_step,
            edges: vec![probe_continue(0, 0, 1)],
            ..probe_spec(1, 1)
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

        let continue_descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 201,
                generation: 1,
                source_map: 201,
            },
            program_counter_count: 1,
            driver_argument_count: 0,
            call_sites: Vec::new(),
            step_method: measured_continue_step,
            edges: vec![probe_continue(0, 0, 1)],
            ..probe_spec(1, 1)
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
        let synchronous_descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 202,
                generation: 1,
                source_map: 202,
            },
            program_counter_count: 2,
            driver_argument_count: 1,
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
            edges: vec![probe_call(0, 1, 0, 3, 4), probe_continue(1, 0, 4)],
            ..probe_spec(4, 2)
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
    #[ignore = "Release-mode S2 publication probe; run with BIRNAM_S_NS from the same micro suite"]
    fn publish_stackless_s2_entry_and_call_measurements() {
        fn median_ns(mut samples: Vec<f64>) -> f64 {
            samples.sort_by(f64::total_cmp);
            samples[samples.len() / 2]
        }

        const CALL_ITERATIONS: usize = 200_000;
        const ENTRY_ITERATIONS: usize = 20_000;
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
        MEASURE_CHILD_VALUE_SLOT.set(runtime.make_integer(1));
        PROBE_STACK_MIN.set(usize::MAX);
        PROBE_STACK_MAX.set(0);
        REGISTERED_THUNK_CALLS.set(0);
        REGISTERED_FRAME_PIN_HIGH_WATER.set(0);

        let migrated_selector = runtime
            .intern(b"measuredRegisteredIdentity:")
            .expect("static selector");
        let receiver = runtime.canonical().object_prototype;
        let argument = runtime.make_integer(41);
        let child = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 204,
                generation: 1,
                source_map: 204,
            },
            program_counter_count: 1,
            driver_argument_count: 0,
            call_sites: Vec::new(),
            step_method: measured_registered_child_step,
            entry_argument_count: 1,
            edges: vec![],
            ..probe_spec(2, 1)
        })
        .expect("measurement child descriptor validates");
        runtime
            .register_stackless_resumable(SemanticRegistrationV1::new(
                registered_probe_thunk,
                child,
                1,
                0,
                1,
            ))
            .expect("measurement child registers");
        let _ = runtime.add_method(
            runtime.globals().object_vt,
            migrated_selector,
            registered_probe_thunk,
        );

        let parent = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 205,
                generation: 1,
                source_map: 205,
            },
            program_counter_count: 2,
            driver_argument_count: 1,
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
            edges: vec![probe_call(0, 1, 0, 3, 4), probe_continue(1, 0, 4)],
            ..probe_spec(4, 2)
        })
        .expect("measurement caller descriptor validates");

        let mut call_samples = Vec::with_capacity(SAMPLES);
        let mut entry_samples = Vec::with_capacity(SAMPLES);
        let mut fallback_samples = Vec::with_capacity(SAMPLES);
        let mut high_water_live_frames = 0;
        let mut high_water_boundary_depth = 0;

        for _ in 0..SAMPLES {
            let machine = runtime
                .create_stackless_machine()
                .expect("Call/Return measurement Machine creates");
            MEASURE_LIMIT.set(CALL_ITERATIONS);
            MEASURE_COUNT.set(0);
            let started = std::time::Instant::now();
            let answer = runtime
                .drive_stackless(
                    machine,
                    parent.clone(),
                    &[(0, receiver), (1, migrated_selector), (2, argument)],
                )
                .expect("migrated Call/Return measurement completes");
            let elapsed = started.elapsed();
            std::hint::black_box(answer);
            assert_eq!(answer, argument);
            assert_eq!(MEASURE_COUNT.get(), CALL_ITERATIONS);
            call_samples.push(elapsed.as_nanos() as f64 / CALL_ITERATIONS as f64);
            let capability = runtime
                .stackless_machine_capability(machine)
                .expect("measurement Machine remains inspectable");
            let diagnostics = unsafe {
                (*runtime.state.get())
                    .execution
                    .machine_diagnostics(current_thread_token(), capability)
                    .expect("measurement diagnostics resolve")
            };
            high_water_live_frames = high_water_live_frames.max(diagnostics.high_water_live_frames);
            high_water_boundary_depth =
                high_water_boundary_depth.max(diagnostics.high_water_synchronous_boundary_depth);
            runtime
                .destroy_stackless_machine(machine)
                .expect("measurement Machine destroys");

            let before_direct = REGISTERED_THUNK_CALLS.get();
            let started = std::time::Instant::now();
            runtime.enter(|| {
                for _ in 0..ENTRY_ITERATIONS {
                    let answer = runtime.send_slice(receiver, migrated_selector, &[argument]);
                    std::hint::black_box(answer);
                    assert_eq!(answer, argument);
                }
            });
            let elapsed = started.elapsed();
            assert_eq!(
                REGISTERED_THUNK_CALLS.get(),
                before_direct,
                "ordinary entry bypasses the installed thunk"
            );
            entry_samples.push(elapsed.as_nanos() as f64 / ENTRY_ITERATIONS as f64);

            let fallback_arguments = [argument];
            let before_fallback = REGISTERED_THUNK_CALLS.get();
            let started = std::time::Instant::now();
            runtime.enter(|| {
                for _ in 0..ENTRY_ITERATIONS {
                    let answer = unsafe {
                        registered_probe_thunk(
                            receiver,
                            fallback_arguments.as_ptr(),
                            fallback_arguments.len(),
                        )
                    };
                    std::hint::black_box(answer);
                    assert_eq!(answer, argument);
                }
            });
            let elapsed = started.elapsed();
            assert_eq!(
                REGISTERED_THUNK_CALLS.get() - before_fallback,
                ENTRY_ITERATIONS,
                "fallback measurement invokes the exact installed thunk"
            );
            fallback_samples.push(elapsed.as_nanos() as f64 / ENTRY_ITERATIONS as f64);
        }

        let baseline = std::env::var("BIRNAM_S_NS")
            .expect("set BIRNAM_S_NS to same-run sends.compiled-unary ns/op")
            .parse::<f64>()
            .expect("BIRNAM_S_NS is numeric");
        let call_ns = median_ns(call_samples);
        let entry_ns = median_ns(entry_samples);
        let fallback_ns = median_ns(fallback_samples);
        let call_ratio = call_ns / baseline;
        let entry_ratio = entry_ns / baseline;
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../abi/fixtures/stackless-s2/frame-descriptor-call.bin");
        let canonical_descriptor_bytes = std::fs::metadata(fixture)
            .expect("canonical Call descriptor fixture resolves")
            .len();
        let runtime_descriptor_bytes = parent.owned_byte_size();
        let stack_movement_bytes = PROBE_STACK_MAX.get().saturating_sub(PROBE_STACK_MIN.get());
        assert_eq!(REGISTERED_FRAME_PIN_HIGH_WATER.get(), 1);
        assert_eq!(runtime.stackless_runtime_diagnostics().live_machines, 0);
        assert_eq!(
            runtime
                .stackless_registration_diagnostics()
                .active_frame_pins,
            0
        );
        assert_eq!(high_water_live_frames, 2);
        assert_eq!(high_water_boundary_depth, 0);
        assert!(stack_movement_bytes <= 4096);
        assert!(call_ratio <= 4.0, "migrated Call/Return exceeds 4 x S");
        assert!(entry_ratio <= 6.0, "ordinary resumable entry exceeds 6 x S");
        println!(
            "STACKLESS_S2_MEASUREMENTS baseline_s_ns={baseline:.3} call_return_ns={call_ns:.3} call_return_ratio={call_ratio:.3} ordinary_entry_ns={entry_ns:.3} ordinary_entry_ratio={entry_ratio:.3} fallback_thunk_ns={fallback_ns:.3} thunk_calls_direct=0 high_water_frames={high_water_live_frames} high_water_boundary_depth={high_water_boundary_depth} native_stack_movement_bytes={stack_movement_bytes} canonical_call_descriptor_bytes={canonical_descriptor_bytes} runtime_call_descriptor_bytes={runtime_descriptor_bytes} microcode_sends_call=1 microcode_sends_child_return=1 microcode_sends_continue=1 microcode_sends_parent_return=1 call_iterations={CALL_ITERATIONS} entry_iterations={ENTRY_ITERATIONS} samples={SAMPLES}"
        );
    }

    #[test]
    #[ignore = "Release-mode S2 native-depth publication probe"]
    fn publish_stackless_s2_depth_measurement() {
        const MODE_ENV: &str = "BIRNAM_STACKLESS_S2_DEPTH_MODE";
        const DEPTH_ENV: &str = "BIRNAM_STACKLESS_S2_DEPTH";
        const THREAD_STACK_BYTES: usize = 2 * 1024 * 1024;

        if let (Ok(mode), Ok(depth)) = (std::env::var(MODE_ENV), std::env::var(DEPTH_ENV)) {
            let depth = depth.parse::<i64>().expect("depth probe value is numeric");
            std::thread::Builder::new()
                .name("stackless-s2-depth-child".to_owned())
                .stack_size(THREAD_STACK_BYTES)
                .spawn(move || {
                    let bootstrap = build_bootstrap();
                    let runtime = &bootstrap.runtime;
                    runtime.publish_canonical_exports();
                    runtime.install_execution_objects();
                    let receiver = runtime.canonical().object_prototype;
                    let argument = runtime.make_integer(depth);
                    if mode == "baseline" {
                        let selector = runtime
                            .intern(b"measuredBaselineDepth:")
                            .expect("static selector");
                        DEPTH_BASELINE_SELECTOR.set(selector);
                        let _ = runtime.add_method(
                            runtime.globals().object_vt,
                            selector,
                            depth_baseline_method,
                        );
                        let answer =
                            runtime.enter(|| runtime.send_slice(receiver, selector, &[argument]));
                        assert_eq!(runtime.integer_value(answer), 0);
                        return;
                    }
                    assert_eq!(mode, "mixed");
                    CALL_SITE_PROBE_SELECTOR
                        .set(runtime.intern(b"callSite:").expect("static selector"));
                    RETURN_PROBE_SELECTOR
                        .set(runtime.intern(b"returnAt:").expect("static selector"));
                    let synchronous_selector = runtime
                        .intern(b"measuredDepthBarrier:")
                        .expect("static selector");
                    let migrated_selector = runtime
                        .intern(b"measuredMigratedDepth:")
                        .expect("static selector");
                    DEPTH_MIGRATED_SELECTOR.set(migrated_selector);
                    DEPTH_SYNCHRONOUS_SELECTOR.set(synchronous_selector);
                    let _ = runtime.add_method(
                        runtime.globals().object_vt,
                        synchronous_selector,
                        depth_synchronous_barrier_method,
                    );
                    let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
                        identity: DescriptorIdentity {
                            implementation: 206,
                            generation: 1,
                            source_map: 206,
                        },
                        program_counter_count: 2,
                        driver_argument_count: 1,
                        call_sites: vec![CallSiteDescriptor {
                            kind: CallSiteKind::Call,
                            receiver_slot: 0,
                            selector_slot: 1,
                            argument_start: 2,
                            argument_count: 1,
                            result_slot: Some(3),
                            continuation_pc: Some(1),
                        }],
                        step_method: depth_migrated_step,
                        entry_argument_count: 1,
                        edges: vec![probe_call(0, 1, 0, 3, 4)],
                        ..probe_spec(4, 2)
                    })
                    .expect("mixed-depth descriptor validates");
                    runtime
                        .register_stackless_resumable(SemanticRegistrationV1::new(
                            registered_probe_thunk,
                            descriptor,
                            1,
                            0,
                            2,
                        ))
                        .expect("mixed-depth entry registers");
                    let _ = runtime.add_method(
                        runtime.globals().object_vt,
                        migrated_selector,
                        registered_probe_thunk,
                    );
                    let answer = runtime
                        .enter(|| runtime.send_slice(receiver, migrated_selector, &[argument]));
                    assert_eq!(runtime.integer_value(answer), 0);
                })
                .expect("depth probe thread starts")
                .join()
                .expect("depth probe thread completes");
            return;
        }

        fn survives(mode: &str, depth: usize) -> bool {
            std::process::Command::new(
                std::env::current_exe().expect("current test executable resolves"),
            )
            .args([
                "--exact",
                "runtime::tests::publish_stackless_s2_depth_measurement",
                "--ignored",
            ])
            .env("BIRNAM_STACKLESS_S2_DEPTH_MODE", mode)
            .env("BIRNAM_STACKLESS_S2_DEPTH", depth.to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("depth child process starts")
            .success()
        }

        fn maximum_depth(mode: &str) -> usize {
            assert!(survives(mode, 8), "shallow depth probe must succeed");
            let mut low = 8usize;
            let mut high = 16usize;
            while high < 65_536 && survives(mode, high) {
                low = high;
                high *= 2;
            }
            assert!(high < 65_536, "depth probe needs a finite upper bound");
            while low + 1 < high {
                let middle = low + (high - low) / 2;
                if survives(mode, middle) {
                    low = middle;
                } else {
                    high = middle;
                }
            }
            low
        }

        let baseline_depth = maximum_depth("baseline");
        let mixed_depth = maximum_depth("mixed");
        let depth_ratio = mixed_depth as f64 / baseline_depth as f64;
        println!(
            "STACKLESS_S2_DEPTH baseline_depth={baseline_depth} mixed_depth={mixed_depth} depth_ratio={depth_ratio:.3} thread_stack_bytes={THREAD_STACK_BYTES}"
        );
        assert!(
            depth_ratio >= 0.5,
            "mixed resumable/synchronous depth is below the provisional 0.5 baseline budget"
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
        let child = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 12,
                generation: 1,
                source_map: 12,
            },
            program_counter_count: 1,
            driver_argument_count: 0,
            call_sites: Vec::new(),
            step_method: call_child_step,
            entry_argument_count: 1,
            edges: vec![],
            ..probe_spec(2, 1)
        })
        .expect("child descriptor validates");
        let parent = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 11,
                generation: 1,
                source_map: 11,
            },
            program_counter_count: 3,
            driver_argument_count: 1,
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
            edges: vec![probe_call(0, 1, 0, 3, 6), probe_call(1, 2, 1, 4, 6)],
            ..probe_spec(6, 3)
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
                        Ok(StacklessTarget::Synchronous(crate::legacy_method::encode(synchronous_increment, false)))
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
    fn typed_entries_preserve_context_through_cache_effectful_lookup_and_machine_calls() {
        use super::{with_active, ScopedOopRootGuard};
        use crate::abi::id_protocol::{NativeEntry, Operand, Outcome};
        struct Context { answer: Oop, calls: Cell<usize> }
        unsafe extern "C" fn answer(context: *mut core::ffi::c_void, _: Oop, args: *const Operand, count: usize) -> Outcome {
            assert_eq!(count, 1);
            assert!(unsafe { args.read() }.as_object().is_some());
            let context = unsafe { &*context.cast::<Context>() };
            context.calls.set(context.calls.get() + 1);
            Outcome::loaded(Operand::object(context.answer))
        }
        struct Lookup { selector: Oop, parent: Oop, replacement: NativeEntry, calls: Cell<usize>, reject: Cell<bool> }
        unsafe extern "C" fn replacing_lookup(context: *mut core::ffi::c_void, receiver: Oop, args: *const Operand, count: usize) -> Outcome {
            assert_eq!(count, 1);
            let selector = unsafe { args.read() }.as_object().unwrap();
            let context = unsafe { &*context.cast::<Lookup>() };
            with_active(|runtime| {
                let selected = unsafe { runtime.lookup_native_method_active(receiver, selector) };
                if selector == context.selector {
                    context.calls.set(context.calls.get() + 1);
                    if context.reject.get() { return Outcome::failed(crate::abi::id_protocol::INVALID_REQUEST, 83); }
                    runtime.add_native_method(context.parent, selector, context.replacement);
                }
                Outcome::loaded(selected.map(Operand::entry).unwrap_or_else(Operand::unit))
            })
        }
        unsafe extern "C" fn tail(machine: Oop, _: *const Oop, _: usize) -> Oop {
            with_active(|runtime| {
                let selector = runtime.intern(b"tailCallSite:").unwrap();
                let site = runtime.make_integer(0);
                runtime.send_slice(machine, selector, &[site])
            })
        }
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        let parent = runtime.create_vtable(runtime.globals().object_vt);
        let child = runtime.create_vtable(parent);
        let receiver = runtime.allocate_instance(child, 0);
        let _root = ScopedOopRootGuard::new(runtime, receiver);
        let selector = runtime.intern(b"typedAnswer:").unwrap();
        let mut first = Context { answer: receiver, calls: Cell::new(0) };
        let mut second = Context { answer: runtime.canonical().nil, calls: Cell::new(0) };
        let first_entry = NativeEntry::new(answer, core::ptr::from_mut(&mut first).cast());
        let second_entry = NativeEntry::new(answer, core::ptr::from_mut(&mut second).cast());
        runtime.add_native_method(parent, selector, first_entry);
        super::dispatch_cache::HITS.set(0);
        for _ in 0..8 { assert_eq!(runtime.send_slice(receiver, selector, &[receiver]), receiver); }
        assert_eq!(super::dispatch_cache::HITS.get(), 7);
        // The inherited entry was cached, including its context.
        assert_eq!(runtime.enter_application(|| {
            assert_eq!(runtime.cached_default_lookup(child, selector), Some(first_entry));
            receiver
        }), receiver);
        runtime.add_native_method(parent, selector, second_entry);
        assert_eq!(runtime.send_slice(receiver, selector, &[receiver]), runtime.canonical().nil);
        assert_eq!((first.calls.get(), second.calls.get()), (8, 1));
        let mut lookup = Lookup { selector, parent, replacement: second_entry, calls: Cell::new(0), reject: Cell::new(false) };
        runtime.add_native_method(runtime.globals().vtable_vt, runtime.globals().lookup,
            NativeEntry::new(replacing_lookup, core::ptr::from_mut(&mut lookup).cast()));
        runtime.add_native_method(parent, selector, first_entry);
        assert_eq!(runtime.send_slice(receiver, selector, &[receiver]), receiver);
        assert_eq!(lookup.calls.get(), 1);
        assert_eq!(runtime.send_slice(receiver, selector, &[receiver]), runtime.canonical().nil);
        CALL_SITE_PROBE_SELECTOR.set(runtime.intern(b"callSite:").unwrap());
        RETURN_PROBE_SELECTOR.set(runtime.intern(b"returnAt:").unwrap());
        let kind_selector = runtime.intern(b"kind:").unwrap();
        export_zero_payload_instance_with_method(runtime, b"Error", kind_selector, test_error_kind_factory);
        let expected_error = runtime.intern(b"nativeInvalidRequest").unwrap();
        let _error_root = ScopedOopRootGuard::new(runtime, expected_error);
        for tail_call in [false, true] {
            runtime.add_native_method(parent, selector, first_entry);
            lookup.calls.set(0);
            let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
                identity: DescriptorIdentity { implementation: 830, generation: 1, source_map: 830 },
                program_counter_count: if tail_call { 1 } else { 2 },
                driver_argument_count: if tail_call { 0 } else { 1 },
                call_sites: vec![CallSiteDescriptor {
                    kind: if tail_call { CallSiteKind::TailCall } else { CallSiteKind::Call },
                    receiver_slot: 0, selector_slot: 1, argument_start: 2, argument_count: 1,
                    result_slot: (!tail_call).then_some(3), continuation_pc: (!tail_call).then_some(1),
                }],
                step_method: if tail_call { tail } else { single_call_parent_step },
                edges: if tail_call { vec![] } else { vec![probe_call(0, 1, 0, 3, 4)] },
                ..probe_spec(4, if tail_call { 1 } else { 2 })
            }).unwrap();
            let machine = runtime.create_stackless_machine().unwrap();
            assert_eq!(runtime.drive_stackless(machine, descriptor.clone(), &[(0, receiver), (1, selector), (2, receiver)]), Ok(receiver));
            assert_eq!(lookup.calls.get(), 1, "Machine invokes the entry selected before replacement exactly once");
            assert_eq!(runtime.send_slice(receiver, selector, &[receiver]), runtime.canonical().nil);
            lookup.reject.set(true);
            lookup.calls.set(0);
            assert_eq!(runtime.send_slice(receiver, selector, &[receiver]), expected_error);
            assert_eq!(lookup.calls.get(), 1);
            let machine = runtime.create_stackless_machine().unwrap();
            lookup.calls.set(0);
            assert_eq!(runtime.drive_stackless(machine, descriptor, &[(0, receiver), (1, selector), (2, receiver)]), Ok(expected_error));
            assert_eq!(lookup.calls.get(), 1, "lookup failure becomes a language Error in both Machine call forms");
            lookup.reject.set(false);
        }
    }

    #[test]
    fn native_and_machine_missing_sends_each_perform_the_original_lookup_once() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        CALL_SITE_PROBE_SELECTOR.set(runtime.intern(b"callSite:").unwrap());
        RETURN_PROBE_SELECTOR.set(runtime.intern(b"returnAt:").unwrap());
        let selector = runtime.intern(b"missingOnce:").unwrap();
        CUSTOM_LOOKUP_SELECTOR.set(selector);
        CUSTOM_LOOKUP_RESULT.set(core::ptr::null_mut());
        runtime.add_method(runtime.globals().vtable_vt, runtime.globals().lookup, late_bound_custom_lookup);
        runtime.add_method(runtime.globals().object_vt, runtime.globals().does_not_understand, synchronous_second_argument);
        let receiver = runtime.canonical().object_prototype;
        let expected = runtime.make_integer(83);
        CUSTOM_LOOKUP_CALLS.set(0);
        assert_eq!(runtime.send_slice(receiver, selector, &[expected]), expected);
        assert_eq!(CUSTOM_LOOKUP_CALLS.get(), 1);

        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity { implementation: 83, generation: 1, source_map: 83 },
            program_counter_count: 2,
            driver_argument_count: 1,
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
            edges: vec![probe_call(0, 1, 0, 3, 4)],
            ..probe_spec(4, 2)
        }).unwrap();
        let machine = runtime.create_stackless_machine().unwrap();
        CUSTOM_LOOKUP_CALLS.set(0);
        assert_eq!(runtime.drive_stackless(machine, descriptor, &[(0, receiver), (1, selector), (2, expected)]), Ok(expected));
        assert_eq!(CUSTOM_LOOKUP_CALLS.get(), 1, "Machine must continue to DNU without retrying the original lookup");
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
        let nested = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 32,
                generation: 1,
                source_map: 32,
            },
            program_counter_count: 1,
            driver_argument_count: 0,
            call_sites: Vec::new(),
            step_method: call_child_step,
            entry_argument_count: 1,
            edges: vec![],
            ..probe_spec(2, 1)
        })
        .expect("nested descriptor validates");
        let outer = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 31,
                generation: 1,
                source_map: 31,
            },
            program_counter_count: 2,
            driver_argument_count: 1,
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
            edges: vec![probe_call(0, 1, 0, 3, 4)],
            ..probe_spec(4, 2)
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
                    Ok(StacklessTarget::Synchronous(crate::legacy_method::encode(synchronous_nested_entry, false)))
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

        let nested = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 34,
                generation: 2,
                source_map: 34,
            },
            program_counter_count: 1,
            driver_argument_count: 0,
            call_sites: Vec::new(),
            step_method: suppressed_entry_step,
            entry_argument_count: 1,
            edges: vec![],
            ..probe_spec(2, 1)
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

        let outer = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 33,
                generation: 2,
                source_map: 33,
            },
            program_counter_count: 2,
            driver_argument_count: 1,
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
            edges: vec![probe_call(0, 1, 0, 3, 4)],
            ..probe_spec(4, 2)
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
                    Ok(StacklessTarget::Synchronous(crate::legacy_method::encode(synchronous_suppressed_entry, false)))
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
    fn generated_adapter_invokes_the_selected_entry_after_binding_replacement() {
        use birnam_native_compiler::Unit;
        use core::ffi::c_void;
        use std::ffi::CString;

        // Compile the production adapter with no send/intern imports. An adapter
        // that silently redoes lookup cannot even link against this contract.
        let mut source = birnam_codegen::resumable_c::emit_entry_thunk(83, b"selected:");
        source.extend_from_slice(b"void* entry_address(void){return (void*)&__birnam_resumable_thunk_83;}\n");
        let source = CString::new(source).unwrap();
        let unit = unsafe {
            Unit::compile(&source, &[(
                c"birnam_enter_resumable_v1",
                crate::registration::birnam_enter_resumable_v1 as *const c_void,
            )])
        }.expect("production adapter links through exact entry only");
        let address: unsafe extern "C" fn() -> *mut c_void = unsafe {
            core::mem::transmute(unit.symbol(c"entry_address").unwrap().as_ptr())
        };
        let method: Method = unsafe { core::mem::transmute(address()) };
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        RETURN_PROBE_SELECTOR.set(runtime.intern(b"returnAt:").unwrap());
        CALL_PROBE_FORCED_GC.set(false);
        let selector = runtime.intern(b"selected:").unwrap();
        let receiver = runtime.canonical().object_prototype;
        let expected = runtime.make_integer(83);
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity { implementation: 83, generation: 1, source_map: 83 },
            program_counter_count: 1,
            driver_argument_count: 0,
            call_sites: Vec::new(),
            step_method: call_child_step,
            entry_argument_count: 1,
            edges: vec![],
            ..probe_spec(2, 1)
        }).unwrap();
        runtime.register_stackless_resumable(SemanticRegistrationV1::new(
            method, descriptor, 1, 0, 1,
        )).unwrap();
        runtime.add_method(runtime.globals().object_vt, selector, method);
        unsafe extern "C" fn replacement(_: Oop, _: *const Oop, _: usize) -> Oop {
            crate::runtime::with_active(|runtime| runtime.canonical().nil)
        }
        runtime.enter(|| {
            let _read = runtime.enter_managed_turn();
            let selected = unsafe { runtime.bind_active(receiver, selector) }.unwrap_or_else(|_| panic!("lookup failed")).unwrap();
            runtime.add_method(runtime.globals().object_vt, selector, replacement);
            let args = [expected];
            let answer = runtime.invoke_native_from_objects(selected, receiver, &args);
            assert_eq!(answer, expected, "the captured adapter retains the exact selected method");
            assert_eq!(runtime.send_slice(receiver, selector, &args), runtime.canonical().nil,
                "the next send observes replacement");
        });
    }

    #[test]
    fn registered_ordinary_send_bypasses_thunk_and_direct_fallback_still_works() {
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
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 41,
                generation: 9,
                source_map: 41,
            },
            program_counter_count: 1,
            driver_argument_count: 0,
            call_sites: Vec::new(),
            step_method: call_child_step,
            entry_argument_count: 1,
            edges: vec![],
            ..probe_spec(2, 1)
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
        assert_eq!(
            REGISTERED_THUNK_CALLS.get(),
            0,
            "ordinary send resolves and pins without invoking the carrier thunk"
        );
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
            "ordinary caller never observes the resolver's fresh Machine"
        );

        let fallback_arguments = [expected];
        let fallback = runtime.enter(|| unsafe {
            registered_probe_thunk(
                receiver,
                fallback_arguments.as_ptr(),
                fallback_arguments.len(),
            )
        });
        assert_eq!(fallback, expected);
        assert_eq!(
            REGISTERED_THUNK_CALLS.get(),
            1,
            "direct raw Method invocation remains a working fallback"
        );

        let parent = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 42,
                generation: 9,
                source_map: 42,
            },
            program_counter_count: 2,
            driver_argument_count: 1,
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
            edges: vec![probe_call(0, 1, 0, 3, 4)],
            ..probe_spec(4, 2)
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
    fn empty_staged_units_do_not_pin_a_reusable_mapping_identity() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let manifest = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../abi/fixtures/stackless-s2/resumable-manifest-empty.bin"),
        )
        .unwrap();
        let authority: [u8; 32] = manifest[0xc0..0xe0].try_into().unwrap();

        assert!(runtime.begin_staged_resumable_unit(&manifest, authority, 101, 103));
        assert!(runtime.finish_staged_resumable_unit());
        assert!(
            runtime.begin_staged_resumable_unit(&manifest, authority, 101, 107),
            "a zero-entry unit retains no code or registration identity"
        );
        assert!(runtime.finish_staged_resumable_unit());
    }

    #[test]
    fn committed_registration_owns_metadata_and_invalidates_initializer_abort() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let (manifest, authority, descriptor, mut envelope) = frozen_registration_envelope(
            "resumable-manifest-block-only.bin",
            "frame-descriptor-source-map-block.bin",
            core::ptr::null_mut(),
        );
        assert!(runtime.begin_staged_resumable_unit(&manifest, authority, 101, 103));
        let status = runtime
            .enter(|| unsafe { birnam_register_resumable_v1(core::ptr::addr_of_mut!(envelope)) });
        assert_eq!(status, crate::registration::OK_V1);
        assert_ne!(envelope.registration_token, 0);
        assert!(runtime.finish_staged_resumable_unit());
        drop(descriptor);
        assert_eq!(
            runtime
                .stackless_registration_diagnostics()
                .registered_entries,
            1,
            "Runtime owns admitted metadata after artifact bytes disappear"
        );

        let token = envelope.registration_token;
        envelope = BirnamResumableRegistrationV1 {
            carrier_version: CARRIER_VERSION_V1,
            struct_size: core::mem::size_of::<BirnamResumableRegistrationV1>() as u32,
            operation: ABORT_V1,
            flags: 0,
            manifest_entry_index: 0,
            reserved0: 0,
            registration_token: token,
            descriptor_bytes: core::ptr::null(),
            descriptor_length: 0,
            entry_thunk: None,
            step_method: None,
            implementation_oop: core::ptr::null_mut(),
            code_generation_identity: 0,
            code_generation_lease: 0,
            portable_implementation_identity: 0,
            language_arity: 0,
            driver_argument_count: 0,
            descriptor_sha256: [0; 32],
        };
        let status = runtime
            .enter(|| unsafe { birnam_register_resumable_v1(core::ptr::addr_of_mut!(envelope)) });
        assert_eq!(status, crate::registration::REJECTED_V1);
        assert_eq!(
            runtime
                .stackless_registration_diagnostics()
                .registered_entries,
            1,
            "manifest commit replaces the initializer-local Abort token"
        );
    }

    #[test]
    fn explicit_staged_unit_cancel_rolls_back_even_a_complete_manifest() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let (manifest, authority, _descriptor, mut envelope) = frozen_registration_envelope(
            "resumable-manifest-block-only.bin",
            "frame-descriptor-source-map-block.bin",
            core::ptr::null_mut(),
        );
        assert!(runtime.begin_staged_resumable_unit(&manifest, authority, 101, 103));
        assert_eq!(
            runtime.enter(|| unsafe {
                birnam_register_resumable_v1(core::ptr::addr_of_mut!(envelope))
            }),
            crate::registration::OK_V1
        );
        assert_eq!(
            runtime.enter(|| birnam_id_cancel_resumable_unit_v1()),
            crate::registration::OK_V1
        );
        assert!(!runtime.stackless_resumable_manifest_matches(authority, 101, &manifest));
        assert_eq!(
            runtime
                .stackless_registration_diagnostics()
                .registered_entries,
            0
        );
        assert!(runtime.begin_staged_resumable_unit(&manifest, authority, 101, 103));
        assert!(runtime.cancel_staged_resumable_unit());
    }

    #[test]
    fn named_manifest_commit_roots_the_implementation_and_invalidates_abort() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let implementation = runtime.allocate_instance(runtime.globals().object_vt, 0);
        let (manifest, authority, _descriptor, mut envelope) = frozen_registration_envelope(
            "resumable-manifest-named-only.bin",
            "frame-descriptor-minimal-return.bin",
            implementation,
        );
        assert!(runtime.begin_staged_resumable_unit(&manifest, authority, 101, 103));
        assert_eq!(
            runtime.enter(|| unsafe {
                birnam_register_resumable_v1(core::ptr::addr_of_mut!(envelope))
            }),
            crate::registration::OK_V1
        );
        assert!(runtime.finish_staged_resumable_unit());
        runtime.gc_mark_floor_roots_for_test();
        assert_eq!(runtime.is_marked_for_test(implementation), Some(true));

        let token = envelope.registration_token;
        envelope = BirnamResumableRegistrationV1 {
            carrier_version: CARRIER_VERSION_V1,
            struct_size: core::mem::size_of::<BirnamResumableRegistrationV1>() as u32,
            operation: ABORT_V1,
            flags: 0,
            manifest_entry_index: 0,
            reserved0: 0,
            registration_token: token,
            descriptor_bytes: core::ptr::null(),
            descriptor_length: 0,
            entry_thunk: None,
            step_method: None,
            implementation_oop: core::ptr::null_mut(),
            code_generation_identity: 0,
            code_generation_lease: 0,
            portable_implementation_identity: 0,
            language_arity: 0,
            driver_argument_count: 0,
            descriptor_sha256: [0; 32],
        };
        assert_eq!(
            runtime.enter(|| unsafe {
                birnam_register_resumable_v1(core::ptr::addr_of_mut!(envelope))
            }),
            crate::registration::REJECTED_V1
        );
        runtime.gc_mark_floor_roots_for_test();
        assert_eq!(runtime.is_marked_for_test(implementation), Some(true));
    }

    #[test]
    fn committed_block_catalog_is_bidirectional_rooted_and_fails_on_binding_drift() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let (manifest, authority, _descriptor, mut envelope) = frozen_registration_envelope(
            "resumable-manifest-block-only.bin",
            "frame-descriptor-source-map-block.bin",
            core::ptr::null_mut(),
        );
        let thunk = envelope.entry_thunk.expect("fixture has an entry thunk");
        assert!(runtime.begin_staged_resumable_unit(&manifest, authority, 101, 103));
        assert_eq!(
            runtime.enter(|| unsafe {
                birnam_register_resumable_v1(core::ptr::addr_of_mut!(envelope))
            }),
            crate::registration::OK_V1
        );

        let vtable = runtime.create_vtable(runtime.globals().object_vt);
        let selector = runtime.intern(b"value:").expect("static selector");
        let _ = runtime.add_method(vtable, selector, thunk);
        assert_eq!(
            runtime.stackless_block_vtable_image_reference(vtable, authority),
            None,
            "staged bindings are not image authority before manifest commit"
        );
        assert!(runtime.finish_staged_resumable_unit());
        assert!(runtime.stackless_resumable_manifest_matches(authority, 101, &manifest));
        let mut wrong_manifest = manifest.clone();
        wrong_manifest[0] ^= 1;
        assert!(!runtime.stackless_resumable_manifest_matches(authority, 101, &wrong_manifest));
        assert_eq!(
            runtime.stackless_block_vtable_image_reference(vtable, authority),
            Some(0)
        );
        assert_eq!(
            runtime.stackless_block_vtable_for_manifest_index(authority, 0),
            Some(vtable)
        );
        let mut image_index = u32::MAX;
        assert_eq!(
            runtime.enter(|| unsafe {
                birnam_id_stackless_block_vtable_image_reference_v1(
                    authority.as_ptr(),
                    authority.len(),
                    vtable,
                    core::ptr::addr_of_mut!(image_index),
                )
            }),
            1
        );
        assert_eq!(image_index, 0);
        assert_eq!(
            runtime.enter(|| unsafe {
                birnam_id_stackless_block_vtable_matches_v1(
                    authority.as_ptr(),
                    authority.len(),
                    image_index,
                    vtable,
                )
            }),
            1
        );
        assert_eq!(
            runtime.enter(|| unsafe {
                birnam_id_stackless_resumable_manifest_matches_v1(
                    authority.as_ptr(),
                    authority.len(),
                    101,
                    manifest.as_ptr(),
                    manifest.len(),
                )
            }),
            1
        );
        assert_eq!(
            runtime.enter(|| unsafe {
                birnam_id_stackless_block_vtable_image_reference_v1(
                    authority.as_ptr(),
                    authority.len() - 1,
                    vtable,
                    core::ptr::addr_of_mut!(image_index),
                )
            }),
            0,
            "the private bridge rejects malformed pointer extents"
        );
        assert_eq!(
            runtime.permanence_of(vtable),
            Some(Permanence::Collectable),
            "post-bootstrap vtables participate in ordinary reachability"
        );
        runtime.gc_mark_floor_roots_for_test();
        assert_eq!(
            runtime.is_marked_for_test(vtable),
            Some(true),
            "an admitted generation owns its resolver-cached block vtables"
        );

        let _ = runtime.add_method(vtable, selector, synchronous_identity);
        assert_eq!(
            runtime.stackless_block_vtable_image_reference(vtable, authority),
            None,
            "overwriting the generated thunk invalidates durable catalog identity"
        );
        assert_eq!(
            runtime.stackless_block_vtable_for_manifest_index(authority, 0),
            None
        );
        assert_eq!(
            runtime.enter(|| unsafe {
                birnam_id_stackless_block_vtable_matches_v1(
                    authority.as_ptr(),
                    authority.len(),
                    0,
                    vtable,
                )
            }),
            0,
            "the private bridge observes catalog drift"
        );
        assert_eq!(
            runtime.stackless_registration_diagnostics().retired_entries,
            1
        );
        let read = runtime.enter_managed_turn();
        assert_eq!(
            runtime.request_stackless_code_generation_retirement(101),
            CodeGenerationRetirement::Deferred,
            "ordinary sends defer irreversible retirement until their outer read guard drops"
        );
        drop(read);
        assert!(runtime.retire_stackless_code_generation(101));
        assert_eq!(
            runtime
                .stackless_registration_diagnostics()
                .registered_entries,
            0
        );
        assert!(!runtime.stackless_resumable_manifest_matches(authority, 101, &manifest));
        assert!(
            runtime.retire_stackless_code_generation(101),
            "the private retirement seam is idempotent after registry removal"
        );
    }

    #[test]
    fn reachable_block_vtable_pins_generation_without_an_active_call() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let (manifest, authority, _descriptor, mut envelope) = frozen_registration_envelope(
            "resumable-manifest-block-only.bin",
            "frame-descriptor-source-map-block.bin",
            core::ptr::null_mut(),
        );
        let thunk = envelope.entry_thunk.expect("fixture has an entry thunk");
        assert!(runtime.begin_staged_resumable_unit(&manifest, authority, 101, 103));
        assert_eq!(
            runtime.enter(|| unsafe {
                birnam_register_resumable_v1(core::ptr::addr_of_mut!(envelope))
            }),
            crate::registration::OK_V1
        );

        let selector = runtime.intern(b"value:").expect("static selector");
        export_zero_payload_instance_with_method(
            runtime,
            b"ReachableMaterializationProbe",
            selector,
            thunk,
        );
        assert!(runtime.finish_staged_resumable_unit());

        assert_eq!(
            runtime.request_stackless_code_generation_retirement(101),
            CodeGenerationRetirement::Rejected,
            "a reachable Block header keeps its private vtable and native generation callable"
        );
        assert!(runtime.stackless_resumable_manifest_matches(authority, 101, &manifest));
    }

    #[test]
    fn named_publication_roots_the_implementation_only_until_entry_retirement() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let implementation = runtime.allocate_instance(runtime.globals().object_vt, 0);
        let (manifest, authority, _descriptor, mut envelope) = frozen_registration_envelope(
            "resumable-manifest-named-only.bin",
            "frame-descriptor-minimal-return.bin",
            implementation,
        );
        assert!(runtime.begin_staged_resumable_unit(&manifest, authority, 101, 103));
        assert_eq!(
            runtime.enter(|| unsafe {
                birnam_register_resumable_v1(core::ptr::addr_of_mut!(envelope))
            }),
            crate::registration::OK_V1
        );
        assert!(runtime.finish_staged_resumable_unit());

        let selector = runtime
            .intern(b"retiringNamedAnchor")
            .expect("static selector");
        let thunk = envelope
            .entry_thunk
            .expect("Prepare requires an entry thunk");
        let _ = runtime.add_method(runtime.globals().object_vt, selector, thunk);
        runtime.gc_mark_floor_roots_for_test();
        assert_eq!(runtime.is_marked_for_test(implementation), Some(true));

        assert!(
            runtime
                .remove_method(runtime.globals().object_vt, selector)
                .is_some()
        );
        assert_eq!(
            runtime.stackless_registration_diagnostics().retired_entries,
            1
        );
        runtime.gc_mark_floor_roots_for_test();
        assert_eq!(runtime.is_marked_for_test(implementation), Some(false));
    }

    #[test]
    fn incomplete_manifest_restores_publications_and_commit_consumes_abort_token() {
        thread_local! { static OVERWRITES: Cell<usize> = const { Cell::new(0) }; }
        unsafe extern "C" fn observe(_: Oop, _: Oop) { OVERWRITES.set(OVERWRITES.get() + 1); }
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let fixture = |name: &str| {
            std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../abi/fixtures/stackless-s2")
                    .join(name),
            )
            .unwrap()
        };
        let mixed = fixture("resumable-manifest-mixed.bin");
        let authority: [u8; 32] = mixed[0xc0..0xe0].try_into().unwrap();
        let named_descriptor = fixture("frame-descriptor-minimal-return.bin");
        let manifest = birnam_artifact::resumable_manifest::inspect(&mixed, authority).unwrap();
        let entry = manifest.entry(0).unwrap();
        let implementation = runtime.allocate_instance(runtime.globals().object_vt, 0);
        let overwritten_selector = runtime
            .intern(b"stagedOverwriteRollback")
            .expect("static selector");
        let added_selector = runtime
            .intern(b"stagedAddRollback")
            .expect("static selector");
        assert_eq!(
            runtime.add_method(
                runtime.globals().object_vt,
                overwritten_selector,
                synchronous_identity,
            ),
            AddMethod::Added
        );
        let mut envelope = BirnamResumableRegistrationV1 {
            carrier_version: CARRIER_VERSION_V1,
            struct_size: core::mem::size_of::<BirnamResumableRegistrationV1>() as u32,
            operation: PREPARE_V1,
            flags: 0,
            manifest_entry_index: 0,
            reserved0: 0,
            registration_token: 0,
            descriptor_bytes: named_descriptor.as_ptr(),
            descriptor_length: named_descriptor.len() as u64,
            entry_thunk: Some(registered_probe_thunk),
            step_method: Some(inert_step),
            implementation_oop: implementation,
            code_generation_identity: 101,
            code_generation_lease: 103,
            portable_implementation_identity: entry.portable_implementation_identity,
            language_arity: entry.language_arity,
            driver_argument_count: 0,
            descriptor_sha256: entry.descriptor_sha256,
        };
        OVERWRITES.set(0);
        runtime.enter(|| unsafe {
            birnam_abi::method_observation::replace(runtime.method_observation(), Some(observe));
        });
        assert!(runtime.begin_staged_resumable_unit(&mixed, authority, 101, 103));
        assert_eq!(
            runtime.enter(|| unsafe {
                birnam_register_resumable_v1(core::ptr::addr_of_mut!(envelope))
            }),
            crate::registration::OK_V1
        );
        assert_eq!(
            runtime.add_method(
                runtime.globals().object_vt,
                overwritten_selector,
                registered_probe_thunk,
            ),
            AddMethod::Overwrote(MethodAddress::from_method(synchronous_identity))
        );
        assert_eq!(
            runtime.add_method(
                runtime.globals().object_vt,
                added_selector,
                registered_probe_thunk,
            ),
            AddMethod::Added
        );
        assert_eq!(OVERWRITES.get(), 1, "only direct replacement notifies");
        assert!(!runtime.finish_staged_resumable_unit());
        assert_eq!(OVERWRITES.get(), 1, "rollback must not notify");
        runtime.enter(|| unsafe {
            birnam_abi::method_observation::replace(runtime.method_observation(), None);
        });
        assert_eq!(
            runtime.lookup_method(runtime.globals().object_vt, overwritten_selector),
            Some(MethodAddress::from_method(synchronous_identity)),
            "rollback restores the exact overwritten predecessor"
        );
        assert_eq!(
            runtime.lookup_method(runtime.globals().object_vt, added_selector),
            None,
            "rollback removes bindings introduced by the failed unit"
        );
        assert_eq!(
            runtime
                .stackless_registration_diagnostics()
                .registered_entries,
            0
        );

        let (manifest, authority, _descriptor, mut envelope) = frozen_registration_envelope(
            "resumable-manifest-block-only.bin",
            "frame-descriptor-source-map-block.bin",
            core::ptr::null_mut(),
        );
        assert!(runtime.begin_staged_resumable_unit(&manifest, authority, 101, 103));
        assert_eq!(
            runtime.enter(|| unsafe {
                birnam_register_resumable_v1(core::ptr::addr_of_mut!(envelope))
            }),
            crate::registration::OK_V1
        );
        assert!(runtime.finish_staged_resumable_unit());
        let token = envelope.registration_token;
        let selector = runtime.intern(b"publishedBlock").unwrap();
        runtime.add_method(
            runtime.globals().object_vt,
            selector,
            registered_probe_thunk,
        );
        envelope = BirnamResumableRegistrationV1 {
            carrier_version: CARRIER_VERSION_V1,
            struct_size: core::mem::size_of::<BirnamResumableRegistrationV1>() as u32,
            operation: ABORT_V1,
            flags: 0,
            manifest_entry_index: 0,
            reserved0: 0,
            registration_token: token,
            descriptor_bytes: core::ptr::null(),
            descriptor_length: 0,
            entry_thunk: None,
            step_method: None,
            implementation_oop: core::ptr::null_mut(),
            code_generation_identity: 0,
            code_generation_lease: 0,
            portable_implementation_identity: 0,
            language_arity: 0,
            driver_argument_count: 0,
            descriptor_sha256: [0; 32],
        };
        assert_eq!(
            runtime.enter(|| unsafe {
                birnam_register_resumable_v1(core::ptr::addr_of_mut!(envelope))
            }),
            crate::registration::REJECTED_V1,
            "binding publication consumes the initializer-local Abort token"
        );
        assert_eq!(runtime.stackless_registration_diagnostics().live_entries, 1);
    }

    #[test]
    fn incomplete_manifest_removes_staged_block_binding_and_catalog_entry() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let fixture = |name: &str| {
            std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../abi/fixtures/stackless-s2")
                    .join(name),
            )
            .unwrap()
        };
        let mixed = fixture("resumable-manifest-mixed.bin");
        let authority: [u8; 32] = mixed[0xc0..0xe0].try_into().unwrap();
        let descriptor = fixture("frame-descriptor-source-map-block.bin");
        let manifest = birnam_artifact::resumable_manifest::inspect(&mixed, authority).unwrap();
        let entry = manifest.entry(1).expect("mixed fixture has a block row");
        assert_eq!(
            entry.implementation_kind,
            birnam_artifact::stackless_selection::ImplementationKind::BlockBody
        );
        let path = entry.path.iter().collect::<Vec<_>>();
        let selection = birnam_artifact::stackless_selection::SelectionSpec {
            source_revision: entry.source_revision,
            root_kind: entry.root_kind,
            root_index: entry.root_index,
            path: &path,
            implementation_kind: entry.implementation_kind,
            disposition: birnam_artifact::stackless_selection::Disposition::ResumableRequired,
        };
        let driver_argument_count =
            birnam_artifact::frame_descriptor::inspect(&descriptor, selection)
                .unwrap()
                .metadata()
                .driver_argument_count;
        let mut envelope = BirnamResumableRegistrationV1 {
            carrier_version: CARRIER_VERSION_V1,
            struct_size: core::mem::size_of::<BirnamResumableRegistrationV1>() as u32,
            operation: PREPARE_V1,
            flags: 0,
            manifest_entry_index: 1,
            reserved0: 0,
            registration_token: 0,
            descriptor_bytes: descriptor.as_ptr(),
            descriptor_length: descriptor.len() as u64,
            entry_thunk: Some(registered_probe_thunk),
            step_method: Some(inert_step),
            implementation_oop: core::ptr::null_mut(),
            code_generation_identity: 101,
            code_generation_lease: 103,
            portable_implementation_identity: entry.portable_implementation_identity,
            language_arity: entry.language_arity,
            driver_argument_count,
            descriptor_sha256: entry.descriptor_sha256,
        };
        assert!(runtime.begin_staged_resumable_unit(&mixed, authority, 101, 103));
        assert_eq!(
            runtime.enter(|| unsafe {
                birnam_register_resumable_v1(core::ptr::addr_of_mut!(envelope))
            }),
            crate::registration::OK_V1
        );
        let vtable = runtime.create_vtable(runtime.globals().object_vt);
        let selector = runtime.intern(b"value:").expect("static selector");
        assert_eq!(
            runtime.add_method(vtable, selector, registered_probe_thunk),
            AddMethod::Added
        );
        assert_eq!(
            unsafe { (*runtime.state.get()).block_vtable_catalog.len() },
            1
        );

        assert!(!runtime.finish_staged_resumable_unit());
        assert_eq!(runtime.lookup_method(vtable, selector), None);
        assert_eq!(
            unsafe { (*runtime.state.get()).block_vtable_catalog.len() },
            0
        );
        assert_eq!(
            runtime
                .stackless_registration_diagnostics()
                .registered_entries,
            0
        );
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
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 43,
                generation: 9,
                source_map: 43,
            },
            program_counter_count: 1,
            driver_argument_count: 0,
            call_sites: Vec::new(),
            step_method: call_child_step,
            entry_argument_count: 1,
            edges: vec![],
            ..probe_spec(2, 1)
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
                let entry = unsafe { (*runtime.state.get()).vtables.native_lookup(vtable, candidate) }.unwrap();
                assert_ne!(entry.method.map(|method| method as usize), Some(step_address.as_oop().addr()));
                assert_ne!(crate::legacy_method::try_decode(entry), Some(step_address),
                    "vtable {vtable_index} selector {selector_index} exposed a StepMethod");
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

        let replacement = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 45,
                generation: 10,
                source_map: 45,
            },
            program_counter_count: 1,
            driver_argument_count: 0,
            call_sites: Vec::new(),
            step_method: microcode_replacement_step,
            entry_argument_count: 1,
            edges: vec![],
            ..probe_spec(2, 1)
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
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 44,
                generation: 10,
                source_map: 44,
            },
            program_counter_count: 2,
            driver_argument_count: 1,
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
            edges: vec![
                probe_continue(0, 0, 5),
                probe_call(0, 1, 0, 2, 5),
                probe_continue(1, 1, 5),
            ],
            ..probe_spec(5, 2)
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
        assert_eq!(
            MICROCODE_REPLACEMENT_CALLS.get(),
            0,
            "ordinary microcode send enters the registered replacement without its thunk"
        );
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
            FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
                identity: DescriptorIdentity {
                    implementation,
                    generation,
                    source_map: implementation,
                },
                program_counter_count: 1,
                driver_argument_count: 0,
                call_sites: Vec::new(),
                step_method: call_child_step,
                entry_argument_count: 1,
                edges: vec![],
                ..probe_spec(2, 1)
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
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 73,
                generation: 23,
                source_map: 73,
            },
            program_counter_count: 1,
            driver_argument_count: 0,
            call_sites: Vec::new(),
            step_method: call_child_step,
            entry_argument_count: 1,
            edges: vec![],
            ..probe_spec(2, 1)
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

        let read = runtime.enter_managed_turn();
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

    #[test]
    fn registered_block_captures_materialize_from_exact_receiver_payload() {
        unsafe extern "C" fn portable_probe_thunk(receiver: Oop, _: *const Oop, _: usize) -> Oop {
            receiver
        }

        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        let selector = runtime.intern(b"capturedValue").expect("static selector");
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 79,
                generation: 29,
                source_map: 79,
            },
            program_counter_count: 1,
            driver_argument_count: 0,
            slot_count: 2,
            entry_receiver_slot: 0,
            entry_argument_start: 1,
            entry_argument_count: 0,
            entry_capture_slots: vec![1],
            implementation_slot: None,
            root_bitmap_words: vec![0b11],
            call_sites: Vec::new(),
            edges: Vec::new(),
            step_method: inert_step,
        })
        .expect("capture descriptor validates");
        runtime
            .register_stackless_resumable(SemanticRegistrationV1::new(
                registered_probe_thunk,
                descriptor,
                0,
                0,
                1,
            ))
            .expect("capturing entry registers");
        let closure_vtable = runtime.create_vtable(runtime.globals().object_vt);
        assert_eq!(
            runtime.add_method(closure_vtable, selector, registered_probe_thunk),
            AddMethod::Added
        );
        let malformed = runtime.allocate_instance(closure_vtable, 0);
        let capture = runtime.make_integer(131);
        let closure = runtime.allocate_instance(closure_vtable, core::mem::size_of::<usize>());
        assert!(unsafe {
            (&mut *runtime.state.get())
                .heap
                .write_payload_word(closure, 0, capture.addr())
        });

        let acquire = |receiver| {
            let read = runtime.enter_managed_turn();
            let method = runtime
                .lookup_method(closure_vtable, selector)
                .expect("published block thunk resolves");
            let target = unsafe {
                (&mut *runtime.state.get())
                    .execution
                    .classify_resumable(method)
                    .expect("published block thunk classifies")
                    .expect("registered block thunk is resumable")
            };
            drop(read);
            let StacklessTarget::Registered { entry } = target else {
                panic!("classification must acquire the registered block entry")
            };
            (receiver, entry)
        };

        let (malformed, entry) = acquire(malformed);
        assert!(
            matches!(
                runtime.materialize_registered_target(entry, malformed, &[]),
                Err(crate::execution::ExecutionError::InvalidAction)
            ),
            "capture-bearing entry rejects a receiver without the exact capture vector"
        );
        assert_eq!(
            runtime
                .stackless_registration_diagnostics()
                .acquired_entries,
            0,
            "failed capture materialization releases its acquisition"
        );

        let (closure, entry) = acquire(closure);
        let materialized = runtime
            .materialize_registered_target(entry, closure, &[])
            .expect("exact capture carrier materializes");
        assert_eq!(materialized.initial_oops, vec![(0, closure), (1, capture)]);
        unsafe {
            (&mut *runtime.state.get())
                .execution
                .release_acquired_entry(entry)
                .expect("test releases successful materialization")
        };

        let portable_selector = runtime
            .intern(b"portableCapturedValue")
            .expect("static selector");
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 83,
                generation: 31,
                source_map: 83,
            },
            program_counter_count: 1,
            driver_argument_count: 0,
            slot_count: 2,
            entry_receiver_slot: 0,
            entry_argument_start: 1,
            entry_argument_count: 0,
            entry_capture_slots: vec![1],
            implementation_slot: None,
            root_bitmap_words: vec![0b11],
            call_sites: Vec::new(),
            edges: Vec::new(),
            step_method: inert_step,
        })
        .expect("portable capture descriptor validates");
        runtime
            .register_stackless_resumable(
                SemanticRegistrationV1::new(portable_probe_thunk, descriptor, 0, 0, 1)
                    .with_receiver_capture_offset_words(1),
            )
            .expect("portable capturing entry registers");
        let portable_vtable = runtime.create_vtable(runtime.globals().object_vt);
        assert_eq!(
            runtime.add_method(portable_vtable, portable_selector, portable_probe_thunk),
            AddMethod::Added
        );
        let code_object = runtime.make_integer(137);
        let malformed = runtime.allocate_instance(portable_vtable, core::mem::size_of::<usize>());
        assert!(unsafe {
            (&mut *runtime.state.get())
                .heap
                .write_payload_word(malformed, 0, code_object.addr())
        });
        let portable =
            runtime.allocate_instance(portable_vtable, 2 * core::mem::size_of::<usize>());
        assert!(unsafe {
            (&mut *runtime.state.get())
                .heap
                .write_payload_word(portable, 0, code_object.addr())
        });
        assert!(unsafe {
            (&mut *runtime.state.get())
                .heap
                .write_payload_word(portable, 1, capture.addr())
        });

        let acquire_portable = |receiver| {
            let read = runtime.enter_managed_turn();
            let method = runtime
                .lookup_method(portable_vtable, portable_selector)
                .expect("portable block thunk resolves");
            let target = unsafe {
                (&mut *runtime.state.get())
                    .execution
                    .classify_resumable(method)
                    .expect("portable block thunk classifies")
                    .expect("portable block thunk is resumable")
            };
            drop(read);
            let StacklessTarget::Registered { entry } = target else {
                panic!("classification must acquire the portable block entry")
            };
            (receiver, entry)
        };

        let (malformed, entry) = acquire_portable(malformed);
        assert!(matches!(
            runtime.materialize_registered_target(entry, malformed, &[]),
            Err(crate::execution::ExecutionError::InvalidAction)
        ));
        let (portable, entry) = acquire_portable(portable);
        let materialized = runtime
            .materialize_registered_target(entry, portable, &[])
            .expect("portable capture carrier materializes");
        assert_eq!(materialized.initial_oops, vec![(0, portable), (1, capture)]);
        unsafe {
            (&mut *runtime.state.get())
                .execution
                .release_acquired_entry(entry)
                .expect("test releases portable materialization")
        };
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
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: descriptor_identity,
            program_counter_count: 2,
            driver_argument_count: 1,
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
            edges: vec![probe_call(0, 1, 0, 3, 4)],
            ..probe_spec(4, 2)
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
                    Ok(StacklessTarget::Synchronous(crate::legacy_method::encode(synchronous_identity, false)))
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
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 51,
                generation: 12,
                source_map: 51,
            },
            program_counter_count: 1,
            driver_argument_count: 0,
            call_sites: Vec::new(),
            step_method: redefine_during_registered_step,
            edges: vec![],
            ..probe_spec(2, 1)
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
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 61,
                generation: 14,
                source_map: 61,
            },
            program_counter_count: 1,
            driver_argument_count: 0,
            call_sites: Vec::new(),
            step_method: call_child_step,
            entry_argument_count: 1,
            edges: vec![],
            ..probe_spec(2, 1)
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

        let read = runtime.enter_managed_turn();
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
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 91,
                generation: 31,
                source_map: 91,
            },
            program_counter_count: 1,
            driver_argument_count: 0,
            call_sites: Vec::new(),
            step_method: call_child_step,
            entry_argument_count: 1,
            edges: vec![],
            ..probe_spec(2, 1)
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
        assert_eq!(
            REGISTERED_THUNK_CALLS.get(),
            0,
            "custom metacircular lookup shares resolve-and-pin and bypasses the thunk"
        );
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
            FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
                identity: DescriptorIdentity {
                    implementation,
                    generation: 1,
                    source_map: implementation,
                },
                program_counter_count: 1,
                driver_argument_count: 0,
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
                edges: vec![],
                ..probe_spec(3, 1)
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
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 3,
                generation: 1,
                source_map: 3,
            },
            program_counter_count: 1,
            driver_argument_count: 0,
            call_sites: Vec::new(),
            step_method: malformed_action_step,
            edges: vec![],
            ..probe_spec(1, 1)
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

        let descriptor = |implementation, slot_kinds: Vec<SlotKind>, step_method| {
            FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
                identity: DescriptorIdentity {
                    implementation,
                    generation: 37,
                    source_map: implementation,
                },
                program_counter_count: 1,
                driver_argument_count: 0,
                call_sites: Vec::new(),
                step_method,
                edges: vec![],
                ..probe_spec(slot_kinds.len(), 1)
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
                descriptor(104, vec![SlotKind::Oop], wrong_slot_return_action_step),
                &[(0, live_value)],
            )
            .expect_err("out-of-range value slot must fail");
        assert_eq!(
            wrong_slot_error,
            crate::execution::ExecutionError::Frame(crate::execution::FrameError::InvalidSlot)
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
        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 4,
                generation: 1,
                source_map: 4,
            },
            program_counter_count: 1,
            driver_argument_count: 0,
            call_sites: Vec::new(),
            step_method: inert_step,
            edges: vec![],
            ..probe_spec(1, 1)
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

    #[cfg(feature = "dev-guarded-slots")]
    thread_local! {
        static GUARD_PROBE_FAILURES: Cell<u32> = const { Cell::new(0) };
    }

    #[cfg(feature = "dev-guarded-slots")]
    fn guard_check(condition: bool, what: &str) {
        if !condition {
            eprintln!("guarded slot probe: {what}");
            GUARD_PROBE_FAILURES.set(GUARD_PROBE_FAILURES.get() + 1);
        }
    }

    #[cfg(feature = "dev-guarded-slots")]
    unsafe extern "C" fn guarded_probe_step(machine: Oop, _: *const Oop, count: usize) -> Oop {
        with_active_if_present(|runtime| {
            if count != 0 {
                return runtime.canonical().nil;
            }
            let nil = runtime.canonical().nil;
            let oop_at = runtime.intern(b"oopAt:").expect("static selector");
            let oop_at_put = runtime.intern(b"oopAt:put:").expect("static selector");
            let lookup = runtime.globals().lookup;
            let objects = unsafe { (*runtime.state.get()).execution.objects() }
                .expect("execution objects are installed");
            let machine_vt = objects.machine_vtable;
            let vtable_vt = runtime.globals().vtable_vt;

            // Hits answer exactly what the send answers, including nil failures.
            for index in 0..2 {
                let operand = runtime.make_integer(index);
                let direct = runtime.guarded_frame_oop_at(machine, operand);
                let sent = runtime.send_slice(machine, oop_at, &[operand]);
                guard_check(direct == Some(sent), "read agrees with send");
            }
            let raw_slot = runtime.make_integer(3);
            guard_check(
                runtime.guarded_frame_oop_at(machine, raw_slot) == Some(nil)
                    && runtime.send_slice(machine, oop_at, &[raw_slot]) == nil,
                "raw slot answers nil on both paths",
            );
            let beyond = runtime.make_integer(9);
            guard_check(
                runtime.guarded_frame_oop_at(machine, beyond) == Some(nil)
                    && runtime.send_slice(machine, oop_at, &[beyond]) == nil,
                "out-of-range slot answers nil on both paths",
            );
            guard_check(
                runtime.guarded_frame_oop_at(machine, nil) == Some(nil)
                    && runtime.send_slice(machine, oop_at, &[nil]) == nil,
                "non-Integer operand answers nil on both paths",
            );
            let value_slot = runtime.make_integer(2);
            let fifty_five = runtime.make_integer(55);
            guard_check(
                runtime.guarded_frame_oop_at_put(machine, value_slot, fifty_five)
                    == Some(fifty_five),
                "guarded write answers the stored value",
            );
            guard_check(
                runtime.send_slice(machine, oop_at, &[value_slot]) == fifty_five,
                "the send observes the guarded write",
            );
            let sixty_six = runtime.make_integer(66);
            let _ = runtime.send_slice(machine, oop_at_put, &[value_slot, sixty_six]);
            guard_check(
                runtime.guarded_frame_oop_at(machine, value_slot) == Some(sixty_six),
                "the guarded read observes the sent write",
            );
            guard_check(
                runtime.guarded_frame_oop_at_put(machine, raw_slot, sixty_six) == Some(nil)
                    && runtime.send_slice(machine, oop_at_put, &[raw_slot, sixty_six]) == nil,
                "raw-slot write is rejected identically",
            );

            // Design 74D: the unboxed entry points agree with the boxed ones
            // and with the send on hits, including every nil case.
            for index in [0i64, 1, 2, 3, 9] {
                let operand = runtime.make_integer(index);
                guard_check(
                    runtime.guarded_frame_slot_at(machine, index)
                        == runtime.guarded_frame_oop_at(machine, operand)
                        && runtime.guarded_frame_slot_at(machine, index)
                            == Some(runtime.send_slice(machine, oop_at, &[operand])),
                    "unboxed read agrees with the boxed read and the send",
                );
            }
            guard_check(
                runtime.guarded_frame_slot_at(machine, -1) == Some(nil)
                    && runtime.send_slice(machine, oop_at, &[runtime.make_integer(-1)]) == nil,
                "negative unboxed index answers nil as the send does",
            );
            let seventy_seven = runtime.make_integer(77);
            guard_check(
                runtime.guarded_frame_slot_at_put(machine, 2, seventy_seven) == Some(seventy_seven)
                    && runtime.send_slice(machine, oop_at, &[value_slot]) == seventy_seven
                    && runtime.guarded_frame_slot_at(machine, 2) == Some(seventy_seven),
                "unboxed write is visible to the send and the reads",
            );
            guard_check(
                runtime.guarded_frame_slot_at_put(machine, 3, seventy_seven) == Some(nil)
                    && runtime.guarded_frame_slot_at_put(machine, -1, seventy_seven) == Some(nil),
                "unboxed raw-slot and negative writes answer nil",
            );

            // Design 74D miss path: the floor performs the ordinary send once
            // with the boxed index, so a counting replacement sees exactly one
            // send with the right operand.
            let _ = runtime.add_method(machine_vt, oop_at, synchronous_increment);
            guard_check(
                runtime.guarded_frame_slot_at(machine, 41).is_none(),
                "unboxed read misses under a redefined oopAt:",
            );
            let answered = runtime.fallback_frame_oop_at(machine, 41);
            guard_check(
                runtime.is_integer(answered) && runtime.integer_value(answered) == 42,
                "the miss fallback sent oopAt: once with the boxed index",
            );
            let _ = runtime.add_method(machine_vt, oop_at, crate::execution::machine_oop_at);
            let _ = runtime.add_method(machine_vt, oop_at_put, synchronous_second_argument);
            let fresh = runtime.make_integer(4242);
            guard_check(
                runtime
                    .guarded_frame_slot_at_put(machine, 2, fresh)
                    .is_none(),
                "unboxed write misses under a redefined oopAt:put:",
            );
            // Precise collection, so the value's survival is owed to the scoped
            // root and not to the conservative native-stack scan.
            let was_precise = unsafe { (*runtime.state.get()).metadata.precise() };
            runtime.gc_set_precise(true);
            crate::runtime::FORCE_COLLECT_IN_SLOT_FALLBACK.with(|flag| flag.set(true));
            let delivered = runtime.fallback_frame_oop_at_put(machine, 2, fresh);
            runtime.gc_set_precise(was_precise);
            guard_check(
                delivered == fresh
                    && runtime.is_integer(delivered)
                    && runtime.integer_value(delivered) == 4242,
                "the store miss delivers the rooted value across a forced collection",
            );
            let _ =
                runtime.add_method(machine_vt, oop_at_put, crate::execution::machine_oop_at_put);
            guard_check(
                runtime.guarded_slot_access_eligible(machine),
                "canonical entries restored hit again",
            );
            let _ = runtime.guarded_frame_slot_at_put(machine, 2, sixty_six);

            // Redefining the canonical entry makes the guard miss; restoring it
            // re-verifies at the new epoch.
            let _ = runtime.add_method(machine_vt, oop_at, synchronous_increment);
            guard_check(
                !runtime.guarded_slot_access_eligible(machine),
                "redefined oopAt: misses",
            );
            guard_check(
                runtime.guarded_frame_oop_at(machine, value_slot).is_none(),
                "miss answers None for the caller's fallback",
            );
            let _ = runtime.add_method(machine_vt, oop_at, crate::execution::machine_oop_at);
            guard_check(
                runtime.guarded_slot_access_eligible(machine),
                "restored oopAt: hits again",
            );
            let _ = runtime.remove_method(machine_vt, oop_at_put);
            guard_check(
                !runtime.guarded_slot_access_eligible(machine),
                "removed oopAt:put: misses",
            );
            let _ =
                runtime.add_method(machine_vt, oop_at_put, crate::execution::machine_oop_at_put);
            guard_check(
                runtime.guarded_slot_access_eligible(machine),
                "restored oopAt:put: hits again",
            );

            // A global custom lookup on vtable_vt misses; restoring it hits.
            let _ = runtime.add_method(vtable_vt, lookup, synchronous_increment);
            guard_check(
                !runtime.guarded_slot_access_eligible(machine),
                "custom global lookup misses",
            );
            let _ = runtime.add_method(vtable_vt, lookup, crate::dispatch::vtable_lookup_method);
            guard_check(
                runtime.guarded_slot_access_eligible(machine),
                "floor lookup restored hits",
            );

            // ensurePrivateVtable on a vtable answers the vtable itself (its
            // dispatch vtable is already vtable_vt), so a different metavtable
            // can only arrive through bootstrap or image paths. Simulate that
            // untracked header change plus the epoch move those paths make.
            let meta = runtime.create_vtable(vtable_vt);
            unsafe {
                let state = &mut *runtime.state.get();
                assert!(state.heap.set_vtable(machine_vt, meta));
                state.vtables.note_mutation();
            }
            guard_check(
                !runtime.guarded_slot_access_eligible(machine),
                "a non-canonical metavtable on the Machine vtable misses",
            );
            unsafe {
                let state = &mut *runtime.state.get();
                assert!(state.heap.set_vtable(machine_vt, vtable_vt));
                state.vtables.note_mutation();
            }
            guard_check(
                runtime.guarded_slot_access_eligible(machine),
                "the canonical metavtable restored hits",
            );
            let _ = runtime.ensure_private_vtable(machine);
            guard_check(
                !runtime.guarded_slot_access_eligible(machine),
                "private vtable on the Machine misses",
            );
            guard_check(
                runtime.send_slice(machine, oop_at, &[value_slot]) == sixty_six,
                "the ordinary send still works after every miss",
            );

            let return_at = runtime.intern(b"returnAt:").expect("static selector");
            runtime.send_slice(machine, return_at, &[value_slot])
        })
        .unwrap_or(core::ptr::null_mut())
    }

    #[cfg(feature = "dev-guarded-slots")]
    #[test]
    fn guarded_slot_access_agrees_with_the_canonical_send_and_misses_on_overrides() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.publish_canonical_exports();
        runtime.install_execution_objects();
        GUARD_PROBE_FAILURES.set(0);

        let machine = runtime
            .create_stackless_machine()
            .expect("guard probe Machine creates");
        let nil = runtime.canonical().nil;
        let zero = runtime.make_integer(0);
        assert!(
            runtime.guarded_slot_access_eligible(machine),
            "a fresh canonical Machine is eligible"
        );
        runtime.enter(|| {
            assert_eq!(
                runtime.guarded_frame_oop_at(machine, zero),
                Some(nil),
                "outside a drive the guarded read fails to nil exactly as the send does"
            );
            assert_eq!(
                runtime.send_slice(
                    machine,
                    runtime.intern(b"oopAt:").expect("static selector"),
                    &[zero]
                ),
                nil
            );
        });

        let descriptor = FrameDescriptor::validate_v2(FrameDescriptorV2Spec {
            identity: DescriptorIdentity {
                implementation: 74,
                generation: 1,
                source_map: 74,
            },
            program_counter_count: 1,
            driver_argument_count: 0,
            call_sites: Vec::new(),
            step_method: guarded_probe_step,
            edges: vec![],
            ..probe_spec(4, 1)
        })
        .expect("guard probe descriptor validates");
        let first = runtime.make_integer(11);
        let second = runtime.make_integer(22);
        let result = runtime
            .drive_stackless(machine, descriptor, &[(0, first), (1, second), (2, nil)])
            .expect("guard probe drives to completion");
        assert!(runtime.is_integer(result));
        assert_eq!(runtime.integer_value(result), 66);
        assert_eq!(
            GUARD_PROBE_FAILURES.get(),
            0,
            "every guarded slot probe agreed"
        );
    }
}

// Only the test harness statically links the upper algorithm. Production Id
// has no dependency on birnam-object-model and exposes only the seed operations.
#[cfg(test)]
impl birnam_object_model::BehaviorAccess for Runtime {
    fn prepare(&self, object: Oop) -> Oop {
        self.prepare_behavior_mutation(object)
    }
    fn metavtable(&self) -> Oop {
        self.globals().vtable_vt
    }
    fn owner(&self, behavior: Oop) -> Oop {
        self.vtable_owner(behavior)
    }
    fn delegate(&self, parent: Oop) -> Oop {
        self.create_vtable(parent)
    }
    fn set_owner(&self, behavior: Oop, object: Oop) {
        self.set_vtable_owner(behavior, object);
    }
    fn attach(&self, object: Oop, behavior: Oop) {
        self.reassociate_behavior(object, behavior);
    }
}

#[cfg(test)]
mod collection_phase_tests;

#[cfg(test)]
mod guard_verdict_tests;

#[cfg(test)]
mod mutation_epoch_tests;
