//! Call-window optimization versus explicit refusal, through actual emitted code.
use super::discarded_clone::{lookup, restore, run, vt};
use super::*;
use std::time::Instant;
const FAST: &[u8] = b"tryPrepareCall:slots:selector:in:tail:";
unsafe extern "C" fn decline(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::unit())
}
#[derive(Clone, Copy)]
enum Change {
    Method(Oop, Oop, NativeEntry),
    Binding(Oop, Oop),
}
struct Forward {
    id: Protocol,
    gc: Oop,
    original: NativeEntry,
    calls: Cell<usize>,
    collecting: bool,
    mutation: Cell<Option<Change>>,
}
unsafe extern "C" fn forward(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let f = &*c.cast::<Forward>();
        f.calls.set(f.calls.get() + 1);
        let answer = f.original.method.unwrap()(f.original.context, r, a, n);
        if f.collecting {
            collect(f.id, f.gc);
        }
        if let Some(change) = f.mutation.take() {
            match change {
                Change::Method(vt, selector, entry) => {
                    send(
                        f.id,
                        vt,
                        b"addMethod",
                        &[Operand::object(selector), Operand::entry(entry)],
                    );
                }
                Change::Binding(namespace, value) => {
                    send(
                        f.id,
                        namespace,
                        b"at:put:",
                        &[
                            Operand::object(f.id.intern_name(b"Integer").unwrap()),
                            Operand::object(value),
                        ],
                    );
                }
            }
        }
        answer
    }
}
unsafe extern "C" fn fail_unbound(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(6, 0)
}
struct Emit {
    id: Protocol,
    options: Oop,
}
unsafe extern "C" fn baseline_emitter(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    unsafe {
        let e = &*c.cast::<Emit>();
        let mut args = borrowed_slice(a, n).unwrap().to_vec();
        args.push(Operand::object(e.options));
        e.id.send_named(
            r,
            b"emit:source:filename:persistent:synchronous:optimizations:",
            &args,
        )
        .map_or_else(|e| e, Outcome::loaded)
    }
}
unsafe fn paused(h: &mut H, block: Oop) -> Vec<Vec<Oop>> {
    unsafe {
        let id = h.id;
        let machine = object(send(id, h.named(b"Machine"), b"new", &[]));
        h.keep(machine);
        let one = h.eval(b"1");
        let mut result = object(send(
            id,
            machine,
            b"runBlock:budget:",
            &[Operand::object(block), Operand::object(one)],
        ));
        let mut frames = Vec::new();
        for _ in 0..64 {
            if result != machine {
                return frames;
            }
            let view = object(send(id, machine, b"currentFrameView", &[]));
            h.keep(view);
            let count = h.integer(object(send(id, view, b"slotCount", &[]))) as usize;
            let mut values = Vec::new();
            for i in 0..count {
                let index = object(send(
                    id,
                    h.named(b"Integer"),
                    b"fromWord:",
                    &[Operand::word(i)],
                ));
                h.keep(index);
                let value = object(send(id, view, b"oopAt:", &[Operand::object(index)]));
                h.keep(value);
                // Each run owns a different Machine; source identities stay exact.
                values.push(if value == machine {
                    h.named(b"Machine")
                } else {
                    value
                });
            }
            frames.push(values);
            h.collect();
            result = object(send(
                id,
                machine,
                b"continueWithBudget:",
                &[Operand::object(one)],
            ));
        }
        panic!("bounded call did not finish");
    }
}
pub(super) unsafe fn check(h: &mut H) {
    unsafe {
        check_case(h, FAST, 61, 29, b"let first := [FrameCallee value: FrameArgument value: FrameArgument value: FrameArgument]; [FrameCallee value: first value: first value: first]");
        check_case(h, b"tryPrepareCall:slots:copies:selector:in:tail:", 125, 61,
            b"[| let callee := FrameCallee; let argument := FrameArgument; let one := argument; let two := one; let first := [callee value: two value: one value: argument]; let next := first; let last := next; [callee value: last value: next value: first]]");
    }
}
unsafe fn check_case(
    h: &mut H,
    fast_name: &[u8],
    candidate_mask: usize,
    baseline_mask: usize,
    source: &[u8],
) {
    unsafe {
        let id = h.id;
        let machine = h.named(b"Machine");
        let mvt = vt(h, machine);
        let fast = lookup(h, machine, fast_name);
        let argument = h.eval(b"[Object clone]");
        h.bind(b"FrameArgument", argument);
        let callee = h.eval(b"[:a :b :c | b]");
        h.bind(b"FrameCallee", callee);
        let backend = h.named(b"NativeBackend");
        let bvt = vt(h, backend);
        let emit_name = b"emit:source:filename:persistent:synchronous:";
        let emit = lookup(h, backend, emit_name);
        let options = object(send(
            id,
            h.named(b"Integer"),
            b"fromWord:",
            &[Operand::word(candidate_mask)],
        ));
        h.keep(options);
        let mut emitter = Emit { id, options };
        install(
            id,
            bvt,
            emit_name,
            baseline_emitter,
            ptr::from_mut(&mut emitter).cast(),
        );
        let block = if candidate_mask == 125 {
            h.eval(source)
        } else {
            let code = h.compile(source);
            h.block(code)
        };
        emitter.options = object(send(
            id,
            h.named(b"Integer"),
            b"fromWord:",
            &[Operand::word(baseline_mask)],
        ));
        h.keep(emitter.options);
        let plain_block = if candidate_mask == 125 {
            h.eval(source)
        } else {
            let code = h.compile(source);
            h.block(code)
        };
        restore(h, bvt, emit_name, emit);

        // Both a normal call and a tail call return the exact input identity.
        assert_eq!(run(h, block, false), argument);
        install(id, mvt, fast_name, decline, ptr::null_mut());
        assert_eq!(run(h, block, false), argument);
        restore(h, mvt, fast_name, fast);
        send(
            id,
            mvt,
            b"removeMethod:",
            &[Operand::object(id.intern_name(fast_name).unwrap())],
        );
        assert_eq!(
            run(h, block, false),
            argument,
            "missing optional protocol uses ordinary calls"
        );
        restore(h, mvt, fast_name, fast);
        let failure_machine = object(send(id, machine, b"new", &[]));
        h.keep(failure_machine);
        install(id, mvt, fast_name, fail_unbound, ptr::null_mut());
        let error = id
            .send_named(failure_machine, b"runBlock:", &[Operand::object(block)])
            .unwrap_err();
        assert_eq!(
            (error.status, error.detail),
            (6, 0),
            "entered UNBOUND must not replay baseline"
        );
        restore(h, mvt, fast_name, fast);
        // Replaced baseline operations must execute with the same effects.
        for (receiver, selector) in [
            (h.named(b"Integer"), b"fromWord:".as_slice()),
            (h.named(b"Integer"), b"wordOf:"),
            (machine, b"oopAt:"),
            (machine, b"oopAt:put:"),
            (machine, b"callSite:"),
            (machine, b"tailCallSite:"),
            (h.ns, b"at:"),
            (h.named(b"GC"), b"prepareAllocation"),
        ] {
            let behavior = vt(h, receiver);
            let mut f = Forward {
                id,
                gc: h.named(b"GC"),
                original: lookup(h, receiver, selector),
                calls: Cell::new(0),
                collecting: false,
                mutation: Cell::new(None),
            };
            install(
                id,
                behavior,
                selector,
                forward,
                ptr::from_mut(&mut f).cast(),
            );
            let mut counts = [0; 2];
            for optimized in [false, true] {
                if optimized {
                    restore(h, mvt, fast_name, fast);
                } else {
                    install(id, mvt, fast_name, decline, ptr::null_mut());
                }
                f.calls.set(0);
                assert_eq!(run(h, block, false), argument);
                counts[usize::from(optimized)] = f.calls.get();
            }
            assert!(
                counts[0] > 0,
                "{} was not exercised",
                String::from_utf8_lossy(selector)
            );
            assert_eq!(
                counts[0],
                counts[1],
                "{} effects differ",
                String::from_utf8_lossy(selector)
            );
            restore(h, behavior, selector, f.original);
        }
        let integers = h.named(b"Integer");
        let ivt = vt(h, integers);
        let original_guard = lookup(h, integers, b"canOmitIndexing");
        let mut factory = Forward {
            id,
            gc: h.named(b"GC"),
            original: lookup(h, integers, b"fromWord:"),
            calls: Cell::new(0),
            collecting: false,
            mutation: Cell::new(None),
        };
        let mut guard = Forward {
            id,
            gc: h.named(b"GC"),
            original: original_guard,
            calls: Cell::new(0),
            collecting: true,
            mutation: Cell::new(Some(Change::Method(
                ivt,
                id.intern_name(b"fromWord:").unwrap(),
                NativeEntry::new(forward, ptr::from_mut(&mut factory).cast()),
            ))),
        };
        install(
            id,
            ivt,
            b"canOmitIndexing",
            forward,
            ptr::from_mut(&mut guard).cast(),
        );
        assert_eq!(run(h, block, false), argument);
        assert!(
            factory.calls.get() > 0,
            "mutation after a positive grant must fall back"
        );
        restore(h, ivt, b"fromWord:", factory.original);
        // A collecting guard alone permits optimization without losing inputs.
        assert_eq!(run(h, block, false), argument);
        assert!(guard.calls.get() > 0);
        // Namespace rebinding need not change the behavior epoch. The final
        // factory fence must still refuse a previously approved omission.
        let alternate = h.eval(b"[Object clone]");
        let avt = vt(h, alternate);
        install(
            id,
            avt,
            b"fromWord:",
            forward,
            ptr::from_mut(&mut factory).cast(),
        );
        let original_reader = lookup(h, integers, b"wordOf:");
        restore(h, avt, b"wordOf:", original_reader);
        factory.calls.set(0);
        guard.mutation.set(Some(Change::Binding(h.ns, alternate)));
        assert_eq!(run(h, block, false), argument);
        h.bind(b"Integer", integers);
        assert!(
            factory.calls.get() > 0,
            "namespace rebinding must restore construction"
        );
        restore(h, ivt, b"canOmitIndexing", original_guard);
        // Bounded execution retains the ordinary inspectable Frame sequence.
        assert_eq!(run(h, block, true), argument);
        install(id, mvt, fast_name, decline, ptr::null_mut());
        let baseline_frames = paused(h, block);
        restore(h, mvt, fast_name, fast);
        let optimized_frames = paused(h, block);
        assert!(!baseline_frames.is_empty());
        assert_eq!(
            baseline_frames, optimized_frames,
            "paused frame slots must match exactly"
        );
        send(id, h.named(b"GC"), b"budget:", &[Operand::word(usize::MAX)]);
        let mut allocations = [0; 2];
        let mut nanos = [0; 2];
        for round in 0..4 {
            for optimized in if round % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                if optimized {
                    restore(h, mvt, fast_name, fast);
                } else {
                    install(id, mvt, fast_name, decline, ptr::null_mut());
                }
                h.collect();
                let before = send(id, id.receiver, b"allocationCount", &[])
                    .as_word()
                    .unwrap();
                let start = Instant::now();
                for _ in 0..25 {
                    assert_eq!(
                        run(h, if optimized { block } else { plain_block }, false),
                        argument
                    );
                }
                nanos[usize::from(optimized)] += start.elapsed().as_nanos();
                allocations[usize::from(optimized)] +=
                    send(id, id.receiver, b"allocationCount", &[])
                        .as_word()
                        .unwrap()
                        - before;
            }
        }
        restore(h, mvt, fast_name, fast);
        println!(
            "frame-call {candidate_mask}: baseline {}ns/{} allocations; optimized {}ns/{} allocations",
            nanos[0], allocations[0], nanos[1], allocations[1]
        );
        assert!(
            allocations[1] < allocations[0],
            "the real call sequence must allocate fewer objects"
        );
        send(id, h.named(b"GC"), b"budget:", &[Operand::word(16 << 20)]);
        h.bind(b"FrameArgument", h.named(b"nil"));
        h.bind(b"FrameCallee", h.named(b"nil"));
        h.clear();
        h.collect();
    }
}
