//! Standalone experimental Id: no dependency on birnam-id or any runtime rung.
//! Storage, interned names and native method code are pinned until shutdown.
use birnam_observation_abi::{
    Allocation, BUSY, Edge, OBSERVE, OWNER_PAYLOAD, SEED_PAYLOAD, StorageView, record,
};
use birnam_seed_abi::*;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::thread::ThreadId;

const MAX_OBJECTS: usize = 8192;
const MAX_PAYLOAD: usize = 64 * 1024;
const MAX_DEPTH: usize = 128;
const MAX_NAME: usize = 4096;

struct DefaultBehavior {
    parent: Oop,
    methods: BTreeMap<Oop, Method>,
}
enum Kind {
    Ordinary,
    Symbol,
    Behavior(DefaultBehavior),
}
struct Block {
    words: Box<[usize]>,
    bytes: usize,
    kind: Kind,
}
struct Seed {
    owner: ThreadId,
    objects: RefCell<BTreeMap<Oop, Block>>,
    symbols: RefCell<BTreeMap<Vec<u8>, Oop>>,
    roots: Roots,
    selectors: Selectors,
    depth: Cell<usize>,
    observing: Cell<bool>,
}
impl Seed {
    fn owns(&self, oop: Oop) -> bool {
        self.objects.borrow().contains_key(&oop)
    }
    fn allocate(&self, behavior: Oop, bytes: usize, kind: Kind) -> Option<Oop> {
        if self.observing.get() || bytes > MAX_PAYLOAD || self.objects.borrow().len() >= MAX_OBJECTS
        {
            return None;
        }
        // Keep a unique, aligned payload address even for an empty object.
        let mut words = vec![0; 1 + bytes.div_ceil(size_of::<usize>()).max(1)].into_boxed_slice();
        words[0] = behavior;
        let oop = unsafe { words.as_mut_ptr().add(1) } as Oop;
        assert!(
            self.objects
                .borrow_mut()
                .insert(oop, Block { words, bytes, kind })
                .is_none()
        );
        Some(oop)
    }
    fn behavior(&self, oop: Oop) -> Oop {
        self.objects
            .borrow()
            .get(&oop)
            .map_or(0, |block| block.words[0])
    }
    fn reassociate(&self, oop: Oop, behavior: Oop) -> Status {
        if self.observing.get() {
            return BUSY;
        }
        if !self.owns(behavior) {
            return INVALID;
        }
        let mut objects = self.objects.borrow_mut();
        let Some(block) = objects.get_mut(&oop) else {
            return INVALID;
        };
        block.words[0] = behavior;
        OK
    }
    fn new_behavior(&self, parent: Oop) -> Option<Oop> {
        if parent != 0 && !self.owns(parent) {
            return None;
        }
        let meta = if parent == 0 {
            self.roots.vtable
        } else {
            self.behavior(parent)
        };
        self.allocate(
            meta,
            0,
            Kind::Behavior(DefaultBehavior {
                parent,
                methods: BTreeMap::new(),
            }),
        )
    }
    fn intern(&self, bytes: &[u8]) -> Option<Oop> {
        if self.observing.get() {
            return None;
        }
        if bytes.len() > MAX_NAME {
            return None;
        }
        if let Some(oop) = self.symbols.borrow().get(bytes) {
            return Some(*oop);
        }
        let oop = self.allocate(self.roots.symbol, 0, Kind::Symbol)?;
        self.symbols.borrow_mut().insert(bytes.to_vec(), oop);
        Some(oop)
    }
    fn is_symbol(&self, oop: Oop) -> bool {
        self.objects
            .borrow()
            .get(&oop)
            .is_some_and(|block| matches!(block.kind, Kind::Symbol))
    }
    fn direct(&self, behavior: Oop, selector: Oop) -> Result<(Option<Method>, Oop), Status> {
        let objects = self.objects.borrow();
        let Some(Block {
            kind: Kind::Behavior(record),
            ..
        }) = objects.get(&behavior)
        else {
            return Err(INVALID);
        };
        Ok((record.methods.get(&selector).copied(), record.parent))
    }
    fn install(&self, behavior: Oop, selector: Oop, method: Method) -> Status {
        if self.observing.get() {
            return BUSY;
        }
        if !self.is_symbol(selector) {
            return INVALID;
        }
        let mut objects = self.objects.borrow_mut();
        let Some(Block {
            kind: Kind::Behavior(record),
            ..
        }) = objects.get_mut(&behavior)
        else {
            return INVALID;
        };
        record.methods.insert(selector, method);
        OK
    }
}

