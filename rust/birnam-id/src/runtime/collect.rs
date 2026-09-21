//! Owner operations consumed by the loaded collection coordinator.
use super::*;
use crate::heap::DetachedBlock;
use birnam_abi::collect::{ApiV1, DeadV1, DetachedV1, SourceV1, VERSION};
use birnam_abi::storage::OK;
use core::ffi::c_void;

struct Source<'a> {
    runtime: &'a Runtime,
    native_range: (usize, usize),
    dead_bindings: Vec<(Oop, Vec<MethodAddress>)>,
    dead: Vec<usize>,
    detached: Option<Detached>,
}

enum Detached {
    Local(DetachedBlock),
    Native {
        object: Oop,
        finalizer: Option<crate::abi::Finalizer>,
        release: crate::abi::id_protocol::NativeEntry,
    },
}
impl Detached {
    fn object(&self) -> Oop {
        match self {
            Self::Local(block) => block.object(),
            Self::Native { object, .. } => *object,
        }
    }
    fn finalizer(&self) -> Option<crate::abi::Finalizer> {
        match self {
            Self::Local(block) => block.finalizer(),
            Self::Native { finalizer, .. } => *finalizer,
        }
    }
    fn reclaim(self) {
        match self {
            Self::Local(block) => block.reclaim(),
            Self::Native { release, .. } => super::id_storage::Storage::release(release),
        }
    }
}

unsafe extern "C" fn phase(context: *mut c_void, phase: u32) {
    let source = unsafe { &*context.cast::<Source<'_>>() };
    source.runtime.collection_phase.set_raw(phase);
}

#[cfg(test)]
pub(super) fn assign_phase_for_test(runtime: &Runtime, code: u32) {
    let mut source = Source {
        runtime,
        native_range: (0, 0),
        dead_bindings: Vec::new(),
        dead: Vec::new(),
        detached: None,
    };
    unsafe { phase((&raw mut source).cast(), code) };
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
                    let string_cleanup = runtime.string_cleanup_matches_runtime(finalizer)
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
    let native_dead = unsafe {
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
        if source.runtime.collection_owner.is_some() || source.runtime.id_storage.is_some() {
            let dead = state.heap.dead_collectable_addresses();
            if source.runtime.collection_owner.is_none() { state.vtables.retire_metadata(&dead); }
            state
                .block_vtable_catalog
                .retain(|entry| !dead.contains(&entry.vtable.addr()));
            source.runtime.collection_owner.is_none().then_some(dead)
        } else {
            let dead_vtables = state.vtables.remove_dead_collectable(&state.heap);
            state
                .block_vtable_catalog
                .retain(|entry| !dead_vtables.contains(&entry.vtable));
            if state.symbol_compaction_enabled {
                state.vtables.remove_dead_weak_bindings(&state.heap);
                state
                    .symbols
                    .remove_objects(&state.heap.dead_collectable_addresses());
            }
            None
        }
    };
    if let Some(dead) = native_dead {
        source.runtime.id_storage.unwrap().retire_indexes(&dead);
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
    let detached = if let Some(id) = source.runtime.id_storage {
        // Retain the actual allocation pointer for the finalizer, rather than
        // reconstructing pointer provenance from the integer registry key.
        let object = unsafe { (&*source.runtime.state.get()).heap.object_at_address(address) }
            .unwrap_or_else(|| boundary::fatal_invariant("dead Id allocation disappeared"));
        let finalizer =
            unsafe { (&mut (*source.runtime.state.get()).heap).detach_dead_metadata(address) }
                .unwrap_or_else(|| {
                    boundary::fatal_invariant("dead GC metadata could not be detached")
                });
        let release = id.detach(address).unwrap_or_else(|| {
            boundary::fatal_invariant("dead Id allocation could not be detached")
        });
        Detached::Native {
            object,
            finalizer: finalizer.callback,
            release,
        }
    } else {
        Detached::Local(
            unsafe { (&mut (*source.runtime.state.get()).heap).detach_dead_collectable(address) }
                .unwrap_or_else(|| {
                    boundary::fatal_invariant("dead GC block could not be detached")
                }),
        )
    };
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

unsafe extern "C" fn upper_roots(context: *mut c_void, sink: *const birnam_abi::trace_live::SinkV1) -> i32 {
    let source = unsafe { &*context.cast::<Source<'_>>() };
    super::tracing::emit_upper_roots(source.runtime, &[], unsafe { &*sink })
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
        if let Some(owner) = &self.collection_owner {
            let collector = unsafe { *owner.0.collector };
            let retirement = birnam_abi::collector::RetirementV1 {
                context: (&raw mut context).cast(), inspect: inspect_retirement,
                settle: settle_deferred, retire: retire_metadata, reclaim: reclaim_deferred, prune,
            };
            let input = birnam_abi::collector::InputV1 {
                start: root_start, bottom: stack_bottom, context: (&raw mut context).cast(),
                roots: Some(upper_roots), retirement: &retirement,
            };
            let mut reclaimed = 0;
            let status = unsafe { (collector.collect)(collector.context, &input, &mut reclaimed) };
            boundary::require_invariant(status == OK, "GC coordinator failed");
            return reclaimed;
        }
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
