//! Independent collection/reuse oracle; no static runtime implementations.
include!("../../trace-probe/src/support.rs");
use birnam_observation_abi::*;
use std::cell::RefCell;
use std::collections::BTreeSet;

type Collect =
    unsafe extern "C" fn(*const SeedApi, *const CollectRequest, *mut CollectionResult) -> Status;
type IntegerCreate = unsafe extern "C" fn(*const SeedApi, *mut Oop) -> *mut c_void;
type IntegerDestroy = unsafe extern "C" fn(*mut c_void);
type SeedCreate = unsafe extern "C" fn() -> *mut SeedApi;
type SeedDestroy = unsafe extern "C" fn(*mut SeedApi) -> Status;
const FAILED: Status = 0x2ff;

fn published(h: Harness, receiver: Oop, name: &[u8]) -> Method {
    unsafe { h.send(receiver, h.intern(name), &[]).unwrap().as_method() }.unwrap()
}
fn invoke(h: Harness, method: Method, arg: Value) -> Status {
    let mut out = Value::none();
    unsafe { (method.entry)(h.0, method.data, 0, &arg, 1, &mut out) }
}
fn collect(
    run: Collect,
    trace: Method,
    h: Harness,
    roots: &[Oop],
    participants: &[CollectionParticipant],
) -> (Status, CollectionResult) {
    let request = CollectRequest {
        roots: roots.as_ptr(),
        root_count: roots.len(),
        participants: participants.as_ptr(),
        participant_count: participants.len(),
        trace,
    };
    let mut output = CollectionResult::default();
    let status = unsafe { run(h.0, &request, &mut output) };
    (status, output)
}
fn expect_collection(
    run: Collect,
    trace: Method,
    h: Harness,
    roots: &[Oop],
    participants: &[CollectionParticipant],
    count: usize,
    bytes: usize,
) {
    let (status, output) = collect(run, trace, h, roots, participants);
    assert_eq!(status, OK);
    assert_eq!(output.retired_objects, count);
    assert_eq!(output.retired_bytes, bytes);
}
fn pair(object: Oop, first: Oop, second: Oop) {
    unsafe {
        (object as *mut Oop).write(first);
        (object as *mut Oop).add(1).write(second);
    }
}
struct Pairs {
    h: Harness,
    behavior: Oop,
    objects: RefCell<BTreeSet<Oop>>,
    commits: Cell<usize>,
    aborts: Cell<usize>,
    fail_description: Cell<bool>,
}
unsafe extern "C" fn describe_pairs(
    _: *const SeedApi,
    data: *mut c_void,
    _: Oop,
    args: *const Value,
    count: usize,
    _: *mut Value,
) -> Status {
    let state = unsafe { &*data.cast::<Pairs>() };
    if state.fail_description.get() {
        return FAILED;
    }
    let sink = unsafe { decode::<ProviderSink>(args, count) }.unwrap();
    let status = unsafe { (sink.root)(sink.context, state.behavior) };
    if status != OK {
        return status;
    }
    for object in state.objects.borrow().iter() {
        let offsets = [0, size_of::<Oop>()];
        let status = unsafe {
            (sink.payload)(
                sink.context,
                *object,
                2 * size_of::<Oop>(),
                offsets.as_ptr(),
                2,
            )
        };
        if status != OK {
            return status;
        }
    }
    OK
}
struct PairPlan {
    owner: *const Pairs,
    replacement: BTreeSet<Oop>,
}
unsafe extern "C" fn pair_commit(data: *mut c_void) {
    let plan = unsafe { Box::from_raw(data.cast::<PairPlan>()) };
    let owner = unsafe { &*plan.owner };
    // Every retired allocation is still owned by Id when metadata commits.
    for dead in owner.objects.borrow().difference(&plan.replacement) {
        assert_ne!(owner.h.behavior(*dead), 0);
    }
    *owner.objects.borrow_mut() = plan.replacement;
    owner.commits.set(owner.commits.get() + 1);
}
unsafe extern "C" fn pair_abort(data: *mut c_void) {
    let plan = unsafe { Box::from_raw(data.cast::<PairPlan>()) };
    let owner = unsafe { &*plan.owner };
    owner.aborts.set(owner.aborts.get() + 1);
}
unsafe extern "C" fn prepare_pairs(
    _: *const SeedApi,
    data: *mut c_void,
    _: Oop,
    args: *const Value,
    count: usize,
    _: *mut Value,
) -> Status {
    let owner = unsafe { &*data.cast::<Pairs>() };
    let request = unsafe { decode::<RetirementRequest>(args, count) }.unwrap();
    let mut replacement = owner.objects.borrow().clone();
    for i in 0..request.count {
        replacement.remove(&unsafe { *request.objects.add(i) });
    }
    let plan = Box::new(PairPlan { owner, replacement });
    unsafe {
        request.plan.write(RetirementPlan {
            data: Box::into_raw(plan).cast(),
            commit: pair_commit,
            abort: pair_abort,
        })
    };
    OK
}
unsafe extern "C" fn noop(
    _: *const SeedApi,
    _: *mut c_void,
    _: Oop,
    _: *const Value,
    _: usize,
    _: *mut Value,
) -> Status {
    OK
}
unsafe extern "C" fn failure(
    _: *const SeedApi,
    _: *mut c_void,
    _: Oop,
    _: *const Value,
    _: usize,
    _: *mut Value,
) -> Status {
    FAILED
}
struct Wrapped {
    original: Method,
    calls: Cell<usize>,
}
unsafe extern "C" fn wrapped(
    api: *const SeedApi,
    data: *mut c_void,
    receiver: Oop,
    args: *const Value,
    count: usize,
    out: *mut Value,
) -> Status {
    let state = unsafe { &*data.cast::<Wrapped>() };
    state.calls.set(state.calls.get() + 1);
    unsafe { (state.original.entry)(api, state.original.data, receiver, args, count, out) }
}

