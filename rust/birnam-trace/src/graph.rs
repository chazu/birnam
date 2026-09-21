//! Live roots and representation edges from completed lower owners.
//! This module has no Runtime, Machine, compiler, heap marks or sweep policy.
use birnam_abi::{
    MarkFn, Oop, SLOT_OOP, TraceHook, borrowed_slice,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
    lifetime, native_mark, object_model, root_registry,
    storage::{BUSY, INCOMPLETE, INVALID, OK, Status},
    trace_graph::{CORE_ONLY, GraphV1, PRECISE},
    trace_hooks,
    trace_live::SinkV1,
};
use core::{cell::Cell, ffi::c_void, ptr};
use std::collections::HashSet;

struct Graph {
    api: GraphV1,
    id: Protocol,
    roots: root_registry::OwnedV1,
    lifetime: lifetime::OwnedV1,
    model: object_model::OwnedV1,
    mark: native_mark::ApiV1,
    mark_context: *mut c_void,
    id_roots: NativeEntry,
    allocations: NativeEntry,
    references: NativeEntry,
    size: NativeEntry,
    thread: std::thread::ThreadId,
}
fn loaded(answer: Outcome) -> Result<Operand, Status> {
    if answer.status == native::LOADED {
        Ok(answer.value)
    } else {
        Err(INVALID)
    }
}
impl Graph {
    fn invoke(&self, entry: NativeEntry, args: &[Operand]) -> Result<Operand, Status> {
        loaded(unsafe {
            entry.method.unwrap()(entry.context, self.id.receiver, args.as_ptr(), args.len())
        })
    }
    fn references(&self, object: Oop) -> Result<(usize, Vec<Oop>), Status> {
        let mut edges = Vec::<Oop>::new();
        let kind = self
            .invoke(
                self.references,
                &[
                    Operand::object(object),
                    Operand::entry(NativeEntry::new(visit_object, (&raw mut edges).cast())),
                ],
            )?
            .as_word()
            .ok_or(INVALID)?;
        if kind > 2 || edges.len() < if kind == 1 { 2 } else { 1 } {
            return Err(INVALID);
        }
        Ok((kind, edges))
    }
    fn allocations(&self) -> Result<Vec<(Oop, usize, usize)>, Status> {
        let mut entries = Vec::<(Oop, usize, usize)>::new();
        self.invoke(
            self.allocations,
            &[Operand::entry(NativeEntry::new(
                visit_allocation,
                (&raw mut entries).cast(),
            ))],
        )?;
        Ok(entries)
    }
    fn hook(&self, behavior: Oop) -> Option<TraceHook> {
        struct Selection<'a> {
            graph: &'a Graph,
            visited: std::cell::RefCell<HashSet<usize>>,
        }
        unsafe extern "C" fn read(
            context: *const c_void,
            behavior: Oop,
            out: *mut trace_hooks::EntryV1,
        ) -> i32 {
            let selection = unsafe { &*context.cast::<Selection<'_>>() };
            if !selection.visited.borrow_mut().insert(behavior.addr()) {
                return 0;
            }
            let Ok((1, edges)) = selection.graph.references(behavior) else {
                return 0;
            };
            let owner = selection.graph.lifetime;
            unsafe {
                out.write(trace_hooks::EntryV1 {
                    parent: edges[1],
                    slot: (owner.hook)(owner.context, behavior),
                })
            };
            1
        }
        // Keep the existing hook-selection policy. A visited set bounds cycles
        // without asking Id for its private behavior registry's size.
        let selection = Selection {
            graph: self,
            visited: Default::default(),
        };
        let source = trace_hooks::SourceV1 {
            context: ptr::from_ref(&selection).cast(),
            budget: usize::MAX,
            read,
        };
        unsafe { (*self.lifetime.hook_factory).resolve.unwrap()(&source, behavior) }
    }
    fn callback(&self, sink: &SinkV1, operation: impl FnOnce(MarkFn)) -> Status {
        if ACTIVE.get().is_some() {
            return BUSY;
        }
        unsafe { self.mark.begin.unwrap()(self.mark_context, sink) };
        ACTIVE.set(Some((self.mark, self.mark_context)));
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                if let Some((api, context)) = ACTIVE.replace(None) {
                    unsafe { api.end.unwrap()(context) };
                }
            }
        }
        let _guard = Guard;
        operation(mark);
        OK
    }
}
thread_local! {
    static ACTIVE: Cell<Option<(native_mark::ApiV1, *mut c_void)>> = const { Cell::new(None) };
}
unsafe extern "C" fn mark(object: Oop) {
    let (api, context) = ACTIVE
        .get()
        .expect("Trace mark callback escaped its invocation");
    unsafe { api.mark.unwrap()(context, object) };
}
unsafe extern "C" fn visit_object(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([object]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(object) = object.as_object() else {
        return invalid();
    };
    unsafe { &mut *context.cast::<Vec<Oop>>() }.push(object);
    Outcome::loaded(Operand::unit())
}
unsafe extern "C" fn visit_allocation(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([object, size, identity]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let (Some(object), Some(size), Some(identity)) =
        (object.as_object(), size.as_word(), identity.as_word())
    else {
        return invalid();
    };
    unsafe { &mut *context.cast::<Vec<(Oop, usize, usize)>>() }.push((object, size, identity));
    Outcome::loaded(Operand::unit())
}
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
unsafe fn graph<'a>(context: *mut c_void) -> Result<&'a Graph, Status> {
    let graph = unsafe { &*context.cast::<Graph>() };
    if graph.thread != std::thread::current().id() {
        return Err(BUSY);
    }
    Ok(graph)
}
fn emit(sink: &SinkV1, object: Oop) -> Result<(), Status> {
    let status = unsafe { (sink.emit)(sink.context, object.addr()) };
    if status == OK { Ok(()) } else { Err(status) }
}
unsafe extern "C" fn roots(context: *mut c_void, sink: *const SinkV1) -> Status {
    (|| {
        let graph = unsafe { graph(context)? };
        if !super::aligned(sink) {
            return Err(INVALID);
        }
        let sink = unsafe { &*sink };
        let retained = unsafe { graph.roots.roots.unwrap()(graph.roots.context) };
        let mut objects = unsafe { borrowed_slice(retained.entries, retained.count) }
            .ok_or(INVALID)?
            .to_vec();
        let providers = unsafe { (*graph.roots.factory).snapshot.unwrap()(graph.roots.api) };
        graph.invoke(
            graph.id_roots,
            &[Operand::entry(NativeEntry::new(
                visit_object,
                (&raw mut objects).cast(),
            ))],
        )?;
        // All registry views and Id storage borrows end before sink/provider calls.
        for object in objects {
            emit(sink, object)?;
        }
        for provider in providers.entries[..providers.count].iter().flatten() {
            let status = graph.callback(sink, |mark| unsafe { provider(mark) });
            if status != OK {
                return Err(status);
            }
        }
        Ok(())
    })()
    .map_or_else(|error| error, |()| OK)
}
unsafe extern "C" fn edges(
    context: *mut c_void,
    object: Oop,
    flags: u32,
    sink: *const SinkV1,
) -> Status {
    (|| {
        let graph = unsafe { graph(context)? };
        if !super::aligned(sink) || flags & !(CORE_ONLY | PRECISE) != 0 {
            return Err(INVALID);
        }
        let sink = unsafe { &*sink };
        let (kind, mut edges) = graph.references(object)?;
        let behavior = edges[0];
        if kind == 1 {
            edges.insert(2, unsafe {
                graph.model.owner.unwrap()(graph.model.context, object)
            });
        }
        for edge in edges {
            emit(sink, edge)?;
        }
        if kind == 1 || flags & CORE_ONLY != 0 {
            return Ok(());
        }
        if kind != 2 {
            let size = graph
                .invoke(graph.size, &[Operand::object(object)])?
                .as_word()
                .ok_or(INVALID)?;
            // Id guarantees zeroed padding to a native-word boundary.
            let words =
                size.checked_add(size_of::<usize>() - 1).ok_or(INVALID)? / size_of::<usize>();
            let layout = unsafe { (graph.lifetime.layout)(graph.lifetime.context, behavior) };
            let factory = unsafe { *graph.lifetime.layout_factory };
            let precise =
                flags & PRECISE != 0 && unsafe { factory.has_layout.unwrap()(layout) } != 0;
            for index in 0..words {
                if !precise || unsafe { factory.slot_kind.unwrap()(layout, index) } == SLOT_OOP {
                    let address = unsafe { object.cast::<usize>().add(index).read() };
                    let status = unsafe { (sink.emit)(sink.context, address) };
                    if status != OK {
                        return Err(status);
                    }
                }
            }
        }
        if let Some(hook) = graph.hook(behavior) {
            let status = graph.callback(sink, |mark| unsafe { hook(object, mark) });
            if status != OK {
                return Err(status);
            }
        }
        Ok(())
    })()
    .map_or_else(|error| error, |()| OK)
}
unsafe extern "C" fn validate(context: *mut c_void) -> Status {
    (|| {
        let graph = unsafe { graph(context)? };
        let allocations = graph.allocations()?;
        let addresses: HashSet<_> = allocations.iter().map(|entry| entry.0.addr()).collect();
        let identities: HashSet<_> = allocations.iter().map(|entry| entry.2).collect();
        if addresses.len() != allocations.len()
            || identities.len() != allocations.len()
            || addresses.contains(&0)
            || identities.contains(&0)
        {
            return Err(INVALID);
        }
        for (object, size, _) in allocations {
            let (kind, edges) = graph.references(object)?;
            if edges
                .iter()
                .any(|edge| !edge.is_null() && !addresses.contains(&edge.addr()))
            {
                return Err(INVALID);
            }
            if kind == 1 {
                let owner = unsafe { graph.model.owner.unwrap()(graph.model.context, object) };
                if !owner.is_null() && !addresses.contains(&owner.addr()) {
                    return Err(INVALID);
                }
            } else if kind == 0 && size != 0 {
                let layout = unsafe { (graph.lifetime.layout)(graph.lifetime.context, edges[0]) };
                if unsafe { (*graph.lifetime.layout_factory).has_layout.unwrap()(layout) } == 0 {
                    return Err(INCOMPLETE);
                }
            }
        }
        Ok(())
    })()
    .map_or_else(|error| error, |()| OK)
}

unsafe extern "C" fn initialize(
    id: *const Protocol,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let result = (|| {
        if !super::aligned(id) {
            return Err(INVALID);
        }
        let id = unsafe { *id };
        if id.version != 1 || id.byte_size as usize != size_of::<Protocol>() || id.message.is_none()
        {
            return Err(INVALID);
        }
        let Some([roots, lifetime, model, mark]) = (unsafe { borrowed_slice(args, count) }) else {
            return Err(INVALID);
        };
        let roots = roots.as_word().ok_or(INVALID)? as *const root_registry::OwnedV1;
        let lifetime = lifetime.as_word().ok_or(INVALID)? as *const lifetime::OwnedV1;
        let model = model.as_word().ok_or(INVALID)? as *const object_model::OwnedV1;
        let mark = mark.as_word().ok_or(INVALID)? as *const native_mark::ApiV1;
        if !unsafe {
            root_registry::valid_owned(roots)
                && lifetime::valid(lifetime)
                && object_model::valid(model)
                && native_mark::valid(mark)
        } {
            return Err(INVALID);
        }
        let intern = |name: &[u8]| -> Result<Oop, Status> {
            loaded(unsafe {
                id.message.unwrap()(id.context, id.receiver, id.intern, &Operand::bytes(name), 1)
            })?
            .as_object()
            .ok_or(INVALID)
        };
        let lookup = intern(b"lookup")?;
        let select = |name: &[u8]| -> Result<NativeEntry, Status> {
            loaded(unsafe {
                id.message.unwrap()(
                    id.context,
                    id.receiver,
                    lookup,
                    &Operand::object(intern(name)?),
                    1,
                )
            })?
            .as_entry()
            .filter(|entry| entry.method.is_some())
            .ok_or(INVALID)
        };
        let mut graph = Box::new(Graph {
            api: GraphV1 {
                version: 1,
                byte_size: size_of::<GraphV1>(),
                context: ptr::null_mut(),
                roots: self::roots,
                edges,
                validate,
                trace: crate::live::birnam_trace_live_v1,
            },
            id,
            roots: unsafe { *roots },
            lifetime: unsafe { *lifetime },
            model: unsafe { *model },
            mark: unsafe { *mark },
            mark_context: ptr::null_mut(),
            id_roots: select(b"rootsDo:")?,
            allocations: select(b"allocationsDo:")?,
            references: select(b"referencesOf:do:")?,
            size: select(b"allocationSizeOf:")?,
            thread: std::thread::current().id(),
        });
        graph.mark_context = unsafe { graph.mark.create.unwrap()() };
        if graph.mark_context.is_null() {
            return Err(INVALID);
        }
        graph.api.context = ptr::from_mut(graph.as_mut()).cast();
        let api = ptr::from_ref(&graph.api);
        // Bootstrap pins these complete dependencies. Runtime later borrows the
        // same graph; no future provider is installed into this context.
        let _ = Box::into_raw(graph);
        Ok(Operand::word(api as usize))
    })();
    match result {
        Ok(value) => Outcome::loaded(value),
        Err(_) => invalid(),
    }
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: 1,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};
