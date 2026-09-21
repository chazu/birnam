//! Protected physical-storage facts. No tracing, settlement or reclamation.
use super::*;
use crate::abi::storage::*;

static NEXT_RUNTIME: AtomicU64 = AtomicU64::new(1);

pub(super) struct ObservationState {
    identity: u64,
    scope: crate::storage_observation::Service,
}
impl ObservationState {
    pub(super) fn new() -> Self {
        let identity = NEXT_RUNTIME.fetch_add(1, Ordering::Relaxed);
        boundary::require_invariant(identity != u64::MAX, "storage runtime identity exhausted");
        Self {
            identity,
            scope: crate::storage_observation::Service::new(),
        }
    }
}
enum Flag { Active, Permit }
struct Reset<'a>(&'a crate::storage_observation::Service, Flag);
impl Drop for Reset<'_> {
    fn drop(&mut self) {
        match self.1 { Flag::Active => self.0.set_active(false), Flag::Permit => self.0.set_permit(false) }
    }
}

impl Runtime {
    pub(crate) fn require_storage_access(&self) {
        boundary::require_invariant(
            current_thread_token() == self.owner,
            "Rust floor entered from a non-owner thread",
        );
        boundary::require_invariant(
            !self.observation.scope.active(),
            "Id access during storage observation",
        );
    }

    /// One host authorization, consumed by the default storage method. The
    /// send still performs ordinary lookup and retains its native code lease.
    pub(crate) unsafe fn observe_storage(&self, request: *const RequestV1) -> Status {
        boundary::require_invariant(
            current_thread_token() == self.owner,
            "Rust floor entered from a non-owner thread",
        );
        if self.observation.scope.active() || self.observation.scope.permit() {
            return BUSY;
        }
        self.assert_owner();
        if self.collection_phase.get() != CollectionPhase::Idle {
            return BUSY;
        }
        let selector = unsafe {
            let state = &*self.state.get();
            if state.execution.has_managed_turn()
                || !(&*self.entry_scopes.get()).is_empty()
                || state.staged_resumable_unit.is_some()
                || state.staged_resumable_publications.is_some()
                || !state.pending_stackless_retirements.is_empty()
            {
                return BUSY;
            }
            state.symbols.existing(b"withStorageView:")
        };
        let Some(selector) = selector else {
            return INVALID;
        };
        if unsafe { valid_request(request) }.is_none() {
            return INVALID;
        }
        self.observation.scope.set_permit(true);
        let _permit = Reset(&self.observation.scope, Flag::Permit);
        let answer = self.enter(|| {
            self.send_slice(
                self.globals().vtable_vt,
                selector,
                &[request.cast_mut().cast()],
            )
        });
        Status::try_from(answer.addr() as isize).unwrap_or(INVALID)
    }

    unsafe fn with_storage(&self, request: *const RequestV1) -> Status {
        if !self.observation.scope.permit()
            || self.observation.scope.active()
            || self.collection_phase.get() != CollectionPhase::Idle
            || unsafe { (&*self.state.get()).execution.managed_turn_depth() } != 1
            || unsafe { !(&*self.entry_scopes.get()).is_empty() }
        {
            return BUSY;
        }
        let Some((context, visitor)) = (unsafe { valid_request(request) }) else {
            return INVALID;
        };
        self.observation.scope.set_permit(false);
        let Some(epoch) = self.observation.scope.advance_epoch() else {
            return INVALID;
        };
        self.observation.scope.set_active(true);
        let _phase = Reset(&self.observation.scope, Flag::Active);

        // Trace copies Id and representation facts with no RuntimeState borrow.
        // This uses the same complete graph already selected for collection.
        let native = if let Some(graph) = self.trace_graph {
            let mut inventory = Inventory::default();
            let status = unsafe { (graph.inspect)(graph.context, (&raw mut inventory).cast(), Some(copy_inventory)) };
            if status != OK { return status; }
            Some(inventory)
        } else { None };
        // The compatibility path asks the completed Id for its roots too.
        // No RuntimeState reference survives that native callback.
        let id_roots = if native.is_none() {
            self.id_storage.map(|id| id.roots()).unwrap_or_default()
        } else { Vec::new() };
        let (allocations, roots, edges, providers, deferred_count, deferred_bytes, precise) = unsafe {
            let state = &*self.state.get();
            let Inventory { mut allocations, mut roots, edges, symbols } =
                native.unwrap_or_else(|| legacy_inventory(state, id_roots));
            let addresses: std::collections::HashSet<_> = allocations.iter().map(|entry| entry.object.addr()).collect();
            let mut permanent = std::collections::HashSet::new();
            for allocation in &mut allocations {
                // Permanence belongs to the collector, not Id or Trace.
                if state.heap.permanence_of(allocation.object) == Some(Permanence::Permanent) {
                    allocation.flags |= PERMANENT;
                    permanent.insert(allocation.object.addr());
                }
            }
            roots.extend(upper_roots_with_symbols(state,
                if state.symbol_compaction_enabled { Vec::new() } else { symbols }));
            roots.extend(edges.iter().filter(|edge| permanent.contains(&edge.from.addr())).map(|edge| edge.to));
            #[cfg(test)]
            if let Some(owner) = &*self.test_integer.get() {
                roots.extend(owner.instance.identities());
            }
            roots.retain(|root| addresses.contains(&root.addr()));
            roots.sort_unstable_by_key(|root| root.addr());
            roots.dedup();
            (
                allocations, roots, edges, state.root_registry.snapshot(),
                state.heap.dead_allocation_count(), state.heap.deferred_bytes(),
                state.metadata.precise(),
            )
        };
        let coverage_gaps = NATIVE_ROOTS_UNOBSERVED
            | if providers.is_empty() {
                0
            } else {
                LEGACY_ROOTS_UNOBSERVED
            }
            | if allocations.iter().any(|entry| entry.trace_hook.is_some()) {
                LEGACY_HOOKS_UNOBSERVED
            } else {
                0
            }
            | if deferred_count == 0 {
                0
            } else {
                DEFERRED_IDENTITIES_UNAVAILABLE
            };
        let view = ViewV1 {
            version: VERSION,
            byte_size: size_of::<ViewV1>(),
            runtime_identity: self.observation.identity,
            observation_identity: epoch,
            allocations: allocations.as_ptr(),
            allocation_count: allocations.len(),
            roots: roots.as_ptr(),
            root_count: roots.len(),
            edges: edges.as_ptr(),
            edge_count: edges.len(),
            legacy_root_providers: providers.as_ptr(),
            legacy_root_provider_count: providers.len(),
            deferred_count,
            deferred_bytes,
            coverage_gaps,
            gc_precise: u32::from(precise),
        };
        unsafe { visitor(context, &view) }
    }
}