#[derive(Clone, Copy)]
struct Saved {
    epoch: u64,
    release: Method,
}
struct Guard {
    h: Harness,
    destroy: SeedDestroy,
    saved: Cell<Option<Saved>>,
    stale: Cell<bool>,
    victim: Oop,
    header_target: Oop,
    parent_target: Oop,
    retirement_calls: Cell<usize>,
    mutators: Vec<(Oop, Method, Vec<Value>)>,
}
unsafe extern "C" fn guarded_retirement(
    api: *const SeedApi,
    data: *mut c_void,
    _: Oop,
    _: *const Value,
    _: usize,
    _: *mut Value,
) -> Status {
    let g = unsafe { &*data.cast::<Guard>() };
    g.retirement_calls.set(g.retirement_calls.get() + 1);
    let saved = g.saved.get().unwrap();
    let release = ReleaseRequest {
        epoch: saved.epoch,
        objects: core::ptr::null(),
        count: 0,
        retire: Method::plain(noop),
    };
    assert_eq!(
        invoke(g.h, saved.release, unsafe { record(&release) }),
        BUSY
    );
    assert_eq!(
        g.h.send(
            g.h.api().roots.object,
            g.h.api().selectors.allocate,
            &[Value::word(0)]
        ),
        Err(BUSY)
    );
    assert_eq!(
        unsafe { (g.h.api().reassociate)(api, g.victim, g.h.api().roots.object) },
        BUSY
    );
    assert_eq!(unsafe { (g.destroy)(api.cast_mut()) }, INVALID);
    for (receiver, method, args) in &g.mutators {
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
    OK
}
unsafe extern "C" fn guard_view(
    _: *const SeedApi,
    data: *mut c_void,
    _: Oop,
    args: *const Value,
    count: usize,
    _: *mut Value,
) -> Status {
    let g = unsafe { &*data.cast::<Guard>() };
    let view = unsafe { decode::<ReclamationView>(args, count) }.unwrap();
    let prior = g.saved.get();
    let current = Saved {
        epoch: view.epoch,
        release: view.release,
    };
    g.saved.set(Some(current));
    if g.stale.get() {
        let stale = prior.unwrap();
        let request = ReleaseRequest {
            epoch: stale.epoch,
            objects: core::ptr::null(),
            count: 0,
            retire: Method::plain(noop),
        };
        assert_eq!(
            invoke(g.h, stale.release, unsafe { record(&request) }),
            INVALID
        );
        return OK;
    }
    let retire = Method {
        entry: guarded_retirement,
        data,
    };
    for objects in [
        vec![1],
        vec![g.victim, g.victim],
        vec![g.h.api().roots.object],
        vec![g.header_target],
        vec![g.parent_target],
    ] {
        let request = ReleaseRequest {
            epoch: view.epoch,
            objects: objects.as_ptr(),
            count: objects.len(),
            retire,
        };
        assert_eq!(
            invoke(g.h, view.release, unsafe { record(&request) }),
            INVALID
        );
        assert_eq!(g.retirement_calls.get(), 0);
    }
    let failed = ReleaseRequest {
        epoch: view.epoch,
        objects: &g.victim,
        count: 1,
        retire: Method::plain(failure),
    };
    assert_eq!(
        invoke(g.h, view.release, unsafe { record(&failed) }),
        FAILED
    );
    assert_ne!(g.h.behavior(g.victim), 0);
    let request = ReleaseRequest {
        epoch: view.epoch,
        objects: core::ptr::null(),
        count: 0,
        retire,
    };
    assert_eq!(invoke(g.h, view.release, unsafe { record(&request) }), OK);
    assert_eq!(invoke(g.h, view.release, unsafe { record(&request) }), BUSY);
    assert_eq!(g.retirement_calls.get(), 1);
    OK
}

fn main() {
    let paths: Vec<_> = std::env::args().collect();
    assert_eq!(
        paths.len(),
        6,
        "collect-probe SEED OBJECT_MODEL INTEGER TRACE COLLECT"
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
    let object_create: unsafe extern "C" fn(*const SeedApi) -> *mut LibraryApi =
        unsafe { std::mem::transmute(object_lib.symbol(c"birnam_object_model_create_v1")) };
    let object_destroy: unsafe extern "C" fn(*mut LibraryApi) =
        unsafe { std::mem::transmute(object_lib.symbol(c"birnam_object_model_destroy_v1")) };
    let object_model = unsafe { object_create(h.0) };
    assert!(!object_model.is_null());
    let private = unsafe { (*object_model).ensure_private };
    let created = || unsafe { ((*object_model).created_count)(object_model) };
    let from_word = h.intern(b"fromWord:");
    let as_word = h.intern(b"asWord");
    let plus = h.intern(b"+");
    let make = |value: usize| h.send(factory, from_word, &[Value::word(value)]).unwrap().a;
    let integer_behavior = h
        .send(factory, h.intern(b"integerBehavior"), &[])
        .unwrap()
        .a;
    let behavior = h.delegate(h.api().roots.object);
    let objects: Vec<_> = (0..4)
        .map(|_| h.allocate(behavior, 2 * size_of::<Oop>()))
        .collect();
    let (left, right, dead_a, dead_b) = (objects[0], objects[1], objects[2], objects[3]);
    let live_integer = make(40);
    let raw_integer = make(dead_a);
    let dead_integer = make(2);
    let dead_private = h.send(dead_integer, private, &[]).unwrap().a;
    let empty = h.allocate(h.api().roots.object, 0);
    pair(left, right, live_integer);
    pair(right, left, raw_integer);
    pair(dead_a, dead_b, 0);
    pair(dead_b, dead_a, 0);
    let pairs = Box::new(Pairs {
        h,
        behavior,
        objects: RefCell::new(objects.into_iter().collect()),
        commits: Cell::new(0),
        aborts: Cell::new(0),
        fail_description: Cell::new(false),
    });
    let data = (&*pairs as *const Pairs).cast_mut().cast();
    let participants = [
        CollectionParticipant {
            provider: published(h, factory, PROVIDER),
            prepare: published(h, factory, RETIREMENT),
        },
        CollectionParticipant {
            provider: published(h, h.api().roots.vtable, b"objectModelStorageProvider"),
            prepare: published(
                h,
                h.api().roots.vtable,
                b"objectModelPrepareStorageRetirement",
            ),
        },
        CollectionParticipant {
            provider: Method {
                entry: describe_pairs,
                data,
            },
            prepare: Method {
                entry: prepare_pairs,
                data,
            },
        },
    ];
    // Graph and representation owners predate both dynamically loaded algorithms.
    let trace_lib = Dylib::open(&paths[4]);
    let trace = Method::plain(unsafe {
        std::mem::transmute::<*mut c_void, Entry>(trace_lib.symbol(c"birnam_trace_view_v1"))
    });
    let collect_lib = Dylib::open(&paths[5]);
    let run: Collect = unsafe { std::mem::transmute(collect_lib.symbol(c"birnam_collect_v1")) };
    case("collect-cycles-and-survivors", || {
        expect_collection(run, trace, h, &[left], &participants, 4, 40);
        for dead in [dead_a, dead_b, dead_integer, empty] {
            assert_eq!(h.behavior(dead), 0);
        }
        assert_ne!(h.behavior(dead_private), 0);
        assert_eq!(pairs.objects.borrow().len(), 2);
        h.expect_word(live_integer, as_word, 40);
        h.expect_word(raw_integer, as_word, dead_a);
        let sum = h
            .send(live_integer, plus, &[Value::object(live_integer)])
            .unwrap()
            .a;
        h.expect_word(sum, as_word, 80);
        expect_collection(run, trace, h, &[left], &participants, 1, 8);
        expect_collection(run, trace, h, &[left], &participants, 0, 0);
    });
    case("collect-root-scopes-and-retained-behavior-edges", || {
        let held = make(17);
        expect_collection(run, trace, h, &[left, held], &participants, 0, 0);
        h.expect_word(held, as_word, 17);
        expect_collection(run, trace, h, &[left], &participants, 1, 8);
        let target = h.allocate(h.api().roots.object, 0);
        let keeper = h.delegate(h.api().roots.object);
        h.set_parent(keeper, target);
        expect_collection(run, trace, h, &[left], &participants, 0, 0);
        assert_ne!(h.behavior(target), 0);
        h.set_parent(keeper, h.api().roots.object);
        expect_collection(run, trace, h, &[left], &participants, 1, 0);
        // Representation ownership survives arbitrary behavior reassociation.
        h.reassociate(right, integer_behavior);
        expect_collection(run, trace, h, &[left], &participants, 0, 0);
        assert_eq!(h.send(right, as_word, &[]), Err(INVALID));
        h.reassociate(right, behavior);
    });
    case("collect-failure-is-atomic", || {
        let dead = make(99);
        let private_dead = h.send(dead, private, &[]).unwrap().a;
        let n = created();
        let commits = pairs.commits.get();
        pairs.fail_description.set(true);
        let (status, output) = collect(run, trace, h, &[left], &participants);
        assert_eq!(status, FAILED);
        assert_eq!(output.retired_objects, 0);
        pairs.fail_description.set(false);
        let mut failing = participants.to_vec();
        failing.push(CollectionParticipant {
            provider: Method::plain(noop),
            prepare: Method::plain(failure),
        });
        let aborts = pairs.aborts.get();
        let (status, output) = collect(run, trace, h, &[left], &failing);
        assert_eq!(status, FAILED);
        assert_eq!(output.retired_objects, 0);
        assert_eq!(pairs.aborts.get(), aborts + 1);
        assert_eq!(pairs.commits.get(), commits);
        h.expect_word(dead, as_word, 99);
        assert_eq!(h.send(dead, private, &[]).unwrap().a, private_dead);
        assert_eq!(created(), n);
        assert_eq!(
            collect(run, trace, h, &[left], &participants[1..]).0,
            INCOMPLETE
        );
        assert_eq!(collect(run, trace, h, &[1], &participants).0, INVALID);
        h.expect_word(dead, as_word, 99);
        expect_collection(run, trace, h, &[left], &participants, 1, 8);
    });
    case("collect-forced-integer-and-private-reuse", || {
        let old = make(123);
        let old_private = h.send(old, private, &[]).unwrap().a;
        expect_collection(run, trace, h, &[left], &participants, 1, 8);
        assert_eq!(h.behavior(old), 0);
        // Zero-byte allocations and i64 payloads share the same backing size.
        let fresh = h.allocate(old_private, 0);
        assert_eq!(fresh, old, "the witness MUST reuse the exact address");
        assert_eq!(unsafe { (fresh as *const usize).read() }, 0);
        assert_eq!(h.send(fresh, as_word, &[]), Err(INVALID));
        let before = created();
        let fresh_private = h.send(fresh, private, &[]).unwrap().a;
        assert_ne!(fresh_private, old_private);
        assert_eq!(created(), before + 1);
        assert_eq!(h.send(fresh, private, &[]).unwrap().a, fresh_private);
        expect_collection(run, trace, h, &[left], &participants, 1, 0);
        let reconstructed = make(321);
        assert_eq!(reconstructed, old);
        h.expect_word(reconstructed, as_word, 321);
        expect_collection(run, trace, h, &[left], &participants, 1, 8);
    });
    case("collect-forced-payload-description-reuse", || {
        let old = h.allocate(behavior, 16);
        pair(old, left, old);
        pairs.objects.borrow_mut().insert(old);
        expect_collection(run, trace, h, &[left], &participants, 1, 16);
        let fresh = h.allocate(behavior, 16);
        assert_eq!(fresh, old);
        assert_eq!(unsafe { (fresh as *const Oop).read() }, 0);
        assert_eq!(unsafe { (fresh as *const Oop).add(1).read() }, 0);
        // Old Pair metadata must not silently describe the new occupant.
        assert_eq!(
            collect(run, trace, h, &[left, fresh], &participants).0,
            INCOMPLETE
        );
        pair(fresh, left, 0);
        pairs.objects.borrow_mut().insert(fresh);
        expect_collection(run, trace, h, &[left, fresh], &participants, 0, 0);
        expect_collection(run, trace, h, &[left], &participants, 1, 16);
    });
    let reclaim_selector = h.intern(RECLAIM);
    let wrapped_state = Box::new(Wrapped {
        original: h.method(h.api().roots.vtable, reclaim_selector),
        calls: Cell::new(0),
    });
    case("collect-replaceable-phase-message", || {
        h.install(
            h.api().roots.vtable,
            reclaim_selector,
            Method {
                entry: wrapped,
                data: (&*wrapped_state as *const Wrapped).cast_mut().cast(),
            },
        );
        expect_collection(run, trace, h, &[left], &participants, 0, 0);
        assert_eq!(wrapped_state.calls.get(), 1);
        h.install(
            h.api().roots.vtable,
            reclaim_selector,
            wrapped_state.original,
        );
    });
    let victim = h.allocate(h.api().roots.object, 0);
    let header_target = h.allocate(h.api().roots.object, 0);
    let header_owner = h.allocate(h.api().roots.object, 0);
    h.reassociate(header_owner, header_target);
    let parent_target = h.allocate(h.api().roots.object, 0);
    let parent_owner = h.delegate(h.api().roots.object);
    h.set_parent(parent_owner, parent_target);
    let s = h.api().selectors;
    let mutators = [
        (h.api().roots.object, s.allocate, vec![Value::word(0)]),
        (h.api().roots.object, s.delegated, vec![]),
        (
            parent_owner,
            s.set_parent,
            vec![Value::object(h.api().roots.object)],
        ),
        (
            h.api().roots.symbol_prototype,
            s.intern,
            vec![Value::bytes(b"forbidden")],
        ),
        (
            h.api().roots.vtable,
            reclaim_selector,
            vec![Value::method(Method::plain(noop))],
        ),
    ]
    .into_iter()
    .map(|(receiver, selector, args)| (receiver, h.method(h.behavior(receiver), selector), args))
    .collect();
    let guard = Box::new(Guard {
        h,
        destroy,
        saved: Cell::new(None),
        stale: Cell::new(false),
        victim,
        header_target,
        parent_target,
        retirement_calls: Cell::new(0),
        mutators,
    });
    let visitor = Method {
        entry: guard_view,
        data: (&*guard as *const Guard).cast_mut().cast(),
    };
    case("collect-release-validation-and-phase-guards", || {
        h.send(
            h.api().roots.vtable,
            reclaim_selector,
            &[Value::method(visitor)],
        )
        .unwrap();
        assert_ne!(h.behavior(victim), 0);
        assert_eq!(h.behavior(header_owner), header_target);
    });
    case("collect-rejects-stale-capability", || {
        let saved = guard.saved.get().unwrap();
        let request = ReleaseRequest {
            epoch: saved.epoch,
            objects: &victim,
            count: 1,
            retire: Method::plain(noop),
        };
        assert_eq!(invoke(h, saved.release, unsafe { record(&request) }), BUSY);
        let foreign = Harness(unsafe { create() });
        assert_eq!(
            invoke(foreign, saved.release, unsafe { record(&request) }),
            INVALID
        );
        assert_eq!(unsafe { destroy(foreign.0) }, OK);
        guard.stale.set(true);
        h.send(
            h.api().roots.vtable,
            reclaim_selector,
            &[Value::method(visitor)],
        )
        .unwrap();
        h.set_parent(parent_owner, h.api().roots.object);
        expect_collection(run, trace, h, &[left], &participants, 4, 0);
    });
    case("collect-repeated-reuse", || {
        let mut prior = None;
        for value in 0..256 {
            let object = make(value);
            if let Some(prior) = prior {
                assert_eq!(object, prior);
            }
            h.expect_word(object, as_word, value);
            expect_collection(run, trace, h, &[left], &participants, 1, 8);
            assert_eq!(h.behavior(object), 0);
            prior = Some(object);
        }
        expect_collection(run, trace, h, &[], &participants, 4, 48);
        assert!(pairs.objects.borrow().is_empty());
        expect_collection(run, trace, h, &[], &participants, 0, 0);
    });
    for index in 0..unsafe { _dyld_image_count() } {
        println!(
            "IMAGE\t{}",
            unsafe { CStr::from_ptr(_dyld_get_image_name(index)) }
                .to_str()
                .unwrap()
        );
    }
    case("collect-ordered-shutdown", || {
        assert_eq!(unsafe { destroy(h.0) }, OK);
        unsafe {
            object_destroy(object_model);
            integer_destroy(integer);
        }
    });
    drop(collect_lib);
    drop(trace_lib);
    drop(object_lib);
    drop(integer_lib);
    drop(seed_lib);
}
