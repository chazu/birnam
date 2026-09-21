//! Mark records and collection controls initialized from the existing Id heap.
use birnam_abi::{
    Oop, allocation_budget, borrowed_slice,
    collection_state::{
        self as abi, AccessV1, MARKED, MISSING, OwnedV2, PERMANENT, RecordV1, RecordsV1, ViewV1,
    },
    gc_settings,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
    native_stack,
};
use core::{cell::Cell, ffi::c_void, ptr};
use birnam_abi::address_map::AddressMap;

struct Records {
    api: RecordsV1,
    entries: AddressMap<RecordV1>,
    snapshot: Vec<RecordV1>,
    thread: std::thread::ThreadId,
}
unsafe fn records<'a>(context: *mut c_void) -> &'a mut Records {
    let records = unsafe { &mut *context.cast::<Records>() };
    assert_eq!(
        records.thread,
        std::thread::current().id(),
        "collection records used off owner thread"
    );
    records
}
unsafe extern "C" fn insert(context: *mut c_void, object: Oop, flags: u32) -> i32 {
    if object.is_null() || flags & !(MARKED | PERMANENT) != 0 {
        return 0;
    }
    let records = unsafe { records(context) };
    if records.entries.contains_key(&object.addr()) {
        return 0;
    }
    records.entries.insert(
        object.addr(),
        RecordV1 {
            object,
            flags,
            reserved: 0,
        },
    );
    1
}
unsafe extern "C" fn get(context: *const c_void, address: usize) -> u32 {
    let records = unsafe { &*context.cast::<Records>() };
    assert_eq!(
        records.thread,
        std::thread::current().id(),
        "collection records used off owner thread"
    );
    records
        .entries
        .get(&address)
        .map_or(MISSING, |record| record.flags)
}
unsafe extern "C" fn retain(context: *mut c_void, address: usize) -> i32 {
    let Some(record) = (unsafe { records(context) }).entries.get_mut(&address) else {
        return 0;
    };
    record.flags |= PERMANENT;
    1
}
unsafe extern "C" fn mark(context: *mut c_void, address: usize) -> i32 {
    let Some(record) = (unsafe { records(context) }).entries.get_mut(&address) else {
        return 0;
    };
    if record.flags & (MARKED | PERMANENT) != 0 {
        return 0;
    }
    record.flags |= MARKED;
    1
}
unsafe extern "C" fn clear(context: *mut c_void) {
    for record in unsafe { records(context) }.entries.values_mut() {
        record.flags &= !MARKED;
    }
}
unsafe extern "C" fn snapshot(context: *mut c_void) -> ViewV1 {
    let records = unsafe { records(context) };
    records.snapshot.clear();
    records.snapshot.extend(records.entries.values().copied());
    ViewV1 {
        entries: records.snapshot.as_ptr(),
        count: records.snapshot.len(),
    }
}
unsafe extern "C" fn remove_dead(context: *mut c_void, address: usize) -> i32 {
    let records = unsafe { records(context) };
    if !records
        .entries
        .get(&address)
        .is_some_and(|record| record.flags & (MARKED | PERMANENT) == 0)
    {
        return 0;
    }
    records.entries.remove(&address);
    1
}
impl Records {
    fn new() -> Box<Self> {
        let mut records = Box::new(Self {
            api: RecordsV1 {
                version: 1,
                byte_size: size_of::<RecordsV1>(),
                context: ptr::null_mut(),
                insert,
                get,
                retain,
                mark,
                clear,
                snapshot,
                remove_dead,
            },
            entries: AddressMap::default(),
            snapshot: Vec::new(),
            thread: std::thread::current().id(),
        });
        records.api.context = ptr::from_mut(records.as_mut()).cast();
        records
    }
}
struct Policy {
    access: Cell<AccessV1>,
    thread: Box<std::thread::ThreadId>,
}
unsafe extern "C" fn early_admit(context: *mut c_void) {
    assert_eq!(
        unsafe { &*context.cast::<std::thread::ThreadId>() },
        &std::thread::current().id(),
        "collection controls used off owner thread"
    );
}
unsafe extern "C" fn admit_settings(context: *mut c_void) {
    let access = unsafe { (*context.cast::<Policy>()).access.get() };
    unsafe { (access.settings)(access.context) };
}
unsafe extern "C" fn admit_stack(context: *mut c_void) {
    let access = unsafe { (*context.cast::<Policy>()).access.get() };
    unsafe { (access.stack)(access.context) };
}
pub(super) struct State {
    owned: OwnedV2,
    records: Box<Records>,
    policy: Box<Policy>,
    collector: Option<Box<crate::collector::Collector>>,
}
unsafe extern "C" fn set_access(context: *mut c_void, access: AccessV1) {
    let state = unsafe { &*context.cast::<State>() };
    assert_eq!(*state.policy.thread, std::thread::current().id());
    state.policy.access.set(access);
}
unsafe extern "C" fn destroy(context: *mut c_void) {
    unsafe { drop(Box::from_raw(context.cast::<State>())) };
}
impl Drop for State {
    fn drop(&mut self) {
        let owned = self.owned;
        unsafe {
            (*owned.stack_factory).destroy.unwrap()(owned.stack);
            (*owned.settings_factory).destroy.unwrap()(owned.settings);
            (*owned.budget_factory).destroy.unwrap()(owned.budget_context);
            (*owned.phase_factory).destroy.unwrap()(owned.phase_context);
        }
    }
}
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
unsafe extern "C" fn allocation(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([object, bytes, identity]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let (Some(object), Some(bytes), Some(identity)) =
        (object.as_object(), bytes.as_word(), identity.as_word())
    else {
        return invalid();
    };
    if identity == 0 {
        return invalid();
    }
    let Some(total) = bytes
        .checked_add(size_of::<Oop>())
        .and_then(|bytes| bytes.checked_add(7))
        .map(|bytes| bytes & !7)
    else {
        return invalid();
    };
    let owner = unsafe { (*context.cast::<State>()).owned };
    let records = unsafe { *owner.records };
    if unsafe { (records.insert)(records.context, object, 0) } != 1 {
        return invalid();
    }
    if unsafe { (*owner.budget_factory).charge.unwrap()(owner.budget_context, total) } != 1 {
        return invalid();
    }
    Outcome::loaded(Operand::unit())
}
unsafe extern "C" fn initialize(
    id: *const Protocol,
    args: *const Operand,
    count: usize,
) -> Outcome {
    if !super::aligned(id) {
        return invalid();
    }
    let id = unsafe { *id };
    if id.version != 1 || id.byte_size as usize != size_of::<Protocol>() || id.message.is_none() {
        return invalid();
    }
    let Some([stack, settings, budget, graph, lifetime, model]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let (Some(stack), Some(settings), Some(budget)) =
        (stack.as_word(), settings.as_word(), budget.as_word())
    else {
        return invalid();
    };
    let (stack, settings, budget) = (
        stack as *const native_stack::FactoryV4,
        settings as *const gc_settings::FactoryV1,
        budget as *const allocation_budget::FactoryV1,
    );
    if !unsafe {
        native_stack::valid_factory(stack)
            && gc_settings::valid_factory(settings)
            && allocation_budget::valid(budget)
    } {
        return invalid();
    }
    let (Some(graph), Some(lifetime), Some(model)) = (graph.as_word(), lifetime.as_word(), model.as_word()) else { return invalid(); };
    let graph = graph as *const birnam_abi::trace_graph::GraphV2;
    let lifetime = lifetime as *const birnam_abi::lifetime::OwnedV2;
    let model = model as *const birnam_abi::object_model::OwnedV1;
    if !unsafe { birnam_abi::trace_graph::valid(graph) && birnam_abi::lifetime::valid(lifetime) && birnam_abi::object_model::valid(model) } { return invalid(); }
    let thread = Box::new(std::thread::current().id());
    let access = AccessV1 {
        context: ptr::from_ref(thread.as_ref()).cast_mut().cast(),
        settings: early_admit,
        stack: early_admit,
    };
    let mut policy = Box::new(Policy {
        access: Cell::new(access),
        thread,
    });
    let context = ptr::from_mut(policy.as_mut()).cast();
    let stack_floor = native_stack::FloorV1 {
        version: 1,
        byte_size: size_of::<native_stack::FloorV1>(),
        context,
        admit: Some(admit_stack),
    };
    let settings_floor = gc_settings::FloorV1 {
        version: 1,
        byte_size: size_of::<gc_settings::FloorV1>(),
        context,
        admit: Some(admit_settings),
    };
    let phase = unsafe { crate::phase::collection_phase_factory() };
    let mut state = Box::new(State {
        owned: OwnedV2 {
            version: 2,
            byte_size: size_of::<OwnedV2>(),
            context: ptr::null_mut(),
            records: ptr::null(),
            stack: unsafe { (*stack).create.unwrap()(&stack_floor) },
            stack_factory: stack,
            settings: unsafe { (*settings).create.unwrap()(&settings_floor) },
            settings_factory: settings,
            budget_context: unsafe { (*budget).create.unwrap()() },
            budget_factory: budget,
            phase_context: unsafe { (*phase).create.unwrap()() },
            phase_factory: phase,
            set_access,
            destroy,
            collect: crate::birnam_collect_v1,
            collector: ptr::null(),
        },
        collector: None,
        records: Records::new(),
        policy,
    });
    assert!(
        !state.owned.stack.is_null()
            && !state.owned.settings.is_null()
            && !state.owned.budget_context.is_null()
            && !state.owned.phase_context.is_null(),
        "collection state construction failed"
    );
    state.owned.context = ptr::from_mut(state.as_mut()).cast();
    state.owned.records = ptr::from_ref(&state.records.api);
    let name = Operand::bytes(b"allocationsDo:");
    let selector = unsafe { id.message.unwrap()(id.context, id.receiver, id.intern, &name, 1) };
    let Some(selector) = selector
        .value
        .as_object()
        .filter(|_| selector.status == native::LOADED)
    else {
        return invalid();
    };
    let visitor = Operand::entry(NativeEntry::new(allocation, state.owned.context));
    let answer = unsafe { id.message.unwrap()(id.context, id.receiver, selector, &visitor, 1) };
    if answer.status != native::LOADED {
        return answer;
    }
    let collector = match unsafe { crate::collector::Collector::new(id, state.owned, *graph, *lifetime, *model) } {
        Ok(collector) => collector, Err(error) => return error,
    };
    state.owned.collector = ptr::from_ref(&collector.api);
    state.collector = Some(collector);
    let owned = ptr::from_ref(&state.owned);
    assert!(unsafe { abi::valid(owned) });
    let _ = Box::into_raw(state);
    Outcome::loaded(Operand::word(owned as usize))
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: 1,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};

pub(super) unsafe fn object_at(context: *mut c_void, address: usize) -> Option<Oop> {
    unsafe { records(context) }.entries.get(&address).map(|record| record.object)
}