#[derive(Default)]
struct Inventory {
    allocations: Vec<AllocationV1>,
    roots: Vec<Oop>,
    edges: Vec<EdgeV1>,
    symbols: Vec<Oop>,
}
unsafe extern "C" fn copy_inventory(context: *mut core::ffi::c_void, snapshot: *const birnam_abi::trace_graph::SnapshotV1) -> Status {
    let (inventory, snapshot) = unsafe { (&mut *context.cast::<Inventory>(), &*snapshot) };
    let (Some(allocations), Some(roots), Some(edges), Some(symbols)) = (unsafe {
        (crate::abi::borrowed_slice(snapshot.allocations, snapshot.allocation_count),
         crate::abi::borrowed_slice(snapshot.roots, snapshot.root_count),
         crate::abi::borrowed_slice(snapshot.edges, snapshot.edge_count),
         crate::abi::borrowed_slice(snapshot.symbols, snapshot.symbol_count))
    }) else { return INVALID; };
    inventory.allocations = allocations.to_vec();
    inventory.roots = roots.to_vec();
    inventory.edges = edges.to_vec();
    inventory.symbols = symbols.to_vec();
    OK
}

// Local/legacy construction has no separately loaded graph. This branch goes
// away with id_init and the aggregate Runtime constructor; production uses Trace.
fn legacy_inventory(state: &RuntimeState, roots: Vec<Oop>) -> Inventory {
    let mut allocations = state.heap.storage_allocations();
    let mut edges = Vec::new();
    let mut symbols = Vec::new();
    for allocation in &mut allocations {
        let object = allocation.object;
        if !allocation.behavior.is_null() { edges.push(EdgeV1 { from: object, to: allocation.behavior }); }
        if state.vtables.is_registered(object) {
            allocation.flags |= CORE_PAYLOAD;
            for to in state.vtables.strong_edges(object) {
                if state.heap.contains(to) { edges.push(EdgeV1 { from: object, to }); }
            }
        } else if !state.symbols.c_name_of(object).is_null() {
            allocation.flags |= CORE_PAYLOAD;
            symbols.push(object);
        }
        if state.vtables.has_layout(allocation.behavior) { allocation.flags |= HAS_GC_LAYOUT; }
        allocation.trace_hook = state.vtables.trace_hook(allocation.behavior);
    }
    Inventory {
        allocations, edges, symbols,
        roots,
    }
}

unsafe fn valid_request(request: *const RequestV1) -> Option<(*mut core::ffi::c_void, VisitorV1)> {
    if request.is_null() || !request.addr().is_multiple_of(align_of::<RequestV1>()) {
        return None;
    }
    let request = unsafe { &*request };
    if request.version != VERSION || request.byte_size != size_of::<RequestV1>() {
        return None;
    }
    Some((request.context, request.visitor?))
}

pub(crate) unsafe extern "C" fn with_storage_view(_: Oop, args: *const Oop, count: usize) -> Oop {
    let Some([request]) = (unsafe { crate::abi::borrowed_oop_args(args, count) }) else {
        return INVALID as usize as Oop;
    };
    let status = with_active(|runtime| unsafe { runtime.with_storage((*request).cast()) });
    status as usize as Oop
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod scope_tests;
