//! Heap collection from completed owners; no Runtime or Execution dependency.
use birnam_abi::{
    Oop, borrowed_slice,
    collect::{self as coordination, DeadV1, DetachedV1},
    collection_state::{MARKED, MISSING, OwnedV2, PERMANENT, RecordV1},
    collector::{ApiV1, InputV1, RetirementV1},
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
    lifetime, object_model,
    storage::{BUSY, INVALID, OK, Status},
    trace_graph::{CORE_ONLY, GraphV1, PRECISE},
    trace_live::{SinkV1, SourceV1},
};
use core::{ffi::c_void, ptr};

pub(super) struct Collector {
    pub(super) api: ApiV1,
    state: OwnedV2,
    graph: GraphV1,
    lifetime: lifetime::OwnedV1,
    model: object_model::OwnedV1,
    receiver: Oop,
    allocations: NativeEntry,
    retire: NativeEntry,
    detach: NativeEntry,
    thread: std::thread::ThreadId,
}
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn loaded(answer: Outcome) -> Result<Operand, Status> {
    if answer.status == native::LOADED {
        Ok(answer.value)
    } else {
        Err(INVALID)
    }
}
impl Collector {
    pub(super) unsafe fn new(
        id: Protocol,
        state: OwnedV2,
        graph: GraphV1,
        lifetime: lifetime::OwnedV1,
        model: object_model::OwnedV1,
    ) -> Result<Box<Self>, Outcome> {
        let intern = |name: &[u8]| -> Result<Oop, Outcome> {
            let answer = unsafe {
                id.message.unwrap()(id.context, id.receiver, id.intern, &Operand::bytes(name), 1)
            };
            loaded(answer)
                .ok()
                .and_then(|value| value.as_object())
                .ok_or_else(invalid)
        };
        let lookup = intern(b"lookup")?;
        let select = |name| -> Result<NativeEntry, Outcome> {
            let selector = intern(name)?;
            let answer = unsafe {
                id.message.unwrap()(
                    id.context,
                    id.receiver,
                    lookup,
                    &Operand::object(selector),
                    1,
                )
            };
            loaded(answer)
                .ok()
                .and_then(|value| value.as_entry())
                .filter(|entry| entry.method.is_some())
                .ok_or_else(invalid)
        };
        let mut collector = Box::new(Self {
            api: ApiV1 {
                version: 1,
                byte_size: size_of::<ApiV1>(),
                context: ptr::null_mut(),
                trace,
                collect,
            },
            state,
            graph,
            lifetime,
            model,
            receiver: id.receiver,
            allocations: select(b"allocationsDo:")?,
            retire: select(b"retireIndexesFor:")?,
            detach: select(b"detachAllocationAt:")?,
            thread: std::thread::current().id(),
        });
        collector.api.context = ptr::from_mut(collector.as_mut()).cast();
        Ok(collector)
    }
    fn check(&self) {
        assert_eq!(
            self.thread,
            std::thread::current().id(),
            "collector used off owner thread"
        );
    }
    fn invoke(&self, entry: NativeEntry, args: &[Operand]) -> Result<Operand, Status> {
        loaded(unsafe {
            entry.method.unwrap()(entry.context, self.receiver, args.as_ptr(), args.len())
        })
    }
    fn records(&self) -> Vec<RecordV1> {
        let api = unsafe { *self.state.records };
        let view = unsafe { (api.snapshot)(api.context) };
        unsafe { borrowed_slice(view.entries, view.count) }
            .expect("invalid collection record view")
            .to_vec()
    }
    fn dead(&self) -> Vec<RecordV1> {
        self.records()
            .into_iter()
            .filter(|record| record.flags & (MARKED | PERMANENT) == 0)
            .collect()
    }
    fn reconcile(&self) -> Status {
        // Id releases its scoped registry borrow before each visitor invocation.
        unsafe extern "C" fn allocation(
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
            if identity == 0 {
                return invalid();
            }
            let collector = unsafe { &*context.cast::<Collector>() };
            let state = collector.state;
            let records = unsafe { *state.records };
            if unsafe { (records.get)(records.context, object.addr()) } == MISSING {
                let Some(bytes) = size
                    .checked_add(size_of::<Oop>())
                    .and_then(|value| value.checked_add(7))
                    .map(|value| value & !7)
                else {
                    return invalid();
                };
                if unsafe { (records.insert)(records.context, object, 0) } != 1
                    || unsafe {
                        (*state.budget_factory).charge.unwrap()(state.budget_context, bytes)
                    } != 1
                {
                    return invalid();
                }
            }
            Outcome::loaded(Operand::unit())
        }
        self.invoke(
            self.allocations,
            &[Operand::entry(NativeEntry::new(
                allocation,
                ptr::from_ref(self).cast_mut().cast(),
            ))],
        )
        .map_or_else(|status| status, |_| OK)
    }
}
struct Mark<'a> {
    collector: &'a Collector,
    input: InputV1,
}
unsafe extern "C" fn roots(context: *mut c_void, sink: *const SinkV1) -> Status {
    let source = unsafe { &*context.cast::<Mark<'_>>() };
    let collector = source.collector;
    unsafe {
        (*collector.state.stack_factory).scan.unwrap()(
            sink,
            source.input.start,
            source.input.bottom,
        )
    };
    if let Some(roots) = source.input.roots {
        let status = unsafe { roots(source.input.context, sink) };
        if status != OK {
            return status;
        }
    }
    // Permanently retained records are not admitted to the tracing worklist.
    for record in collector
        .records()
        .into_iter()
        .filter(|record| record.flags & PERMANENT != 0)
    {
        let graph = collector.graph;
        let status = unsafe { (graph.edges)(graph.context, record.object, CORE_ONLY, sink) };
        if status != OK {
            return status;
        }
    }
    unsafe { (collector.graph.roots)(collector.graph.context, sink) }
}
unsafe extern "C" fn admit(context: *mut c_void, address: usize) -> i32 {
    let source = unsafe { &*context.cast::<Mark<'_>>() };
    let records = unsafe { *source.collector.state.records };
    unsafe { (records.mark)(records.context, address) }
}
unsafe extern "C" fn edges(context: *mut c_void, address: usize, sink: *const SinkV1) -> Status {
    let source = unsafe { &*context.cast::<Mark<'_>>() };
    let collector = source.collector;
    let Some(object) =
        (unsafe { crate::state::object_at((*collector.state.records).context, address) })
    else {
        return INVALID;
    };
    // Read current precision after any preceding native hook/provider effects.
    let precise =
        unsafe { (*collector.state.settings_factory).precise.unwrap()(collector.state.settings) }
            != 0;
    unsafe {
        (collector.graph.edges)(
            collector.graph.context,
            object,
            if precise { PRECISE } else { 0 },
            sink,
        )
    }
}
unsafe extern "C" fn trace(context: *mut c_void, input: *const InputV1) -> Status {
    if !super::aligned(context.cast::<Collector>()) || !super::aligned(input) {
        return INVALID;
    }
    let collector = unsafe { &*context.cast::<Collector>() };
    collector.check();
    let status = collector.reconcile();
    if status != OK {
        return status;
    }
    let mut mark = Mark {
        collector,
        input: unsafe { *input },
    };
    let source = SourceV1 {
        version: 1,
        byte_size: size_of::<SourceV1>(),
        context: (&raw mut mark).cast(),
        roots: Some(roots),
        admit: Some(admit),
        edges: Some(edges),
    };
    unsafe { (collector.graph.trace)(&source) }
}
struct Collection<'a> {
    collector: &'a Collector,
    input: InputV1,
    retirement: Option<RetirementV1>,
    dead: Vec<usize>,
    detached: Option<(usize, NativeEntry)>,
}
unsafe extern "C" fn phase(context: *mut c_void, value: u32) {
    let state = unsafe { (*context.cast::<Collection<'_>>()).collector.state };
    assert_eq!(
        unsafe { (*state.phase_factory).set.unwrap()(state.phase_context, value) },
        1
    );
}
unsafe extern "C" fn clear(context: *mut c_void) {
    let records = unsafe { *(*context.cast::<Collection<'_>>()).collector.state.records };
    unsafe { (records.clear)(records.context) };
}
unsafe extern "C" fn mark(context: *mut c_void) -> Status {
    let source = unsafe { &*context.cast::<Collection<'_>>() };
    unsafe { trace(source.collector.api.context, &source.input) }
}
unsafe extern "C" fn inspect(context: *mut c_void) -> i32 {
    let extension = unsafe { (*context.cast::<Collection<'_>>()).retirement };
    extension.map_or(0, |extension| unsafe {
        (extension.inspect)(extension.context)
    })
}
unsafe extern "C" fn settle(context: *mut c_void) {
    let extension = unsafe { (*context.cast::<Collection<'_>>()).retirement }
        .expect("settlement requires upper retirement");
    unsafe { (extension.settle)(extension.context) };
}
unsafe extern "C" fn retire(context: *mut c_void) {
    let (collector, extension) = unsafe {
        let source = &*context.cast::<Collection<'_>>();
        (source.collector, source.retirement)
    };
    if let Some(extension) = extension {
        unsafe { (extension.retire)(extension.context) };
    }
    let dead: Vec<_> = collector
        .dead()
        .into_iter()
        .map(|record| record.object.addr())
        .collect();
    unsafe {
        (collector.lifetime.retire)(collector.lifetime.context, dead.as_ptr(), dead.len());
        collector.model.retire.unwrap()(collector.model.context, dead.as_ptr(), dead.len());
    }
    let bytes = unsafe {
        core::slice::from_raw_parts(
            dead.as_ptr().cast(),
            core::mem::size_of_val(dead.as_slice()),
        )
    };
    collector
        .invoke(collector.retire, &[Operand::bytes(bytes)])
        .expect("Id index retirement failed");
}
unsafe extern "C" fn snapshot(context: *mut c_void, out: *mut DeadV1) {
    let source = unsafe { &mut *context.cast::<Collection<'_>>() };
    source.dead = source
        .collector
        .dead()
        .into_iter()
        .map(|record| record.object.addr())
        .collect();
    unsafe {
        out.write(DeadV1 {
            addresses: source.dead.as_ptr(),
            count: source.dead.len(),
        })
    };
}
unsafe extern "C" fn detach(context: *mut c_void, address: usize, out: *mut DetachedV1) -> Status {
    let collector = unsafe { (*context.cast::<Collection<'_>>()).collector };
    assert!(unsafe { (*context.cast::<Collection<'_>>()).detached.is_none() });
    let records = unsafe { *collector.state.records };
    let Some(object) = (unsafe { crate::state::object_at(records.context, address) }) else {
        return INVALID;
    };
    if unsafe { (records.remove_dead)(records.context, address) } != 1 {
        return INVALID;
    }
    let lifetime = collector.lifetime;
    let finalizer = unsafe { (lifetime.finalizer)(lifetime.context, object) };
    unsafe {
        (lifetime.replace_finalizer)(
            lifetime.context,
            object,
            birnam_abi::finalizers::SlotV1::EMPTY,
        )
    };
    let release = match collector
        .invoke(collector.detach, &[Operand::word(address)])
        .ok()
        .and_then(|value| value.as_entry())
    {
        Some(release) => release,
        None => return INVALID,
    };
    unsafe {
        (*context.cast::<Collection<'_>>()).detached = Some((address, release));
        out.write(DetachedV1 {
            object,
            finalizer: finalizer.callback,
        });
    }
    OK
}
unsafe extern "C" fn reclaim(context: *mut c_void, address: usize) -> Status {
    let Some((expected, release)) =
        (unsafe { (*context.cast::<Collection<'_>>()).detached.take() })
    else {
        return INVALID;
    };
    if address != expected {
        return INVALID;
    }
    loaded(unsafe { release.method.unwrap()(release.context, ptr::null_mut(), ptr::null(), 0) })
        .map_or_else(|status| status, |_| OK)
}
unsafe extern "C" fn reclaim_upper(context: *mut c_void) -> usize {
    let extension = unsafe { (*context.cast::<Collection<'_>>()).retirement };
    extension.map_or(0, |extension| unsafe {
        (extension.reclaim)(extension.context)
    })
}
unsafe extern "C" fn reset(context: *mut c_void) {
    let state = unsafe { (*context.cast::<Collection<'_>>()).collector.state };
    unsafe { (*state.budget_factory).replace.unwrap()(state.budget_context, 0) };
}
unsafe extern "C" fn prune(context: *mut c_void) {
    let extension = unsafe { (*context.cast::<Collection<'_>>()).retirement };
    if let Some(extension) = extension {
        unsafe { (extension.prune)(extension.context) };
    }
}
unsafe extern "C" fn collect(
    context: *mut c_void,
    input: *const InputV1,
    out: *mut usize,
) -> Status {
    if !super::aligned(context.cast::<Collector>())
        || !super::aligned(input)
        || !super::aligned(out)
    {
        return INVALID;
    }
    let collector = unsafe { &*context.cast::<Collector>() };
    collector.check();
    let state = collector.state;
    if unsafe { (*state.phase_factory).read.unwrap()(state.phase_context) } != coordination::IDLE {
        return BUSY;
    }
    let input = unsafe { *input };
    if !input.retirement.is_null() && !super::aligned(input.retirement) {
        return INVALID;
    }
    let retirement = if input.retirement.is_null() {
        None
    } else {
        Some(unsafe { *input.retirement })
    };
    let mut collection = Collection {
        collector,
        input,
        retirement,
        dead: Vec::new(),
        detached: None,
    };
    let source = coordination::SourceV1 {
        version: 1,
        byte_size: size_of::<coordination::SourceV1>(),
        context: (&raw mut collection).cast(),
        phase: Some(phase),
        clear_marks: Some(clear),
        trace: Some(mark),
        inspect_retirement: Some(inspect),
        settle_deferred: Some(settle),
        retire_metadata: Some(retire),
        snapshot_dead: Some(snapshot),
        detach: Some(detach),
        reclaim: Some(reclaim),
        reclaim_deferred: Some(reclaim_upper),
        reset_accounting: Some(reset),
        prune: Some(prune),
    };
    unsafe { (state.collect)(&source, out) }
}