/// No RefCell borrow may cross an externally supplied method invocation.
unsafe fn seed<'a>(api: *const SeedApi) -> &'a Seed {
    let seed = unsafe { &*((*api).context.cast::<Seed>()) };
    assert_eq!(
        seed.owner,
        std::thread::current().id(),
        "owner-thread-only experiment"
    );
    seed
}
struct Depth<'a>(&'a Cell<usize>);
impl Drop for Depth<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

unsafe extern "C" fn send(
    api: *const SeedApi,
    receiver: Oop,
    selector: Oop,
    args: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    if api.is_null() || out.is_null() || (count != 0 && args.is_null()) || count > 16 {
        return INVALID;
    }
    unsafe { out.write(Value::none()) };
    let seed = unsafe { seed(api) };
    if seed.observing.get() {
        return BUSY;
    }
    if !seed.owns(receiver) || !seed.is_symbol(selector) {
        return INVALID;
    }
    if seed.depth.get() >= MAX_DEPTH {
        return RECURSION;
    }
    seed.depth.set(seed.depth.get() + 1);
    let _depth = Depth(&seed.depth);
    let method = if receiver == seed.roots.vtable && selector == seed.selectors.lookup {
        // Terminate meta-recursion with a raw read of the CURRENT binding.
        match seed.direct(seed.roots.vtable, selector) {
            Ok((Some(method), _)) => method,
            Ok(_) => return UNBOUND,
            Err(e) => return e,
        }
    } else {
        let lookup_args = [Value::object(selector)];
        let result = unsafe {
            call(
                api,
                seed.behavior(receiver),
                seed.selectors.lookup,
                &lookup_args,
            )
        };
        match result {
            Ok(value) if value.kind == NONE => return UNBOUND,
            Ok(value) => match unsafe { value.as_method() } {
                Some(method) => method,
                None => return INVALID,
            },
            Err(e) => return e,
        }
    };
    // Native code/data are process-pinned. A copied Method remains valid even
    // when this call reenters and replaces its own dictionary binding.
    unsafe { (method.entry)(api, method.data, receiver, args, count, out) }
}
unsafe extern "C" fn behavior_of(api: *const SeedApi, object: Oop) -> Oop {
    unsafe { seed(api) }.behavior(object)
}
unsafe extern "C" fn reassociate(api: *const SeedApi, object: Oop, behavior: Oop) -> Status {
    unsafe { seed(api) }.reassociate(object, behavior)
}

unsafe fn operands<'a>(args: *const Value, count: usize) -> &'a [Value] {
    if count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(args, count) }
    }
}
unsafe fn answer(out: *mut Value, value: Value) -> Status {
    unsafe { out.write(value) };
    OK
}

