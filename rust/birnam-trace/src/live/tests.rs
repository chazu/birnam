use super::*;

#[derive(Default)]
struct Probe {
    marked: [bool; 4],
    order: Vec<usize>,
    admitted: Vec<usize>,
    saved: Option<SinkV1>,
    fail_roots: bool,
    fail_edges: bool,
    invalid_admit: bool,
    recursive_admit: bool,
}
unsafe extern "C" fn roots(context: *mut c_void, sink: *const SinkV1) -> Status {
    let sink = unsafe { *sink };
    // Avoid retaining the Probe borrow across emission -> admission callbacks.
    let old = unsafe { (*context.cast::<Probe>()).saved.replace(sink) };
    if let Some(old) = old {
        assert_eq!(unsafe { (old.emit)(old.context, 3) }, BUSY);
    }
    assert_eq!(unsafe { birnam_trace_live_v1(core::ptr::null()) }, BUSY);
    for address in [0, 1, 2, 1, 10] {
        unsafe { (sink.emit)(sink.context, address) }; // ignored errors must latch
    }
    if unsafe { (*context.cast::<Probe>()).fail_roots } {
        -77
    } else {
        OK
    }
}
unsafe extern "C" fn admit(context: *mut c_void, address: usize) -> i32 {
    let pointer = context.cast::<Probe>();
    if unsafe { (*pointer).recursive_admit } {
        let sink = unsafe { (*pointer).saved.unwrap() };
        assert_eq!(unsafe { (sink.emit)(sink.context, address) }, BUSY);
    }
    let probe = unsafe { &mut *pointer };
    probe.admitted.push(address);
    if probe.invalid_admit {
        return 2;
    }
    if address == 0 || address >= probe.marked.len() {
        return 0;
    }
    i32::from(!core::mem::replace(&mut probe.marked[address], true))
}
unsafe extern "C" fn edges(context: *mut c_void, address: usize, sink: *const SinkV1) -> Status {
    let sink = unsafe { *sink };
    let saved = unsafe {
        let probe = &mut *context.cast::<Probe>();
        probe.order.push(address);
        assert!(probe.marked[address]); // admission happened before traversal
        probe.saved.unwrap()
    };
    assert_eq!(unsafe { (saved.emit)(saved.context, 1) }, BUSY); // prior callback
    if address == 2 {
        assert_eq!(unsafe { (sink.emit)(sink.context, 3) }, OK);
        assert_eq!(unsafe { (sink.emit)(sink.context, 1) }, OK); // cycle/duplicate
    }
    if unsafe { (*context.cast::<Probe>()).fail_edges } {
        -78
    } else {
        OK
    }
}
fn source(probe: &mut Probe) -> SourceV1 {
    SourceV1 {
        version: VERSION,
        byte_size: size_of::<SourceV1>(),
        context: (probe as *mut Probe).cast(),
        roots: Some(roots),
        admit: Some(admit),
        edges: Some(edges),
    }
}

#[test]
fn first_mark_admission_and_lifo_order_are_preserved() {
    let mut probe = Probe::default();
    let source = source(&mut probe);
    assert_eq!(unsafe { birnam_trace_live_v1(&source) }, OK);
    assert_eq!(probe.order, [2, 3, 1]);
    assert_eq!(probe.admitted, [0, 1, 2, 1, 10, 3, 1]);
    let saved = probe.saved.unwrap();
    assert_eq!(unsafe { (saved.emit)(saved.context, 2) }, BUSY);
    probe.marked = [false; 4];
    probe.order.clear();
    probe.admitted.clear();
    let source = self::source(&mut probe);
    assert_eq!(unsafe { birnam_trace_live_v1(&source) }, OK);
    assert_eq!(probe.order, [2, 3, 1]);
}

#[test]
fn failures_expire_sinks_and_never_hide_ignored_admission_errors() {
    for (probe, expected) in [
        (
            Probe {
                fail_roots: true,
                ..Probe::default()
            },
            -77,
        ),
        (
            Probe {
                fail_edges: true,
                ..Probe::default()
            },
            -78,
        ),
        (
            Probe {
                invalid_admit: true,
                ..Probe::default()
            },
            INVALID,
        ),
        (
            Probe {
                recursive_admit: true,
                ..Probe::default()
            },
            BUSY,
        ),
    ] {
        let mut probe = probe;
        assert_eq!(
            unsafe { birnam_trace_live_v1(&source(&mut probe)) },
            expected
        );
        let sink = probe.saved.unwrap();
        assert_eq!(unsafe { (sink.emit)(sink.context, 1) }, BUSY);
        let mut fresh = Probe::default();
        assert_eq!(unsafe { birnam_trace_live_v1(&source(&mut fresh)) }, OK);
    }
}

#[test]
fn malformed_sources_and_abi_geometry() {
    assert_eq!(size_of::<SinkV1>(), 16);
    assert_eq!(size_of::<SourceV1>(), 48);
    assert_eq!(size_of::<ApiV1>(), 24);
    assert_eq!(unsafe { birnam_trace_live_v1(core::ptr::null()) }, INVALID);
    let mut probe = Probe::default();
    let mut source = source(&mut probe);
    source.byte_size -= 1;
    assert_eq!(unsafe { birnam_trace_live_v1(&source) }, INVALID);
    source.byte_size += 1;
    source.admit = None;
    assert_eq!(unsafe { birnam_trace_live_v1(&source) }, INVALID);
    assert!(probe.admitted.is_empty() && probe.order.is_empty());
}

#[test]
fn sink_tokens_cannot_enter_another_threads_walk() {
    let mut probe = Probe::default();
    assert_eq!(unsafe { birnam_trace_live_v1(&source(&mut probe)) }, OK);
    let sink = probe.saved.unwrap();
    let (token, emit) = (sink.context.addr(), sink.emit);
    std::thread::spawn(move || {
        let mut probe = Probe {
            saved: Some(SinkV1 {
                context: token as *mut c_void,
                emit,
            }),
            ..Probe::default()
        };
        assert_eq!(unsafe { birnam_trace_live_v1(&source(&mut probe)) }, OK);
    })
    .join()
    .unwrap();
}
