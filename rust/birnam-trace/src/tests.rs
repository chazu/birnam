use super::*;
use birnam_abi::Oop;
use storage::{AllocationV1, EdgeV1, OK, ViewV1};

fn object(index: usize) -> Oop {
    (0x1000 + index * 16) as Oop
}
fn allocation(index: usize) -> AllocationV1 {
    AllocationV1 {
        object: object(index),
        identity: index as u64 + 1,
        requested_size: 0,
        padded_size: 0,
        allocation_bytes: 8,
        behavior: core::ptr::null_mut(),
        flags: storage::PERMANENT, // permanence must not invent a graph root
        trace_hook: None,
    }
}
fn view(allocations: &[AllocationV1], edges: &[EdgeV1]) -> ViewV1 {
    ViewV1 {
        version: storage::VERSION,
        byte_size: size_of::<ViewV1>(),
        runtime_identity: 7,
        observation_identity: 19,
        allocations: allocations.as_ptr(),
        allocation_count: allocations.len(),
        roots: core::ptr::null(),
        root_count: 0,
        edges: edges.as_ptr(),
        edge_count: edges.len(),
        legacy_root_providers: core::ptr::null(),
        legacy_root_provider_count: 0,
        deferred_count: 0,
        deferred_bytes: 0,
        coverage_gaps: 0,
        gc_precise: 0,
    }
}
fn described(view: &ViewV1, roots: &[Oop], edges: &[EdgeV1]) -> DescribedViewV1 {
    DescribedViewV1 {
        storage: view,
        roots: roots.as_ptr(),
        root_count: roots.len(),
        descriptions: core::ptr::null(),
        description_count: 0,
        payload_edges: edges.as_ptr(),
        payload_edge_count: edges.len(),
        missing_description_count: 0,
        coverage_gaps: 0,
    }
}
#[derive(Default)]
struct Capture {
    calls: usize,
    reached: Vec<AllocationRefV1>,
    not_reached: Vec<AllocationRefV1>,
    gaps: u32,
    status: Status,
    nested: bool,
}
unsafe extern "C" fn capture(context: *mut c_void, report: *const ReportV1) -> Status {
    let (capture, report) = unsafe { (&mut *context.cast::<Capture>(), &*report) };
    assert_eq!(report.version, VERSION);
    assert_eq!(report.byte_size, size_of::<ReportV1>());
    assert_eq!(
        (report.runtime_identity, report.observation_identity),
        (7, 19)
    );
    capture.calls += 1;
    capture.reached = unsafe { borrowed_slice(report.reached, report.reached_count) }
        .unwrap()
        .to_vec();
    capture.not_reached = unsafe { borrowed_slice(report.not_reached, report.not_reached_count) }
        .unwrap()
        .to_vec();
    capture.gaps = report.coverage_gaps;
    if capture.nested {
        let mut nested = Capture::default();
        assert_eq!(
            unsafe { birnam_trace_described_v1(report.described, &request(&mut nested)) },
            OK
        );
        assert_eq!(nested.reached, capture.reached);
    }
    capture.status
}
fn request(capture: &mut Capture) -> RequestV1 {
    RequestV1 {
        version: VERSION,
        byte_size: size_of::<RequestV1>(),
        accept_gaps: 0,
        context: (capture as *mut Capture).cast(),
        visitor: Some(self::capture),
    }
}

#[test]
fn all_three_vertex_graphs_and_roots_match_transitive_closure_oracle() {
    // Floyd-Warshall is independent of the implementation's indexed worklist.
    // Scrambled allocation order checks deterministic identity ordering too.
    let allocations = [allocation(2), allocation(0), allocation(1)];
    for graph in 0..512 {
        let mut closure = [[false; 3]; 3];
        let mut core_edges = Vec::new();
        let mut payload_edges = Vec::new();
        for (from, row) in closure.iter_mut().enumerate() {
            for (to, connected) in row.iter_mut().enumerate() {
                *connected = graph & (1 << (from * 3 + to)) != 0;
                if *connected {
                    let target = if (from + to) % 2 == 0 {
                        &mut core_edges
                    } else {
                        &mut payload_edges
                    };
                    target.push(EdgeV1 {
                        from: object(from),
                        to: object(to),
                    });
                    target.push(EdgeV1 {
                        from: object(from),
                        to: object(to),
                    });
                }
            }
        }
        for via in 0..3 {
            for from in 0..3 {
                for to in 0..3 {
                    closure[from][to] |= closure[from][via] && closure[via][to];
                }
            }
        }
        let view = view(&allocations, &core_edges);
        for root_mask in 0..8 {
            let mut roots = vec![core::ptr::null_mut()];
            for index in 0..3 {
                if root_mask & (1 << index) != 0 {
                    roots.extend([object(index); 2]);
                }
            }
            let description = described(&view, &roots, &payload_edges);
            let mut captured = Capture::default();
            assert_eq!(
                unsafe { birnam_trace_described_v1(&description, &request(&mut captured)) },
                OK
            );
            let expected: Vec<_> = (0..3)
                .filter(|&to| {
                    root_mask & (1 << to) != 0
                        || (0..3).any(|from| root_mask & (1 << from) != 0 && closure[from][to])
                })
                .map(|index| AllocationRefV1 {
                    object: object(index),
                    allocation_identity: index as u64 + 1,
                })
                .collect();
            assert_eq!(
                captured.reached, expected,
                "graph={graph} roots={root_mask}"
            );
            assert_eq!(captured.reached.len() + captured.not_reached.len(), 3);
            assert!(
                captured
                    .not_reached
                    .iter()
                    .all(|entry| !expected.contains(entry))
            );
        }
    }
}