unsafe extern "C" fn lookup(
    api: *const SeedApi,
    _: *mut c_void,
    receiver: Oop,
    args: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    let args = unsafe { operands(args, count) };
    if args.len() != 1 || args[0].kind != OBJECT {
        return INVALID;
    }
    let seed = unsafe { seed(api) };
    if !seed.is_symbol(args[0].a) {
        return INVALID;
    }
    // Copy dictionary result/parent BEFORE invoking an arbitrary parent.
    let (method, parent) = match seed.direct(receiver, args[0].a) {
        Ok(result) => result,
        Err(e) => return e,
    };
    if let Some(method) = method {
        return unsafe { answer(out, Value::method(method)) };
    }
    if parent == 0 {
        return unsafe { answer(out, Value::none()) };
    }
    // Parent can be an ordinary object with an entirely different resolver.
    unsafe {
        send(
            api,
            parent,
            seed.selectors.lookup,
            args.as_ptr(),
            args.len(),
            out,
        )
    }
}
unsafe extern "C" fn add_method(
    api: *const SeedApi,
    _: *mut c_void,
    receiver: Oop,
    args: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    if unsafe { seed(api) }.observing.get() {
        return BUSY;
    }
    let args = unsafe { operands(args, count) };
    if args.len() != 2 || args[0].kind != OBJECT {
        return INVALID;
    }
    let Some(method) = (unsafe { args[1].as_method() }) else {
        return INVALID;
    };
    let status = unsafe { seed(api) }.install(receiver, args[0].a, method);
    if status == OK {
        unsafe { answer(out, args[1]) }
    } else {
        status
    }
}
unsafe extern "C" fn remove_method(
    api: *const SeedApi,
    _: *mut c_void,
    receiver: Oop,
    args: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    if unsafe { seed(api) }.observing.get() {
        return BUSY;
    }
    let args = unsafe { operands(args, count) };
    if args.len() != 1 || args[0].kind != OBJECT {
        return INVALID;
    }
    let seed = unsafe { seed(api) };
    if !seed.is_symbol(args[0].a) {
        return INVALID;
    }
    let mut objects = seed.objects.borrow_mut();
    let Some(Block {
        kind: Kind::Behavior(record),
        ..
    }) = objects.get_mut(&receiver)
    else {
        return INVALID;
    };
    let old = record
        .methods
        .remove(&args[0].a)
        .map_or(Value::none(), Value::method);
    unsafe { answer(out, old) }
}
unsafe extern "C" fn allocate(
    api: *const SeedApi,
    _: *mut c_void,
    receiver: Oop,
    args: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    if unsafe { seed(api) }.observing.get() {
        return BUSY;
    }
    let args = unsafe { operands(args, count) };
    if args.len() != 1 || args[0].kind != WORD {
        return INVALID;
    }
    match unsafe { seed(api) }.allocate(receiver, args[0].a, Kind::Ordinary) {
        Some(oop) => unsafe { answer(out, Value::object(oop)) },
        None => INVALID,
    }
}
unsafe extern "C" fn delegated(
    api: *const SeedApi,
    _: *mut c_void,
    receiver: Oop,
    _: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    if unsafe { seed(api) }.observing.get() {
        return BUSY;
    }
    if count != 0 {
        return INVALID;
    }
    match unsafe { seed(api) }.new_behavior(receiver) {
        Some(oop) => unsafe { answer(out, Value::object(oop)) },
        None => INVALID,
    }
}
unsafe extern "C" fn intern_method(
    api: *const SeedApi,
    _: *mut c_void,
    _: Oop,
    args: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    if unsafe { seed(api) }.observing.get() {
        return BUSY;
    }
    let args = unsafe { operands(args, count) };
    if args.len() != 1
        || args[0].kind != BYTES
        || args[0].b > MAX_NAME
        || (args[0].b != 0 && args[0].a == 0)
    {
        return INVALID;
    }
    let bytes = if args[0].b == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(args[0].a as *const u8, args[0].b) }
    };
    match unsafe { seed(api) }.intern(bytes) {
        Some(oop) => unsafe { answer(out, Value::object(oop)) },
        None => INVALID,
    }
}
unsafe extern "C" fn parent(
    api: *const SeedApi,
    _: *mut c_void,
    receiver: Oop,
    _: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    if count != 0 {
        return INVALID;
    }
    let seed = unsafe { seed(api) };
    match seed.direct(receiver, 0) {
        Ok((_, 0)) => unsafe { answer(out, Value::none()) },
        Ok((_, parent)) => unsafe { answer(out, Value::object(parent)) },
        Err(e) => e,
    }
}
unsafe extern "C" fn set_parent(
    api: *const SeedApi,
    _: *mut c_void,
    receiver: Oop,
    args: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    if unsafe { seed(api) }.observing.get() {
        return BUSY;
    }
    let args = unsafe { operands(args, count) };
    if args.len() != 1 || !matches!(args[0].kind, OBJECT | NONE) {
        return INVALID;
    }
    let parent = if args[0].kind == NONE { 0 } else { args[0].a };
    let seed = unsafe { seed(api) };
    if parent != 0 && !seed.owns(parent) {
        return INVALID;
    }
    let mut objects = seed.objects.borrow_mut();
    let Some(Block {
        kind: Kind::Behavior(record),
        ..
    }) = objects.get_mut(&receiver)
    else {
        return INVALID;
    };
    record.parent = parent;
    unsafe { answer(out, Value::object(receiver)) }
}

struct Observation<'a>(&'a Cell<bool>);
impl Drop for Observation<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

