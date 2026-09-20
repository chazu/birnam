//! Protected physical-storage facts. No tracing, settlement or reclamation.
use super::*;
use crate::abi::storage::*;

static NEXT_RUNTIME: AtomicU64 = AtomicU64::new(1);

pub(super) struct ObservationState {
    identity: u64,
    epoch: Cell<u64>,
    active: Cell<bool>,
    permit: Cell<bool>,
}
impl ObservationState {
    pub(super) fn new() -> Self {
        let identity = NEXT_RUNTIME.fetch_add(1, Ordering::Relaxed);
        boundary::require_invariant(identity != u64::MAX, "storage runtime identity exhausted");
        Self {
            identity,
            epoch: Cell::new(0),
            active: Cell::new(false),
            permit: Cell::new(false),
        }
    }
}
struct Reset<'a>(&'a Cell<bool>);
impl Drop for Reset<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

impl Runtime {
    pub(crate) fn require_storage_access(&self) {
        boundary::require_invariant(
            current_thread_token() == self.owner,
            "Rust floor entered from a non-owner thread",
        );
        boundary::require_invariant(
            !self.observation.active.get(),
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
        if self.observation.active.get() || self.observation.permit.get() {
            return BUSY;
        }
        self.assert_owner();
        if self.collection_phase.get() != CollectionPhase::Idle {
            return BUSY;
        }
        let selector = unsafe {
            let state = &*self.state.get();
            if state.execution.has_dispatch_readers()
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
        self.observation.permit.set(true);
        let _permit = Reset(&self.observation.permit);
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
        if !self.observation.permit.get()
            || self.observation.active.get()
            || self.collection_phase.get() != CollectionPhase::Idle
            || unsafe { (&*self.state.get()).execution.dispatch_read_depth() } != 1
            || unsafe { !(&*self.entry_scopes.get()).is_empty() }
        {
            return BUSY;
        }
        let Some((context, visitor)) = (unsafe { valid_request(request) }) else {
            return INVALID;
        };
        self.observation.permit.set(false);
        let Some(epoch) = self.observation.epoch.get().checked_add(1) else {
            return INVALID;
        };
        self.observation.epoch.set(epoch);
        self.observation.active.set(true);
        let _phase = Reset(&self.observation.active);

        // This borrow ends before visitor code is invoked. All arrays own their
        // native scratch; no references into RuntimeState cross the callback.
        let (allocations, roots, edges, providers, deferred_count, deferred_bytes, precise) = unsafe {
            let state = &*self.state.get();
            let mut allocations = state.heap.storage_allocations();
            let mut edges = Vec::new();
            for allocation in &mut allocations {
                let object = allocation.object;
                if !allocation.behavior.is_null() {
                    edges.push(EdgeV1 {
                        from: object,
                        to: allocation.behavior,
                    });
                }
                if state.vtables.is_registered(object) {
                    allocation.flags |= CORE_PAYLOAD;
                    for to in state.vtables.strong_edges(object) {
                        if state.heap.contains(to) {
                            edges.push(EdgeV1 { from: object, to });
                        }
                    }
                } else if !state.symbols.c_name_of(object).is_null() {
                    allocation.flags |= CORE_PAYLOAD;
                }
                if state.vtables.has_layout(allocation.behavior) {
                    allocation.flags |= HAS_GC_LAYOUT;
                }
                allocation.trace_hook = state.vtables.trace_hook(allocation.behavior);
            }
            let mut roots = floor_roots(state);
            #[cfg(test)]
            if let Some(owner) = &*self.test_integer.get() {
                roots.extend(owner.instance.identities());
            }
            roots.retain(|&root| state.heap.contains(root));
            roots.sort_unstable_by_key(|root| root.addr());
            roots.dedup();
            let physical_bytes: usize =
                allocations.iter().map(|entry| entry.allocation_bytes).sum();
            let deferred_count = state.heap.allocation_count() - allocations.len();
            let deferred_bytes = state.heap.allocated_bytes() - physical_bytes;
            (
                allocations,
                roots,
                edges,
                state.root_registry.snapshot(),
                deferred_count,
                deferred_bytes,
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