#[test]
fn coverage_is_strict_by_default_and_never_cleared() {
    let allocations = [allocation(0)];
    let mut view = view(&allocations, &[]);
    view.coverage_gaps =
        storage::NATIVE_ROOTS_UNOBSERVED | storage::DEFERRED_IDENTITIES_UNAVAILABLE;
    view.deferred_count = 5;
    view.deferred_bytes = 120;
    let mut description = described(&view, &[], &[]);
    description.coverage_gaps = 1 << 31; // future gap bits survive unchanged
    description.missing_description_count = 1;
    let expected = view.coverage_gaps | (1 << 31) | storage::PAYLOAD_DESCRIPTIONS_MISSING;
    let mut captured = Capture::default();
    let mut request = request(&mut captured);
    assert_eq!(
        unsafe { birnam_trace_described_v1(&description, &request) },
        INCOMPLETE
    );
    assert_eq!(captured.calls, 0);
    request.accept_gaps = expected;
    assert_eq!(
        unsafe { birnam_trace_described_v1(&description, &request) },
        OK
    );
    assert_eq!(captured.gaps, expected);
    assert_eq!(captured.not_reached.len(), 1); // no invented deferred Oops
    assert_eq!((view.deferred_count, view.deferred_bytes), (5, 120));
}

#[test]
fn malformed_graphs_never_publish_and_errors_leave_no_state() {
    let mut captured = Capture::default();
    for allocations in [
        vec![allocation(0), allocation(0)],
        vec![
            allocation(0),
            AllocationV1 {
                identity: 9,
                ..allocation(0)
            },
        ],
        vec![
            allocation(0),
            AllocationV1 {
                identity: 1,
                ..allocation(1)
            },
        ],
        vec![AllocationV1 {
            identity: 0,
            ..allocation(0)
        }],
        vec![AllocationV1 {
            object: core::ptr::null_mut(),
            ..allocation(0)
        }],
        vec![AllocationV1 {
            object: 1usize as Oop,
            ..allocation(0)
        }],
    ] {
        let view = view(&allocations, &[]);
        let description = described(&view, &[], &[]);
        assert_eq!(
            unsafe { birnam_trace_described_v1(&description, &request(&mut captured)) },
            INVALID
        );
    }
    let allocations = [allocation(0)];
    let foreign_edges = [EdgeV1 {
        from: object(0),
        to: object(1),
    }];
    let mut view = view(&allocations, &foreign_edges);
    let mut description = described(&view, &[], &[]);
    assert_eq!(
        unsafe { birnam_trace_described_v1(&description, &request(&mut captured)) },
        INVALID
    );
    view.edge_count = 0;
    description.storage = &view;
    description.payload_edges = foreign_edges.as_ptr();
    description.payload_edge_count = 1;
    assert_eq!(
        unsafe { birnam_trace_described_v1(&description, &request(&mut captured)) },
        INVALID
    );
    description.payload_edge_count = 0;
    let foreign_roots = [object(1)];
    description.roots = foreign_roots.as_ptr();
    description.root_count = 1;
    assert_eq!(
        unsafe { birnam_trace_described_v1(&description, &request(&mut captured)) },
        INVALID
    );
    description.roots = core::ptr::null();
    assert_eq!(
        unsafe { birnam_trace_described_v1(&description, &request(&mut captured)) },
        INVALID
    );
    description.root_count = 0;
    view.version = 8;
    description.storage = &view;
    assert_eq!(
        unsafe { birnam_trace_described_v1(&description, &request(&mut captured)) },
        INVALID
    );
    view.version = storage::VERSION;
    description.storage = &view;
    let mut invalid = request(&mut captured);
    invalid.visitor = None;
    assert_eq!(
        unsafe { birnam_trace_described_v1(&description, &invalid) },
        INVALID
    );
    invalid = request(&mut captured);
    invalid.byte_size -= 1;
    assert_eq!(
        unsafe { birnam_trace_described_v1(&description, &invalid) },
        INVALID
    );
    assert_eq!(
        unsafe { birnam_trace_described_v1(core::ptr::null(), &invalid) },
        INVALID
    );
    assert_eq!(captured.calls, 0);
    captured.status = -77;
    assert_eq!(
        unsafe { birnam_trace_described_v1(&description, &request(&mut captured)) },
        -77
    );
    captured.status = OK;
    captured.nested = true;
    assert_eq!(
        unsafe { birnam_trace_described_v1(&description, &request(&mut captured)) },
        OK
    );
    assert_eq!(captured.calls, 2);
    assert!(captured.reached.is_empty());
    assert_eq!(captured.not_reached.len(), 1);
}

#[test]
fn empty_graph_and_carrier_geometry() {
    assert_eq!(size_of::<AllocationRefV1>(), 16);
    assert_eq!(size_of::<RequestV1>(), 40);
    assert_eq!(size_of::<ReportV1>(), 80);
    let view = view(&[], &[]);
    let description = described(&view, &[], &[]);
    let mut captured = Capture::default();
    assert_eq!(
        unsafe { birnam_trace_described_v1(&description, &request(&mut captured)) },
        OK
    );
    assert_eq!(captured.calls, 1);
    assert!(captured.reached.is_empty() && captured.not_reached.is_empty());
}
