//! Runtime roots and mark admission over the completed native Trace graph.
//! The legacy graph adapter remains for minimal unit/legacy initialization.
use super::*;
use crate::gc::{emit_candidate, emit_native_root_range, invoke_with_mark_callback};
use birnam_abi::storage::OK;
use birnam_abi::trace_live::{ApiV1, SinkV1, SourceV1, VERSION};
use core::ffi::c_void;

struct Source<'a> {
    runtime: &'a Runtime,
    native_range: (usize, usize),
    initial_roots: &'a [Oop],
    id: Option<super::id_storage::Storage>,
}

pub(super) fn emit_upper_roots(runtime: &Runtime, initial: &[Oop], sink: &SinkV1) -> i32 {
    for root in initial { emit_candidate(sink, root.addr()); }
    runtime.dispatch_cache.clear();
    #[cfg(all(test, target_os = "macos"))]
    runtime.read_reuse_gc_epoch.set(runtime.read_reuse_gc_epoch.get().saturating_add(1));
    let roots = unsafe { upper_roots(&*runtime.state.get()) };
    for root in roots { emit_candidate(sink, root.addr()); }
    OK
}
unsafe extern "C" fn native_upper_roots(context: *mut c_void, sink: *const SinkV1) -> i32 {
    let source = unsafe { &*context.cast::<Source<'_>>() };
    // The installed collector extension supplies Runtime roots on every call.
    for root in source.initial_roots { emit_candidate(unsafe { &*sink }, root.addr()); }
    OK
}

unsafe extern "C" fn roots(context: *mut c_void, sink: *const SinkV1) -> i32 {
    let (source, sink) = unsafe { (&*context.cast::<Source<'_>>(), &*sink) };
    let runtime = source.runtime;
    if let Some(owner) = &runtime.collection_owner {
        unsafe { (*owner.0.stack_factory).scan.unwrap()(sink, source.native_range.0, source.native_range.1) };
    } else {
        emit_native_root_range(sink, source.native_range.0, source.native_range.1);
    }
    for root in source.initial_roots {
        emit_candidate(sink, root.addr());
    }
    // Preserve invalidation after native admission and before foreign providers.
    runtime.dispatch_cache.clear();
    #[cfg(all(test, target_os = "macos"))]
    runtime
        .read_reuse_gc_epoch
        .set(runtime.read_reuse_gc_epoch.get().saturating_add(1));
    let graph = runtime.trace_graph;
    let (roots, providers) = unsafe {
        let state = &*runtime.state.get();
        (
            if source.id.is_some() {
                upper_roots(state)
            } else {
                floor_roots(state)
            },
            if graph.is_none() { state.root_registry.snapshot() } else { Vec::new() },
        )
    };
    for root in roots {
        emit_candidate(sink, root.addr());
    }
    if let Some(id) = source.id {
        if graph.is_none() {
            for root in id.roots() { emit_candidate(sink, root.addr()); }
        }
        // Permanent allocations are not admitted to the tracing worklist. Their
        // Id references and upper behavior owners must still seed the walk.
        for (object, _, _) in id.allocations() {
            let permanent = unsafe { (&*runtime.state.get()).heap.permanence_of(object) }
                == Some(Permanence::Permanent);
            if !permanent {
                continue;
            }
            if let Some(graph) = graph {
                let status = unsafe { (graph.edges)(graph.context, object, birnam_abi::trace_graph::CORE_ONLY, sink) };
                if status != OK { return status; }
                continue;
            }
            let mut facts = id
                .references(object)
                .expect("permanent Id allocation disappeared");
            if facts.kind == 1 {
                let owner = unsafe { (&*runtime.state.get()).vtables.owner_of(object) };
                facts.edges.insert(2, owner);
            }
            for edge in facts.edges {
                emit_candidate(sink, edge.addr());
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
    if let Some(graph) = graph {
        let status = unsafe { (graph.roots)(graph.context, sink) };
        if status != OK { return status; }
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
    let object = unsafe { (&*runtime.state.get()).heap.object_at_address(address) }
        .unwrap_or_else(|| boundary::fatal_invariant("marked GC block disappeared"));
    if let Some(graph) = runtime.trace_graph {
        let flags = if unsafe { (&*runtime.state.get()).metadata.precise() } { birnam_abi::trace_graph::PRECISE } else { 0 };
        return unsafe { (graph.edges)(graph.context, object, flags, sink) };
    }
    let (vtable, registered, symbol) = if let Some(id) = source.id {
        let mut facts = id
            .references(object)
            .expect("marked Id allocation disappeared");
        let vtable = facts.edges[0];
        if facts.kind == 1 {
            let owner = unsafe { (&*runtime.state.get()).vtables.owner_of(object) };
            facts.edges.insert(2, owner);
        }
        for edge in facts.edges {
            emit_candidate(sink, edge.addr());
        }
        (vtable, facts.kind == 1, facts.kind == 2)
    } else {
        // Minimal unit harnesses have no native carrier. Production always uses
        // the message-based branch above.
        let (vtable, registered, edges) = unsafe {
            let state = &*runtime.state.get();
            (
                state.heap.vtable_of(object).unwrap(),
                state.vtables.is_registered(object),
                state.vtables.strong_edges(object),
            )
        };
        emit_candidate(sink, vtable.addr());
        for edge in edges {
            emit_candidate(sink, edge.addr());
        }
        (vtable, registered, false)
    };
    if !registered {
        let (words, precise) = unsafe {
            let state = &*runtime.state.get();
            (
                state.heap.payload_words(address).unwrap(),
                state.metadata.precise() && state.vtables.has_layout(vtable),
            )
        };
        for index in 0..if symbol { 0 } else { words.len() } {
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
        let id = self.id_storage;
        let mut context = Source {
            runtime: self,
            id,
            native_range,
            initial_roots,
        };
        if let Some(owner) = &self.collection_owner {
            let collector = unsafe { *owner.0.collector };
            let input = birnam_abi::collector::InputV1 {
                start: native_range.0, bottom: native_range.1, context: (&raw mut context).cast(),
                roots: Some(native_upper_roots), retirement: core::ptr::null(),
            };
            let status = unsafe { (collector.trace)(collector.context, &input) };
            boundary::require_invariant(status == OK, "GC tracer failed before sweep");
            return;
        }
        let trace = self
            .trace
            .get()
            .unwrap_or_else(|| boundary::fatal_invariant("GC tracer dependency not mounted"));
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
