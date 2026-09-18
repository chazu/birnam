//! Independent reachability oracle. No static implementation dependencies.
include!("support.rs");
use birnam_observation_abi::*;
use std::collections::{BTreeMap, BTreeSet};

type Trace = unsafe extern "C" fn(*const SeedApi, *const TraceRequest) -> Status;
type IntegerCreate = unsafe extern "C" fn(*const SeedApi, *mut Oop) -> *mut c_void;
type IntegerDestroy = unsafe extern "C" fn(*mut c_void);
const FAILED_PROVIDER: Status = 0x2ff;

#[derive(Default)]
struct Capture {
    allocations: BTreeMap<Oop, Allocation>,
    roots: BTreeSet<Oop>,
    edges: Vec<Edge>,
    live: BTreeSet<Oop>,
    dead: BTreeSet<Oop>,
    calls: usize,
    status: Status,
}
unsafe extern "C" fn capture(
    _: *const SeedApi,
    data: *mut c_void,
    _: Oop,
    args: *const Value,
    count: usize,
    _: *mut Value,
) -> Status {
    let output = unsafe { &mut *data.cast::<Capture>() };
    let report = unsafe { decode::<TraceReport>(args, count) }.unwrap();
    let view = unsafe { &*report.view };
    output.calls += 1;
    for i in 0..view.allocation_count {
        let a = unsafe { *view.allocations.add(i) };
        output.allocations.insert(a.object, a);
    }
    for i in 0..view.root_count {
        output.roots.insert(unsafe { *view.roots.add(i) });
    }
    for i in 0..view.edge_count {
        output.edges.push(unsafe { *view.edges.add(i) });
    }
    for i in 0..report.reachable_count {
        assert!(output.live.insert(unsafe { *report.reachable.add(i) }));
    }
    for i in 0..report.unreachable_count {
        assert!(output.dead.insert(unsafe { *report.unreachable.add(i) }));
    }
    output.status
}
fn run(
    trace: Trace,
    h: Harness,
    roots: &[Oop],
    providers: &[Method],
    status: Status,
) -> (Status, Capture) {
    let mut output = Capture {
        status,
        ..Capture::default()
    };
    let request = TraceRequest {
        roots: roots.as_ptr(),
        root_count: roots.len(),
        providers: providers.as_ptr(),
        provider_count: providers.len(),
        report: Method {
            entry: capture,
            data: (&mut output as *mut Capture).cast(),
        },
    };
    let status = unsafe { trace(h.0, &request) };
    (status, output)
}
fn success(trace: Trace, h: Harness, roots: &[Oop], providers: &[Method]) -> Capture {
    let (status, report) = run(trace, h, roots, providers, OK);
    assert_eq!(status, OK);
    assert_eq!(report.calls, 1);
    assert!(report.live.is_disjoint(&report.dead));
    assert_eq!(
        report
            .live
            .union(&report.dead)
            .copied()
            .collect::<BTreeSet<_>>(),
        report.allocations.keys().copied().collect()
    );
    report
}

struct Pairs {
    objects: Vec<Oop>,
    behavior: Oop,
    extra_root: Cell<Oop>,
    mode: Cell<u32>,
}
unsafe extern "C" fn pairs(
    _: *const SeedApi,
    data: *mut c_void,
    _: Oop,
    args: *const Value,
    count: usize,
    _: *mut Value,
) -> Status {
    let pairs = unsafe { &*data.cast::<Pairs>() };
    let sink = unsafe { decode::<ProviderSink>(args, count) }.unwrap();
    for root in [pairs.behavior, pairs.extra_root.get()] {
        if root != 0 {
            let status = unsafe { (sink.root)(sink.context, root) };
            if status != OK {
                return status;
            }
        }
    }
    if pairs.mode.get() == 3 {
        return FAILED_PROVIDER;
    }
    let offsets = if pairs.mode.get() == 2 {
        [0, 1]
    } else {
        [0, size_of::<Oop>()]
    };
    let bytes = if pairs.mode.get() == 1 {
        3 * size_of::<Oop>()
    } else {
        2 * size_of::<Oop>()
    };
    for &object in &pairs.objects {
        let status =
            unsafe { (sink.payload)(sink.context, object, bytes, offsets.as_ptr(), offsets.len()) };
        if status != OK && pairs.mode.get() != 4 {
            return status;
        }
        if pairs.mode.get() == 4 {
            // Deliberately ignore a duplicate-description rejection.
            unsafe { (sink.payload)(sink.context, object, bytes, offsets.as_ptr(), offsets.len()) };
        }
    }
    OK
}
fn write_pair(object: Oop, first: Oop, second: Oop) {
    // These two fields belong to this fixture's declared representation.
    unsafe {
        (object as *mut Oop).write(first);
        (object as *mut Oop).add(1).write(second);
    }
}
fn published(h: Harness, receiver: Oop, selector: &[u8]) -> Method {
    let value = h.send(receiver, h.intern(selector), &[]).unwrap();
    unsafe { value.as_method() }.unwrap()
}
unsafe extern "C" fn marker(
    _: *const SeedApi,
    _: *mut c_void,
    _: Oop,
    _: *const Value,
    _: usize,
    out: *mut Value,
) -> Status {
    unsafe { out.write(Value::word(7)) };
    OK
}

