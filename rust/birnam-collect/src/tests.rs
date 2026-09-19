use super::*;

struct Fixture {
    events: Vec<&'static str>,
    settle: i32,
    trace_status: Status,
    detach_status: Status,
    reclaim_status: Status,
    dead: [usize; 2],
}

unsafe fn fixture<'a>(context: *mut c_void) -> &'a mut Fixture {
    unsafe { &mut *context.cast() }
}

unsafe extern "C" fn phase(context: *mut c_void, phase: u32) {
    unsafe { fixture(context) }.events.push(match phase {
        MARKING => "marking",
        FINALIZING => "finalizing",
        IDLE => "idle",
        _ => panic!("phase"),
    });
}
macro_rules! event {
    ($name:ident) => {
        unsafe extern "C" fn $name(context: *mut c_void) {
            unsafe { fixture(context) }.events.push(stringify!($name));
        }
    };
}
event!(clear_marks);
event!(settle_deferred);
event!(retire_metadata);
event!(reset_accounting);
event!(prune);
unsafe extern "C" fn trace(context: *mut c_void) -> Status {
    assert_eq!(
        unsafe { birnam_collect_v1(core::ptr::null(), core::ptr::null_mut()) },
        BUSY
    );
    let f = unsafe { fixture(context) };
    f.events.push("trace");
    f.trace_status
}
unsafe extern "C" fn inspect_retirement(context: *mut c_void) -> i32 {
    let f = unsafe { fixture(context) };
    f.events.push("inspect");
    f.settle
}
unsafe extern "C" fn snapshot_dead(context: *mut c_void, out: *mut DeadV1) {
    let f = unsafe { fixture(context) };
    f.events.push("snapshot");
    unsafe {
        out.write(DeadV1 {
            addresses: f.dead.as_ptr(),
            count: f.dead.len(),
        })
    };
}
unsafe extern "C" fn detach(context: *mut c_void, address: usize, out: *mut DetachedV1) -> Status {
    let f = unsafe { fixture(context) };
    f.events.push(if address == f.dead[0] {
        "detach_a"
    } else {
        "detach_b"
    });
    unsafe {
        out.write(DetachedV1 {
            object: core::ptr::with_exposed_provenance_mut(address),
            finalizer: None,
        })
    };
    f.detach_status
}
unsafe extern "C" fn reclaim(context: *mut c_void, address: usize) -> Status {
    let f = unsafe { fixture(context) };
    f.events.push(if address == f.dead[0] {
        "reclaim_a"
    } else {
        "reclaim_b"
    });
    f.reclaim_status
}
unsafe extern "C" fn reclaim_deferred(context: *mut c_void) -> usize {
    unsafe { fixture(context) }.events.push("deferred");
    7
}
fn source(f: &mut Fixture) -> SourceV1 {
    SourceV1 {
        version: VERSION,
        byte_size: size_of::<SourceV1>(),
        context: (f as *mut Fixture).cast(),
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
    }
}
fn new_fixture() -> Fixture {
    Fixture {
        events: vec![],
        settle: 0,
        trace_status: OK,
        detach_status: OK,
        reclaim_status: OK,
        dead: [32, 16],
    }
}

#[test]
fn ordered_collection_and_conditional_settlement() {
    for settle in [0, 1] {
        let mut f = new_fixture();
        f.settle = settle;
        let mut count = usize::MAX;
        assert_eq!(
            unsafe { birnam_collect_v1(&source(&mut f), &mut count) },
            OK
        );
        assert_eq!(count, 9);
        let mut expected = vec!["marking", "clear_marks", "trace", "inspect"];
        if settle == 1 {
            expected.push("settle_deferred");
        }
        expected.extend([
            "retire_metadata",
            "snapshot",
            "finalizing",
            "detach_a",
            "reclaim_a",
            "detach_b",
            "reclaim_b",
            "deferred",
            "clear_marks",
            "reset_accounting",
            "idle",
            "prune",
        ]);
        assert_eq!(f.events, expected);
    }
}

#[test]
fn errors_stop_before_the_next_effect_without_publishing_a_count() {
    for (trace_status, settle, detach_status, reclaim_status, last) in [
        (-77, 0, 0, 0, "trace"),
        (0, 2, 0, 0, "inspect"),
        (0, 0, -78, 0, "detach_a"),
        (0, 0, 0, -79, "reclaim_a"),
    ] {
        let mut f = new_fixture();
        f.trace_status = trace_status;
        f.settle = settle;
        f.detach_status = detach_status;
        f.reclaim_status = reclaim_status;
        let mut count = usize::MAX;
        assert_ne!(
            unsafe { birnam_collect_v1(&source(&mut f), &mut count) },
            OK
        );
        assert_eq!(count, usize::MAX);
        assert_eq!(f.events.last(), Some(&last));
        // The active guard is released on errors, without inventing source rollback.
        assert!(!ACTIVE.get());
    }
}

#[test]
fn malformed_boundary_is_rejected_before_effects() {
    let mut f = new_fixture();
    let mut count = usize::MAX;
    assert_eq!(
        unsafe { birnam_collect_v1(core::ptr::null(), &mut count) },
        INVALID
    );
    let mut s = source(&mut f);
    s.version += 1;
    assert_eq!(unsafe { birnam_collect_v1(&s, &mut count) }, INVALID);
    s = source(&mut f);
    s.byte_size -= 1;
    assert_eq!(unsafe { birnam_collect_v1(&s, &mut count) }, INVALID);
    s = source(&mut f);
    s.reclaim = None;
    assert_eq!(unsafe { birnam_collect_v1(&s, &mut count) }, INVALID);
    assert!(f.events.is_empty());
    assert_eq!(count, usize::MAX);
    assert_eq!(size_of::<SourceV1>(), 120);
    assert_eq!(size_of::<ApiV1>(), 24);
    assert_eq!(size_of::<DeadV1>(), 16);
    assert_eq!(size_of::<DetachedV1>(), 16);
    assert_eq!(core::mem::offset_of!(SourceV1, detach), 80);
}