unsafe extern "C" fn with_storage_view(
    api: *const SeedApi,
    _: *mut c_void,
    receiver: Oop,
    args: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    let seed = unsafe { seed(api) };
    // The host calls at an explicit quiescent boundary, not from another send.
    if seed.observing.get() || seed.depth.get() != 1 {
        return BUSY;
    }
    if receiver != seed.roots.vtable || count != 1 {
        return INVALID;
    }
    let Some(visitor) = (unsafe { (*args).as_method() }) else {
        return INVALID;
    };
    seed.observing.set(true);
    let _phase = Observation(&seed.observing);
    let mut allocations = Vec::new();
    let mut edges = Vec::new();
    {
        let objects = seed.objects.borrow();
        for (&object, block) in objects.iter() {
            allocations.push(Allocation {
                object,
                bytes: block.bytes,
                payload_owner: if matches!(block.kind, Kind::Ordinary) {
                    OWNER_PAYLOAD
                } else {
                    SEED_PAYLOAD
                },
            });
            edges.push(Edge {
                from: object,
                to: block.words[0],
            });
            if let Kind::Behavior(behavior) = &block.kind {
                if behavior.parent != 0 {
                    edges.push(Edge {
                        from: object,
                        to: behavior.parent,
                    });
                }
                for &selector in behavior.methods.keys() {
                    edges.push(Edge {
                        from: object,
                        to: selector,
                    });
                }
            }
        }
    }
    let r = seed.roots;
    let mut roots = vec![r.object, r.vtable, r.symbol, r.symbol_prototype];
    // This seed deliberately retains interned Symbols strongly. Enumerating
    // allocations must NOT make every other allocation a root.
    roots.extend(seed.symbols.borrow().values().copied());
    roots.sort_unstable();
    roots.dedup();
    let view = StorageView {
        allocations: allocations.as_ptr(),
        allocation_count: allocations.len(),
        roots: roots.as_ptr(),
        root_count: roots.len(),
        edges: edges.as_ptr(),
        edge_count: edges.len(),
    };
    let arguments = [unsafe { record(&view) }];
    // No registry borrow crosses this callback. Native code/data stay pinned;
    // their addresses are not Oop edges. Their owners provide explicit roots.
    unsafe { (visitor.entry)(api, visitor.data, receiver, arguments.as_ptr(), 1, out) }
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_seed_create_v1() -> *mut SeedApi {
    let zero_roots = Roots {
        object: 0,
        vtable: 0,
        symbol: 0,
        symbol_prototype: 0,
    };
    let zero_selectors = Selectors {
        lookup: 0,
        add_method: 0,
        remove_method: 0,
        allocate: 0,
        delegated: 0,
        intern: 0,
        parent: 0,
        set_parent: 0,
    };
    let mut state = Box::new(Seed {
        owner: std::thread::current().id(),
        objects: RefCell::new(BTreeMap::new()),
        symbols: RefCell::new(BTreeMap::new()),
        roots: zero_roots,
        selectors: zero_selectors,
        depth: Cell::new(0),
        observing: Cell::new(false),
    });
    let vtable = state.new_behavior(0).unwrap();
    state.roots.vtable = vtable;
    assert_eq!(state.reassociate(vtable, vtable), OK);
    let object = state.new_behavior(0).unwrap();
    state.roots.object = object;
    if let Kind::Behavior(record) = &mut state.objects.borrow_mut().get_mut(&vtable).unwrap().kind {
        record.parent = object;
    }
    state.roots.symbol = state.new_behavior(object).unwrap();
    state.roots.symbol_prototype = state.allocate(state.roots.symbol, 0, Kind::Symbol).unwrap();
    state.selectors = Selectors {
        lookup: state.intern(b"lookup").unwrap(),
        add_method: state.intern(b"addMethod").unwrap(),
        remove_method: state.intern(b"removeMethod:").unwrap(),
        allocate: state.intern(b"allocate").unwrap(),
        delegated: state.intern(b"delegated").unwrap(),
        intern: state.intern(b"intern").unwrap(),
        parent: state.intern(b"parent").unwrap(),
        set_parent: state.intern(b"parent:").unwrap(),
    };
    assert_eq!(
        state.install(vtable, state.selectors.lookup, Method::plain(lookup)),
        OK
    );
    assert_eq!(
        state.install(
            vtable,
            state.selectors.add_method,
            Method::plain(add_method)
        ),
        OK
    );
    let observe_selector = state.intern(OBSERVE).unwrap();
    let api = Box::into_raw(Box::new(SeedApi {
        version: VERSION,
        size: size_of::<SeedApi>() as u32,
        context: Box::into_raw(state).cast(),
        roots: zero_roots,
        selectors: zero_selectors,
        send,
        behavior_of,
        reassociate,
    }));
    unsafe {
        (*api).roots = seed(api).roots;
        (*api).selectors = seed(api).selectors;
        for (target, selector, entry) in [
            (vtable, observe_selector, with_storage_view as Entry),
            (
                vtable,
                (*api).selectors.remove_method,
                remove_method as Entry,
            ),
            (vtable, (*api).selectors.allocate, allocate as Entry),
            (vtable, (*api).selectors.delegated, delegated as Entry),
            (vtable, (*api).selectors.parent, parent as Entry),
            (vtable, (*api).selectors.set_parent, set_parent as Entry),
            (
                (*api).roots.symbol,
                (*api).selectors.intern,
                intern_method as Entry,
            ),
        ] {
            // Past the recursion root, bootstrap itself installs via messages.
            call(
                api,
                target,
                (*api).selectors.add_method,
                &[Value::object(selector), Value::method(Method::plain(entry))],
            )
            .unwrap();
        }
    }
    api
}

/// # Safety
/// API must come from create, and no consumer may use it after success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_seed_destroy_v1(api: *mut SeedApi) -> Status {
    if api.is_null() {
        return INVALID;
    }
    if unsafe { seed(api) }.depth.get() != 0 {
        return INVALID;
    }
    let api = unsafe { Box::from_raw(api) };
    drop(unsafe { Box::from_raw(api.context.cast::<Seed>()) });
    OK
}