struct Guards {
    h: Harness,
    destroy: SeedDestroy,
    originals: Vec<(Oop, Method, Vec<Value>)>,
    calls: Cell<usize>,
}
unsafe extern "C" fn guards(
    api: *const SeedApi,
    data: *mut c_void,
    _: Oop,
    _: *const Value,
    _: usize,
    _: *mut Value,
) -> Status {
    let g = unsafe { &*data.cast::<Guards>() };
    let h = g.h;
    assert_eq!(
        h.send(
            h.api().roots.object,
            h.api().selectors.allocate,
            &[Value::word(0)]
        ),
        Err(BUSY)
    );
    assert_eq!(
        unsafe { (h.api().reassociate)(api, h.api().roots.object, h.api().roots.vtable) },
        BUSY
    );
    assert_eq!(unsafe { (g.destroy)(api.cast_mut()) }, INVALID);
    for (receiver, method, args) in &g.originals {
        let mut out = Value::none();
        assert_eq!(
            unsafe {
                (method.entry)(
                    api,
                    method.data,
                    *receiver,
                    args.as_ptr(),
                    args.len(),
                    &mut out,
                )
            },
            BUSY
        );
    }
    g.calls.set(g.calls.get() + 1);
    OK
}
struct Nested {
    trace: Trace,
    called: Cell<usize>,
}
unsafe extern "C" fn nested(
    api: *const SeedApi,
    data: *mut c_void,
    _: Oop,
    _: *const Value,
    _: usize,
    _: *mut Value,
) -> Status {
    let nested = unsafe { &*data.cast::<Nested>() };
    nested.called.set(nested.called.get() + 1);
    let (status, output) = run(nested.trace, Harness(api.cast_mut()), &[], &[], OK);
    assert_eq!(status, BUSY);
    assert_eq!(output.calls, 0);
    OK
}
struct Observed {
    original: Method,
    calls: Cell<usize>,
}
unsafe extern "C" fn observed(
    api: *const SeedApi,
    data: *mut c_void,
    receiver: Oop,
    args: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    let observed = unsafe { &*data.cast::<Observed>() };
    observed.calls.set(observed.calls.get() + 1);
    unsafe { (observed.original.entry)(api, observed.original.data, receiver, args, count, out) }
}

