//! Floor-owned live-GC facts, consumed by the independently loaded tracer.
use super::*;
use crate::gc::{emit_candidate, emit_native_root_range, invoke_with_mark_callback};
use birnam_abi::storage::OK;
use birnam_abi::trace_live::{ApiV1, SinkV1, SourceV1, VERSION};
use core::ffi::c_void;

struct Source<'a> {
    runtime: &'a Runtime,
    native_range: (usize, usize),
    initial_roots: &'a [Oop],
}

unsafe extern "C" fn roots(context: *mut c_void, sink: *const SinkV1) -> i32 {
    let (source, sink) = unsafe { (&*context.cast::<Source<'_>>(), &*sink) };
    let runtime = source.runtime;
    emit_native_root_range(sink, source.native_range.0, source.native_range.1);
    for root in source.initial_roots {
        emit_candidate(sink, root.addr());
    }
    // Preserve invalidation after native admission and before foreign providers.
    runtime.dispatch_cache.clear();
    #[cfg(all(test, target_os = "macos"))]
    runtime
        .read_reuse_gc_epoch
        .set(runtime.read_reuse_gc_epoch.get().saturating_add(1));
    let (roots, providers) = unsafe {
        let state = &*runtime.state.get();
        (floor_roots(state), state.metadata.root_providers())
    };
    for root in roots {
        emit_candidate(sink, root.addr());
    }
    #[cfg(test)]
    {
        let roots = unsafe {
            (&*runtime.test_integer.get())
                .as_ref()
                .map(|owner| owner.instance.identities())
        };
        if let Some(roots) = roots {
            for root in roots {
                emit_candidate(sink, root.addr());
            }
        }
    }
    #[cfg(test)]
    {
        let roots = unsafe {
            (&*runtime.test_float.get())
                .as_ref()
                .map(|owner| owner.instance.identities())
        };
        if let Some(roots) = roots {
            for root in roots {
                emit_candidate(sink, root.addr());
            }
        }
    }
    #[cfg(test)]
    {
        let roots = unsafe {
            (&*runtime.test_word.get())
                .as_ref()
                .map(|owner| owner.instance.identities())
        };
        if let Some(roots) = roots {
            for root in roots {
                emit_candidate(sink, root.addr());
            }
        }
    }
    for provider in providers {
        invoke_with_mark_callback(sink, |mark| unsafe { provider(mark) });
    }
    OK
}

unsafe extern "C" fn admit(context: *mut c_void, address: usize) -> i32 {
    let source = unsafe { &*context.cast::<Source<'_>>() };
    // Admission touches only heap membership/mark metadata. No callback, lookup,
    // payload read, allocation, collection or ordinary Id operation occurs.
    i32::from(unsafe {
        (&mut (*source.runtime.state.get()).heap).mark_collectable_address(address)
    })
}

unsafe extern "C" fn edges(context: *mut c_void, address: usize, sink: *const SinkV1) -> i32 {
    let (source, sink) = unsafe { (&*context.cast::<Source<'_>>(), &*sink) };
    let runtime = source.runtime;
    let (object, vtable, registered) = unsafe {
        let state = &*runtime.state.get();
        let object = state
            .heap
            .object_at_address(address)
            .unwrap_or_else(|| boundary::fatal_invariant("marked GC block disappeared"));
        let vtable = state
            .heap
            .vtable_of(object)
            .unwrap_or_else(|| boundary::fatal_invariant("marked GC block lost its vtable"));
        (object, vtable, state.vtables.is_registered_address(address))
    };
    emit_candidate(sink, vtable.addr());
    if registered {
        let edges = unsafe { (&*runtime.state.get()).vtables.strong_edges(object) };
        for edge in edges {
            emit_candidate(sink, edge.addr());
        }
    } else {
        let (words, precise) = unsafe {
            let state = &*runtime.state.get();
            (
                state.heap.payload_words(address).unwrap(),
                state.metadata.precise() && state.vtables.has_layout(vtable),
            )
        };
        for index in 0..words.len() {
            let selected = !precise
                || unsafe { (&*runtime.state.get()).vtables.slot_kind(vtable, index) == SLOT_OOP };
            if selected {
                emit_candidate(sink, unsafe { words.read(index) });
            }
        }
        let hook = unsafe { (&*runtime.state.get()).vtables.trace_hook(vtable) };
        if let Some(hook) = hook {
            invoke_with_mark_callback(sink, |mark| unsafe { hook(object, mark) });
        }
    }
    OK
}

impl Runtime {
    /// Mount once. The host pins implementation code for this runtime's life.
    pub(crate) unsafe fn attach_trace(&self, api: *const ApiV1) -> bool {
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
        let Some(trace) = api.trace else {
            return false;
        };
        if let Some(existing) = self.trace.get() {
            return core::ptr::fn_addr_eq(existing, trace);
        }
        self.trace.set(Some(trace));
        true
    }

    pub(super) fn run_mark_phase(&self, native_range: (usize, usize), initial_roots: &[Oop]) {
        let trace = self
            .trace
            .get()
            .unwrap_or_else(|| boundary::fatal_invariant("GC tracer dependency not mounted"));
        let mut context = Source {
            runtime: self,
            native_range,
            initial_roots,
        };
        let source = SourceV1 {
            version: VERSION,
            byte_size: size_of::<SourceV1>(),
            context: (&raw mut context).cast(),
            roots: Some(roots),
            admit: Some(admit),
            edges: Some(edges),
        };
        let status = unsafe { trace(&source) };
        boundary::require_invariant(status == OK, "GC tracer failed before sweep");
    }
}

#[cfg(test)]
mod tests;
