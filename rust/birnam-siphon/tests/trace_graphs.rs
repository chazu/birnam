//! Authored Trace against independent graph closure and adversarial providers.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{self as abi, NativeEntry, Operand, Outcome, Protocol},
};
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    os::unix::ffi::OsStrExt,
};
use support::*;

struct Graph {
    id: Protocol,
    roots_owner: Oop,
    gc: Oop,
    trace: Oop,
    visitor: Cell<Oop>,
    allocations: RefCell<Vec<(Oop, usize)>>,
    roots: RefCell<Vec<Oop>>,
    edges: RefCell<Vec<(Oop, Oop)>>,
    rows: RefCell<Vec<(Oop, usize, bool)>>,
    gaps: Cell<usize>,
    completed: Cell<usize>,
    mode: Cell<usize>,
    result_mode: Cell<usize>,
    saved: Cell<Oop>,
    result: Cell<Oop>,
    bytes: RefCell<Vec<u8>>,
    string: Oop,
    cleanup_mode: Cell<usize>,
    original_end: Cell<Option<NativeEntry>>,
}
unsafe extern "C" fn cleanup(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let g = unsafe { &*c.cast::<Graph>() };
    let original = g.original_end.get().unwrap();
    let answer = unsafe { (original.method.unwrap())(original.context, r, a, n) };
    g.bytes.borrow_mut().fill(b'!');
    unsafe { collect(g.id, g.gc) };
    if g.cleanup_mode.get() == 2 {
        return Outcome::failed(924, 76);
    }
    answer
}
unsafe extern "C" fn observe(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let g = &*c.cast::<Graph>();
        let [sink] = borrowed_slice(a, n).unwrap() else {
            panic!("observe arity")
        };
        let sink = object(*sink);
        if g.saved.get().is_null() {
            g.saved.set(sink);
            retain(g.id, g.roots_owner, sink);
        }
        if g.mode.get() == 1 {
            // Ignore a malformed carrier failure. Trace must still reject.
            assert!(
                g.id.send_named(
                    sink,
                    b"allocation:bytes:identity:",
                    &[
                        Operand::object(g.trace),
                        Operand::object(g.trace),
                        Operand::word(9),
                    ]
                )
                .is_err()
            );
        }
        if g.mode.get() == 2 {
            return Ok(Operand::word(44));
        } // no completion
        for &(o, identity) in g.allocations.borrow().iter() {
            g.id.send_named(
                sink,
                b"allocation:bytes:identity:",
                &[
                    Operand::object(o),
                    Operand::word(0),
                    Operand::word(identity),
                ],
            )?;
        }
        for &root in g.roots.borrow().iter() {
            g.id.send_named(sink, b"root:", &[Operand::object(root)])?;
        }
        for &(from, to) in g.edges.borrow().iter() {
            g.id.send_named(
                sink,
                b"edgeFrom:to:",
                &[Operand::object(from), Operand::object(to)],
            )?;
        }
        let answer =
            g.id.send_named(sink, b"complete:", &[Operand::word(g.gaps.get())]);
        if g.mode.get() == 3 {
            return Ok(Operand::word(44));
        } // ignore report failure
        answer
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
unsafe fn row(c: *mut c_void, a: *const Operand, n: usize, marked: bool) -> Outcome {
    let g = unsafe { &*c.cast::<Graph>() };
    let [o, identity] = (unsafe { borrowed_slice(a, n) }).unwrap() else {
        panic!("row")
    };
    g.rows
        .borrow_mut()
        .push((object(*o), identity.as_word().unwrap(), marked));
    if g.result_mode.get() == 5 {
        return Outcome::failed(921, 73);
    }
    Outcome::loaded(Operand::unit())
}
unsafe extern "C" fn reached(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe { row(c, a, n, true) }
}
unsafe extern "C" fn unreached(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe { row(c, a, n, false) }
}
unsafe extern "C" fn echo_argument(_: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    let [bytes] = (unsafe { borrowed_slice(a, n) }).unwrap() else {
        panic!("byte echo arity")
    };
    assert!(bytes.as_bytes().is_some());
    // The output aliases the adapter-owned input buffer, not a global buffer.
    Outcome::loaded(*bytes)
}
unsafe extern "C" fn finish(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    let g = unsafe { &*c.cast::<Graph>() };
    let [gaps] = (unsafe { borrowed_slice(a, n) }).unwrap() else {
        panic!("complete")
    };
    assert_eq!(gaps.as_word(), Some(g.gaps.get()));
    g.completed.set(g.completed.get() + 1);
    match g.result_mode.get() {
        1 => Outcome::loaded(Operand::unit()),
        2 => {
            let result = unsafe {
                send(
                    g.id,
                    g.string,
                    b"fromBytes:",
                    &[Operand::bytes(b"fresh completion")],
                )
            };
            g.result.set(object(result));
            Outcome::loaded(result)
        }
        3 => Outcome::loaded(Operand::bytes(&g.bytes.borrow())),
        4 => Outcome::failed(923, 75),
        6 => Outcome::loaded(Operand::entry(NativeEntry::new(finish, c))),
        7 => Outcome::loaded(Operand::protocol(&g.id)),
        8 => Outcome::loaded(Operand::object(std::ptr::null_mut())),
        _ => Outcome::loaded(Operand::word(usize::MAX)),
    }
}
unsafe fn report(g: &Graph, mask: usize) -> Result<Operand, Outcome> {
    g.rows.borrow_mut().clear();
    g.completed.set(0);
    unsafe {
        g.id.send_named(
            g.trace,
            b"report:acceptingGaps:",
            &[Operand::object(g.visitor.get()), Operand::word(mask)],
        )
    }
}
#[test]
#[ignore = "requires installed authored Trace"]
fn authored_graphs_preserve_identity_validation_failure_and_native_results() {
    unsafe {
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        let named = |name: &[u8]| {
            object(send(
                id,
                ns,
                b"at:",
                &[Operand::object(id.intern_name(name).unwrap())],
            ))
        };
        let roots = named(b"Roots");
        let gc = named(b"GC");
        send(id, gc, b"automaticCollection:", &[Operand::word(0)]);
        send(id, gc, b"precisePayloads:", &[Operand::word(1)]);
        let g = Box::leak(Box::new(Graph {
            id,
            roots_owner: roots,
            gc,
            trace: named(b"Trace"),
            visitor: Cell::new(std::ptr::null_mut()),
            allocations: RefCell::new(vec![]),
            roots: RefCell::new(vec![]),
            edges: RefCell::new(vec![]),
            rows: RefCell::new(vec![]),
            gaps: Cell::new(1),
            completed: Cell::new(0),
            mode: Cell::new(0),
            result_mode: Cell::new(0),
            saved: Cell::new(std::ptr::null_mut()),
            result: Cell::new(std::ptr::null_mut()),
            bytes: RefCell::new(b"native completion bytes".to_vec()),
            string: named(b"String"),
            cleanup_mode: Cell::new(0),
            original_end: Cell::new(None),
        }));
        let c = (g as *mut Graph).cast();
        let storage = named(b"Storage");
        let storage_vt = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(storage)],
        ));
        install(id, storage_vt, b"observe:", observe, c);
        let visitor_vt = behavior(id);
        install(id, visitor_vt, b"reached:identity:", reached, c);
        install(id, visitor_vt, b"notReached:identity:", unreached, c);
        install(id, visitor_vt, b"complete:", finish, c);
        g.visitor.set(allocate(id, visitor_vt, 0));
        retain(id, roots, g.visitor.get());
        // Equal-valued, distinct String objects must remain distinct graph nodes.
        let mut nodes = vec![];
        for _ in 0..4 {
            let node = object(send(
                id,
                named(b"String"),
                b"fromBytes:",
                &[Operand::bytes(b"same")],
            ));
            retain(id, roots, node);
            nodes.push(node);
        }
        g.result.set(nodes[3]);
        let normal = vec![
            (nodes[2], usize::MAX),
            (nodes[0], 1),
            (nodes[1], 1usize << 63),
        ];
        *g.allocations.borrow_mut() = normal.clone();
        // Each three-node graph/root set is checked against independent bit closure.
        for edges in 0..512usize {
            *g.edges.borrow_mut() = (0..3)
                .flat_map(|i| {
                    (0..3).filter_map(move |j| (edges & (1 << (i * 3 + j)) != 0).then_some((i, j)))
                })
                .map(|(i, j)| (nodes[i], nodes[j]))
                .collect();
            for roots in 0..8usize {
                *g.roots.borrow_mut() = (0..3)
                    .filter(|i| roots & (1 << i) != 0)
                    .map(|i| nodes[i])
                    .collect();
                let mut closure = roots;
                loop {
                    let before = closure;
                    for i in 0..3 {
                        for j in 0..3 {
                            if closure & (1 << i) != 0 && edges & (1 << (i * 3 + j)) != 0 {
                                closure |= 1 << j;
                            }
                        }
                    }
                    if before == closure {
                        break;
                    }
                }
                assert_eq!(
                    report(g, 1).unwrap().as_word(),
                    Some(usize::MAX),
                    "edges={edges} roots={roots}"
                );
                assert_eq!(
                    &*g.rows.borrow(),
                    &vec![
                        (nodes[0], 1, closure & 1 != 0),
                        (nodes[1], 1usize << 63, closure & 2 != 0),
                        (nodes[2], usize::MAX, closure & 4 != 0),
                    ]
                );
                if roots == 7 {
                    collect(id, g.gc);
                }
            }
        }
        g.roots.borrow_mut().clear();
        g.edges.borrow_mut().clear();
        // Foreign roots and even unreachable foreign edges fail before reports.
        for kind in 0..5 {
            *g.allocations.borrow_mut() = normal.clone();
            match kind {
                0 => g.roots.borrow_mut().push(nodes[3]),
                1 => g.edges.borrow_mut().push((nodes[2], nodes[3])),
                2 => g.allocations.borrow_mut().push((nodes[3], 1)),
                3 => g.allocations.borrow_mut().push((nodes[0], 7)),
                _ => g.allocations.borrow_mut()[0].1 = 0,
            }
            assert_eq!(report(g, 1).unwrap_err().status, abi::INVALID_REQUEST);
            assert!(g.rows.borrow().is_empty());
            g.roots.borrow_mut().clear();
            g.edges.borrow_mut().clear();
        }
        *g.allocations.borrow_mut() = normal;
        g.roots.borrow_mut().extend([nodes[0], nodes[0]]);
        g.edges.borrow_mut().extend([
            (nodes[0], nodes[1]),
            (nodes[0], nodes[1]),
            (nodes[1], nodes[0]),
        ]);
        report(g, 1).unwrap();
        assert_eq!(
            g.rows.borrow().iter().map(|r| r.2).collect::<Vec<_>>(),
            vec![true, true, false]
        );
        assert!(
            id.send_named(g.saved.get(), b"root:", &[Operand::object(nodes[0])])
                .is_err()
        );
        for mode in [1, 2, 3] {
            g.mode.set(mode);
            g.result_mode.set(if mode == 3 { 5 } else { 0 });
            let error = report(g, 1).unwrap_err();
            if mode == 3 {
                assert_eq!((error.status, error.detail), (921, 73));
            }
            g.mode.set(0);
            g.result_mode.set(0);
            report(g, 1).unwrap();
        }
        g.gaps.set(1usize << 63);
        assert_eq!(report(g, 1).unwrap_err().detail, 3);
        assert!(g.rows.borrow().is_empty());
        report(g, usize::MAX).unwrap();
        g.gaps.set(1);
        // Cleanup may collect and overwrite a provider's borrowed byte buffer.
        let trace_vt = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(g.trace)],
        ));
        g.original_end.set(Some(
            send(
                id,
                id.receiver,
                b"bind:to:",
                &[
                    Operand::object(id.intern_name(b"end:").unwrap()),
                    Operand::object(g.trace),
                ],
            )
            .as_entry()
            .unwrap(),
        ));
        send(
            id,
            trace_vt,
            b"addMethod",
            &[
                Operand::object(id.intern_name(b"originalEnd:").unwrap()),
                Operand::entry(g.original_end.get().unwrap()),
            ],
        );
        install(id, trace_vt, b"end:", cleanup, c);
        g.cleanup_mode.set(1);
        for mode in [0, 1, 2, 3, 4, 6, 7, 8] {
            g.result_mode.set(mode);
            *g.bytes.borrow_mut() = b"native completion bytes".to_vec();
            let result = report(g, 1);
            match mode {
                0 => assert_eq!(result.unwrap().as_word(), Some(usize::MAX)),
                1 => assert_eq!(result.unwrap().kind(), abi::UNIT),
                2 => {
                    assert_eq!(result.unwrap().as_object(), Some(g.result.get()));
                    assert!(present(id, g.result.get()));
                }
                3 => {
                    let raw = result.unwrap().as_bytes().unwrap();
                    assert_eq!(
                        birnam_abi::borrowed_bytes(raw.pointer, raw.length).unwrap(),
                        b"native completion bytes"
                    );
                }
                6 => assert_eq!(
                    result.unwrap().as_entry(),
                    Some(NativeEntry::new(finish, c))
                ),
                7 => assert_eq!(
                    result.unwrap().as_protocol(),
                    Some(&g.id as *const Protocol)
                ),
                8 => assert_eq!(result.unwrap().as_object(), Some(std::ptr::null_mut())),
                _ => {
                    let e = result.unwrap_err();
                    assert_eq!((e.status, e.detail), (923, 75));
                }
            }
        }
        g.cleanup_mode.set(2);
        g.result_mode.set(0);
        let error = report(g, 1).unwrap_err();
        assert_eq!((error.status, error.detail), (924, 76));
        g.result_mode.set(4);
        let error = report(g, 1).unwrap_err();
        assert_eq!((error.status, error.detail), (923, 75));
        g.cleanup_mode.set(1);
        g.result_mode.set(0);
        g.allocations.borrow_mut().clear();
        g.roots.borrow_mut().clear();
        g.edges.borrow_mut().clear();
        report(g, 1).unwrap();
        assert!(g.rows.borrow().is_empty());
        assert_eq!(g.completed.get(), 1);

        // Deferred native messages also preserve a result borrowing an argument.
        // MallocScribble=1 makes freeing the argument before copying observable.
        install(
            id,
            visitor_vt,
            b"echoBytes:",
            echo_argument,
            std::ptr::null_mut(),
        );
        send(
            id,
            ns,
            b"at:put:",
            &[
                Operand::object(id.intern_name(b"TraceCarrierTarget").unwrap()),
                Operand::object(g.visitor.get()),
            ],
        );
        let source=object(send(id,named(b"String"),b"fromBytes:",&[Operand::bytes(br#"
            let carrierRequest := [Object clone];
            [Reflect defineMethod: #value on: carrierRequest as: [Execution block: [:self | self] withNativeSignature: "m"]];
            [Reflect defineMethod: #nativeReceiver on: carrierRequest as: [:self | TraceCarrierTarget]];
            [Reflect defineMethod: #nativeSelector on: carrierRequest as: [:self | #echoBytes:]];
            [Reflect defineMethod: #nativeArguments on: carrierRequest as: [:self | #[["borrowed argument survives" asBytes]]]];
            [Reflect defineMethod: #nativeSignature on: carrierRequest as: [:self | "vb"]];
            [Reflect defineMethod: #nativeCleanup on: carrierRequest as: [:self | [| [System gc]]]];
            carrierRequest
        "#)]));
        let source_root = retain(id, roots, source);
        let request = object(send(
            id,
            named(b"Codegen"),
            b"eval:",
            &[Operand::object(source)],
        ));
        let request_root = retain(id, roots, request);
        let result = send(id, request, b"value", &[]).as_bytes().unwrap();
        assert_eq!(
            birnam_abi::borrowed_bytes(result.pointer, result.length).unwrap(),
            b"borrowed argument survives"
        );
        release(id, roots, request_root);
        release(id, roots, source_root);
    }
}
