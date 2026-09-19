//! Owner operations consumed by the loaded collection coordinator.
use super::*;
use crate::heap::DetachedBlock;
use birnam_abi::collect::{
    ApiV1, DeadV1, DetachedV1, FINALIZING, IDLE, MARKING, SourceV1, VERSION,
};
use birnam_abi::storage::OK;
use core::ffi::c_void;

struct Source<'a> {
    runtime: &'a Runtime,
    native_range: (usize, usize),
    dead_bindings: Vec<(Oop, Vec<MethodAddress>)>,
    dead: Vec<usize>,
    detached: Option<DetachedBlock>,
}

unsafe extern "C" fn phase(context: *mut c_void, phase: u32) {
    let source = unsafe { &*context.cast::<Source<'_>>() };
    source.runtime.collection_phase.set(match phase {
        IDLE => CollectionPhase::Idle,
        MARKING => CollectionPhase::Marking,
        FINALIZING => CollectionPhase::Finalizing,
        _ => boundary::fatal_invariant("invalid collection phase"),
    });
}

unsafe extern "C" fn clear_marks(context: *mut c_void) {
    let source = unsafe { &*context.cast::<Source<'_>>() };
    unsafe { (&mut (*source.runtime.state.get()).heap).clear_marks() };
}

unsafe extern "C" fn trace(context: *mut c_void) -> i32 {
    let source = unsafe { &*context.cast::<Source<'_>>() };
    source.runtime.run_mark_phase(source.native_range, &[]);
    OK
}

unsafe extern "C" fn inspect_retirement(context: *mut c_void) -> i32 {
    let source = unsafe { &mut *context.cast::<Source<'_>>() };
    let runtime = source.runtime;
    // Capture once, before deferred settlement, exactly as before extraction.
    unsafe {
        let state = &mut *runtime.state.get();
        source.dead_bindings = state.vtables.dead_collectable_bindings(&state.heap);
        let required = state.heap.has_dead_allocations()
            && (!source.dead_bindings.is_empty()
                || state.heap.dead_finalizers().any(|(object, finalizer)| {
                    // Only the exact String cleanup on its owning mounting
                    // and owned storage cannot observe the physical sweep.
                    // It still runs normally below. Wrappers, replacement
                    // finalizers and missing owners retain ordinary settlement.
                    let string_cleanup =
                        runtime.string_cleanup_matches_runtime(finalizer)
                            && runtime.string_bytes(object).is_some();
                    !string_cleanup
                        && !state
                            .execution
                            .inert_machine_finalizer(&state.heap, object, finalizer)
                }));
        i32::from(required)
    }
}

unsafe extern "C" fn settle_deferred(context: *mut c_void) {
    let source = unsafe { &*context.cast::<Source<'_>>() };
    // Realize already charged identities without new logical allocation or GC.
    unsafe { (&mut (*source.runtime.state.get()).heap).settle_dead_allocations() };
}

unsafe extern "C" fn retire_metadata(context: *mut c_void) {
    let source = unsafe { &*context.cast::<Source<'_>>() };
    // No foreign call occurs while the current metadata owners are borrowed.
    unsafe {
        let state = &mut *source.runtime.state.get();
        for (_, methods) in &source.dead_bindings {
            for method in methods {
                let removed = if block_registration_for_method(state, *method).is_some() {
                    state
                        .execution
                        .note_collected_block_binding_removed(*method)
                } else {
                    state.execution.note_direct_binding_removed(*method)
                };
                removed.unwrap_or_else(|_| {
                    boundary::fatal_invariant("dead vtable binding retirement accounting failed")
                });
            }
        }
        let dead_vtables = state.vtables.remove_dead_collectable(&state.heap);
        state
            .block_vtable_catalog
            .retain(|entry| !dead_vtables.contains(&entry.vtable));
        if state.symbol_compaction_enabled {
            state.vtables.remove_dead_weak_bindings(&state.heap);
            state.symbols.remove_dead_collectable(&state.heap);
        }
    }
}

unsafe extern "C" fn snapshot_dead(context: *mut c_void, out: *mut DeadV1) {
    let source = unsafe { &mut *context.cast::<Source<'_>>() };
    source.dead = unsafe {
        (*source.runtime.state.get())
            .heap
            .dead_collectable_addresses()
    };
    unsafe {
        out.write(DeadV1 {
            addresses: source.dead.as_ptr(),
            count: source.dead.len(),
        })
    };
}

unsafe extern "C" fn detach(context: *mut c_void, address: usize, out: *mut DetachedV1) -> i32 {
    let source = unsafe { &mut *context.cast::<Source<'_>>() };
    boundary::require_invariant(source.detached.is_none(), "GC block still detached");
    let detached =
        unsafe { (&mut (*source.runtime.state.get()).heap).detach_dead_collectable(address) }
            .unwrap_or_else(|| boundary::fatal_invariant("dead GC block could not be detached"));
    unsafe {
        out.write(DetachedV1 {
            object: detached.object(),
            finalizer: detached.finalizer(),
        })
    };
    source.detached = Some(detached);
    // All state/heap and detached-owner borrows end before the driver calls the
    // current finalizer. Later dead objects remain registered until their turn.
    OK
}

unsafe extern "C" fn reclaim(context: *mut c_void, address: usize) -> i32 {
    let source = unsafe { &mut *context.cast::<Source<'_>>() };
    let detached = source
        .detached
        .take()
        .unwrap_or_else(|| boundary::fatal_invariant("GC block is not detached"));
    boundary::require_invariant(
        detached.object().addr() == address,
        "wrong detached GC block",
    );
    detached.reclaim();
    OK
}

unsafe extern "C" fn reclaim_deferred(context: *mut c_void) -> usize {
    let source = unsafe { &*context.cast::<Source<'_>>() };
    unsafe { (&mut (*source.runtime.state.get()).heap).reclaim_dead_allocations() }
}

unsafe extern "C" fn reset_accounting(context: *mut c_void) {
    let source = unsafe { &*context.cast::<Source<'_>>() };
    unsafe { (&mut (*source.runtime.state.get()).heap).reset_allocation_bytes() };
}

unsafe extern "C" fn prune(context: *mut c_void) {
    let source = unsafe { &*context.cast::<Source<'_>>() };
    source.runtime.native_tier.borrow_mut().prune();
}

impl Runtime {
    /// The host pins implementation code. Mounting is idempotent, never replacement.
    pub(crate) unsafe fn attach_collect(&self, api: *const ApiV1) -> bool {
        self.assert_owner();
        if self.collection_phase.get() != CollectionPhase::Idle
            || api.is_null()
            || !api.addr().is_multiple_of(align_of::<ApiV1>())
        {
            return false;
        }
        let api = unsafe { *api };
        if api.version != VERSION || api.byte_size != size_of::<ApiV1>() {
            return false;
        }
        let Some(collect) = api.collect else {
            return false;
        };
        if let Some(existing) = self.collect.get() {
            return core::ptr::fn_addr_eq(existing, collect);
        }
        self.collect.set(Some(collect));
        true
    }

    pub(super) fn gc_collect_from_native_range(
        &self,
        root_start: usize,
        stack_bottom: usize,
    ) -> usize {
        let collect = self
            .collect
            .get()
            .unwrap_or_else(|| boundary::fatal_invariant("GC coordinator dependency not mounted"));
        let _collection_guard = CollectionGuard {
            phase: &self.collection_phase,
        };
        let mut context = Source {
            runtime: self,
            native_range: (root_start, stack_bottom),
            dead_bindings: Vec::new(),
            dead: Vec::new(),
            detached: None,
        };
        let source = SourceV1 {
            version: VERSION,
            byte_size: size_of::<SourceV1>(),
            context: (&raw mut context).cast(),
            phase: Some(phase),
            clear_marks: Some(clear_marks),
            trace: Some(trace),
            inspect_retirement: Some(inspect_retirement),
            settle_deferred: Some(settle_deferred),
            retire_metadata: Some(retire_metadata),
            snapshot_dead: Some(snapshot_dead),
            detach: Some(detach),
            reclaim: Some(reclaim),
            reclaim_deferred: Some(reclaim_deferred),
            reset_accounting: Some(reset_accounting),
            prune: Some(prune),
        };
        let mut reclaimed = 0;
        let status = unsafe { collect(&source, &raw mut reclaimed) };
        boundary::require_invariant(status == OK, "GC coordinator failed");
        boundary::require_invariant(
            context.detached.is_none(),
            "GC coordinator left detached storage",
        );
        reclaimed
    }
}