fn main() {
    let paths: Vec<_> = std::env::args().collect();
    assert_eq!(
        paths.len(),
        5,
        "trace-probe SEED OBJECT_MODEL INTEGER TRACE"
    );
    let seed_lib = Dylib::open(&paths[1]);
    let create: SeedCreate =
        unsafe { std::mem::transmute(seed_lib.symbol(c"birnam_seed_create_v1")) };
    let destroy: SeedDestroy =
        unsafe { std::mem::transmute(seed_lib.symbol(c"birnam_seed_destroy_v1")) };
    let h = Harness(unsafe { create() });
    let integer_lib = Dylib::open(&paths[3]);
    let integer_create: IntegerCreate =
        unsafe { std::mem::transmute(integer_lib.symbol(c"birnam_integer_create_v1")) };
    let integer_destroy: IntegerDestroy =
        unsafe { std::mem::transmute(integer_lib.symbol(c"birnam_integer_destroy_v1")) };
    let mut factory = 0;
    let integer = unsafe { integer_create(h.0, &mut factory) };
    assert!(!integer.is_null());
    let object_lib = Dylib::open(&paths[2]);
    let object_create: LibraryCreate =
        unsafe { std::mem::transmute(object_lib.symbol(c"birnam_object_model_create_v1")) };
    let object_destroy: LibraryDestroy =
        unsafe { std::mem::transmute(object_lib.symbol(c"birnam_object_model_destroy_v1")) };
    let object_model = unsafe { object_create(h.0) };
    assert!(!object_model.is_null());
    let private = unsafe { (*object_model).ensure_private };
    let from_word = h.intern(b"fromWord:");
    let as_word = h.intern(b"asWord");
    let make = |value: usize| h.send(factory, from_word, &[Value::word(value)]).unwrap().a;
    let live_integer = make(40);
    let dead_integer = make(2);
    let parent = h.delegate(h.api().roots.object);
    let behavior = h.delegate(parent);
    h.set_parent(behavior, parent);
    let marker_selector = h.intern(b"pair-marker");
    h.install(behavior, marker_selector, Method::plain(marker));
    let objects: Vec<_> = (0..5)
        .map(|_| h.allocate(behavior, 2 * size_of::<Oop>()))
        .collect();
    let (left, right, shared, dead_a, dead_b) =
        (objects[0], objects[1], objects[2], objects[3], objects[4]);
    let raw_address_integer = make(dead_a);
    write_pair(left, right, shared);
    write_pair(right, left, shared);
    write_pair(shared, live_integer, raw_address_integer);
    write_pair(dead_a, dead_b, 0);
    write_pair(dead_b, dead_a, 0);
    let private_left = h.send(left, private, &[]).unwrap().a;
    let private_dead = h.send(dead_integer, private, &[]).unwrap().a;
    let empty_dead = h.allocate(h.api().roots.object, 0);
    let pair_state = Box::new(Pairs {
        objects,
        behavior,
        extra_root: Cell::new(0),
        mode: Cell::new(0),
    });
    let pair_provider = Method {
        entry: pairs,
        data: (&*pair_state as *const Pairs).cast_mut().cast(),
    };
    let integer_provider = published(h, factory, PROVIDER);
    let object_provider = published(h, h.api().roots.vtable, b"objectModelStorageProvider");
    let providers = [integer_provider, object_provider, pair_provider];
    // The entire graph and upper modules exist BEFORE the tracer is loaded.
    let tracer_lib = Dylib::open(&paths[4]);
    let trace: Trace = unsafe { std::mem::transmute(tracer_lib.symbol(c"birnam_trace_v1")) };
    let dead_expected = BTreeSet::from([dead_a, dead_b, dead_integer, private_dead, empty_dead]);
    case("trace-existing-storage-facts", || {
        let report = success(trace, h, &[left], &providers);
        for object in &pair_state.objects {
            assert_eq!(report.allocations[object].bytes, 2 * size_of::<Oop>());
        }
        assert_eq!(report.allocations[&live_integer].bytes, 8);
        assert_eq!(report.allocations[&empty_dead].bytes, 0);
        assert!(report.edges.contains(&Edge {
            from: behavior,
            to: parent
        }));
        assert!(report.edges.contains(&Edge {
            from: behavior,
            to: marker_selector
        }));
        assert!(report.edges.contains(&Edge {
            from: left,
            to: private_left
        }));
        for object in [
            h.api().roots.object,
            h.api().roots.vtable,
            h.api().roots.symbol,
            h.api().roots.symbol_prototype,
        ] {
            assert!(report.roots.contains(&object));
        }
        assert!(report.roots.contains(&marker_selector));
    });
    case("trace-shared-cycles-and-raw-words", || {
        let report = success(trace, h, &[left], &providers);
        assert_eq!(report.dead, dead_expected);
        for object in [
            left,
            right,
            shared,
            live_integer,
            raw_address_integer,
            private_left,
            parent,
            behavior,
            factory,
        ] {
            assert!(report.live.contains(&object));
        }
        h.expect_word(raw_address_integer, as_word, dead_a);
    });
    case("trace-changing-root-scopes", || {
        let report = success(trace, h, &[left, dead_a], &providers);
        assert_eq!(
            report.dead,
            BTreeSet::from([dead_integer, private_dead, empty_dead])
        );
        pair_state.extra_root.set(dead_integer);
        let report = success(trace, h, &[left], &providers);
        assert_eq!(report.dead, BTreeSet::from([dead_a, dead_b, empty_dead]));
        pair_state.extra_root.set(0);
        assert_eq!(success(trace, h, &[left], &providers).dead, dead_expected);
        let report = success(trace, h, &[], &providers);
        let expected: BTreeSet<_> = dead_expected
            .iter()
            .copied()
            .chain([
                left,
                right,
                shared,
                live_integer,
                raw_address_integer,
                private_left,
            ])
            .collect();
        assert_eq!(report.dead, expected);
    });
    case("trace-representation-survives-reassociation", || {
        let integer_behavior = h
            .send(factory, h.intern(b"integerBehavior"), &[])
            .unwrap()
            .a;
        h.reassociate(right, integer_behavior);
        assert_eq!(success(trace, h, &[left], &providers).dead, dead_expected);
        // A Pair with Integer behavior still has Pair slots, not raw integer bytes.
        assert_eq!(h.send(right, as_word, &[]), Err(INVALID));
        h.reassociate(right, behavior);
    });
    case("trace-rejects-incomplete-and-invalid-input", || {
        for incomplete in [&providers[..2], &providers[1..]] {
            let (status, output) = run(trace, h, &[left], incomplete, OK);
            assert_eq!(status, INCOMPLETE);
            assert_eq!(output.calls, 0);
        }
        for mode in [1, 2, 4] {
            pair_state.mode.set(mode);
            let (status, output) = run(trace, h, &[left], &providers, OK);
            assert_eq!(status, INVALID);
            assert_eq!(output.calls, 0);
        }
        pair_state.mode.set(0);
        assert_eq!(run(trace, h, &[1], &providers, OK).0, INVALID);
        assert_eq!(
            run(
                trace,
                h,
                &[left],
                &[
                    integer_provider,
                    object_provider,
                    pair_provider,
                    integer_provider
                ],
                OK
            )
            .0,
            INVALID
        );
        write_pair(shared, 1, raw_address_integer);
        assert_eq!(run(trace, h, &[left], &providers, OK).0, INVALID);
        write_pair(shared, live_integer, raw_address_integer);
        assert_eq!(success(trace, h, &[left], &providers).dead, dead_expected);
    });
    let s = h.api().selectors;
    let observer_selector = h.intern(OBSERVE);
    let originals: Vec<_> = [
        (h.api().roots.object, s.allocate, vec![Value::word(0)]),
        (h.api().roots.object, s.delegated, vec![]),
        (
            behavior,
            s.add_method,
            vec![
                Value::object(marker_selector),
                Value::method(Method::plain(marker)),
            ],
        ),
        (
            behavior,
            s.remove_method,
            vec![Value::object(marker_selector)],
        ),
        (behavior, s.set_parent, vec![Value::object(parent)]),
        (
            h.api().roots.symbol_prototype,
            s.intern,
            vec![Value::bytes(b"should-not-intern")],
        ),
        (
            h.api().roots.vtable,
            observer_selector,
            vec![Value::method(Method::plain(marker))],
        ),
    ]
    .into_iter()
    .map(|(receiver, selector, args)| (receiver, h.method(h.behavior(receiver), selector), args))
    .collect();
    let guard_state = Box::new(Guards {
        h,
        destroy,
        originals,
        calls: Cell::new(0),
    });
    let guard_provider = Method {
        entry: guards,
        data: (&*guard_state as *const Guards).cast_mut().cast(),
    };
    case("trace-observation-phase-guards", || {
        let report = success(
            trace,
            h,
            &[left],
            &[
                integer_provider,
                object_provider,
                pair_provider,
                guard_provider,
            ],
        );
        assert_eq!(report.dead, dead_expected);
        assert_eq!(guard_state.calls.get(), 1);
        h.expect_word(left, marker_selector, 7);
    });
    let nested_state = Box::new(Nested {
        trace,
        called: Cell::new(0),
    });
    case("trace-error-unwinds-phase-and-nesting", || {
        pair_state.mode.set(3);
        let (status, output) = run(trace, h, &[left], &providers, OK);
        assert_eq!(status, FAILED_PROVIDER);
        assert_eq!(output.calls, 0);
        pair_state.mode.set(0);
        let (status, output) = run(trace, h, &[left], &providers, FAILED_PROVIDER);
        assert_eq!(status, FAILED_PROVIDER);
        assert_eq!(output.calls, 1);
        let nested_selector = h.intern(b"nestedTrace");
        h.install(
            behavior,
            nested_selector,
            Method {
                entry: nested,
                data: (&*nested_state as *const Nested).cast_mut().cast(),
            },
        );
        h.send(left, nested_selector, &[]).unwrap();
        assert_eq!(nested_state.called.get(), 1);
        assert_eq!(success(trace, h, &[left], &providers).dead, dead_expected);
        h.expect_word(live_integer, as_word, 40);
    });
    let observed_state = Box::new(Observed {
        original: h.method(h.api().roots.vtable, observer_selector),
        calls: Cell::new(0),
    });
    case("trace-storage-message-remains-replaceable", || {
        h.install(
            h.api().roots.vtable,
            observer_selector,
            Method {
                entry: observed,
                data: (&*observed_state as *const Observed).cast_mut().cast(),
            },
        );
        assert_eq!(success(trace, h, &[left], &providers).dead, dead_expected);
        assert_eq!(observed_state.calls.get(), 1);
        h.install(
            h.api().roots.vtable,
            observer_selector,
            observed_state.original,
        );
    });
    for index in 0..unsafe { _dyld_image_count() } {
        println!(
            "IMAGE\t{}",
            unsafe { CStr::from_ptr(_dyld_get_image_name(index)) }
                .to_str()
                .unwrap()
        );
    }
    case("trace-ordered-shutdown", || {
        assert_eq!(unsafe { destroy(h.0) }, OK);
        unsafe {
            object_destroy(object_model);
            integer_destroy(integer);
        }
    });
    drop(tracer_lib);
    drop(object_lib);
    drop(integer_lib);
    drop(seed_lib);
}
