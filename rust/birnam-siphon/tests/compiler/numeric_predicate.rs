//! The real source producer and Integer/Execution consumer, against an unannotated
//! baseline. Inventory counts are actual objects, never synthetic elision charges.
use super::discarded_clone::{instances, lookup, restore, run, vt};
use super::*;
use std::time::Instant;
const ATTACH: &[u8] = b"withNumericPredicate:then:left:right:limit:return:in:";
const EVALUATE: &[u8] = b"evaluatePredicate:then:left:right:limit:in:epoch:with:";
unsafe extern "C" fn unannotated(_: *mut c_void, r: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::object(r))
}
unsafe fn number(h: &mut H, value: i64) -> Oop {
    unsafe {
        let v = object(send(
            h.id,
            h.named(b"Integer"),
            b"fromWord:",
            &[Operand::word(value as usize)],
        ));
        h.keep(v)
    }
}
struct Forward {
    id: Protocol,
    gc: Oop,
    original: NativeEntry,
    calls: Cell<usize>,
    collect: bool,
    change: Option<(Oop, Oop, NativeEntry)>,
}
unsafe extern "C" fn forward(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Forward>();
        p.calls.set(p.calls.get() + 1);
        if p.collect {
            collect(p.id, p.gc);
        }
        if let Some((behavior, selector, entry)) = p.change {
            send(
                p.id,
                behavior,
                b"addMethod",
                &[Operand::object(selector), Operand::entry(entry)],
            );
        }
        p.original.method.unwrap()(p.original.context, r, a, n)
    }
}
unsafe fn inputs(h: &mut H, values: [i64; 3]) -> [Oop; 3] {
    unsafe {
        let v = values.map(|v| number(h, v));
        for (name, value) in [b"PredicateA".as_slice(), b"PredicateB", b"PredicateC"]
            .into_iter()
            .zip(v)
        {
            h.bind(name, value);
        }
        v
    }
}
unsafe fn source(first: &str, then: &str) -> Vec<u8> {
    format!("[:a :b :c | let answer := [[a {first} b] {then} c]; answer]").into_bytes()
}
unsafe fn paused(h: &mut H, block: Oop, intermediate: i64, expected: Oop) {
    unsafe {
        let machine = object(send(h.id, h.named(b"Machine"), b"new", &[]));
        h.keep(machine);
        let one = number(h, 1);
        let integer_vt = h.named(b"IntegerVt");
        let mut observed = false;
        let mut value = object(send(
            h.id,
            machine,
            b"runBlock:budget:",
            &[Operand::object(block), Operand::object(one)],
        ));
        for _ in 0..24 {
            if value != machine {
                break;
            }
            let view = object(send(h.id, machine, b"currentFrameView", &[]));
            h.keep(view);
            if view != h.named(b"nil") {
                let count = h.integer(object(send(h.id, view, b"slotCount", &[])));
                for i in 0..count {
                    let index = number(h, i);
                    let value = object(send(h.id, view, b"oopAt:", &[Operand::object(index)]));
                    if vt(h, value) == integer_vt && h.integer(value) == intermediate {
                        h.collect();
                        assert!(present(h.id, value));
                        observed = true;
                    }
                }
            }
            value = object(send(
                h.id,
                machine,
                b"continueWithBudget:",
                &[Operand::object(one)],
            ));
        }
        assert_eq!(value, expected);
        assert!(observed, "paused baseline exposes the intermediate Integer");
    }
}
struct CommitProbe {
    id: Protocol,
    gc: Oop,
    roots: Oop,
    integer: Oop,
    machine: Oop,
    calls: Cell<usize>,
    expected: Oop,
}
unsafe extern "C" fn after_commit(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<CommitProbe>();
        let args = borrowed_slice(a, n).unwrap();
        let result = p.id.send_named(r, b"savedPredicateCommit:", args).unwrap();
        assert_eq!(result.kind(), 0);
        assert!(p.id.send_named(r, b"savedPredicateCommit:", args).is_err(), "a result is committed only once");
        // Publication precedes owner cleanup: collect and inspect the caller
        // while the Integer query is still entered, before it returns Unit.
        collect(p.id, p.gc);
        let view = object(send(p.id, p.machine, b"currentFrameView", &[]));
        let root = retain(p.id, p.roots, view);
        let count = send(p.id, view, b"slotCount", &[]);
        let count = send(p.id, p.integer, b"wordOf:", &[count])
            .as_word()
            .unwrap();
        let mut found = false;
        for i in 0..count {
            let index = send(p.id, p.integer, b"fromWord:", &[Operand::word(i)]);
            found |= send(p.id, view, b"oopAt:", &[index]).as_object() == Some(p.expected);
        }
        release(p.id, p.roots, root);
        assert!(
            found,
            "Boolean is already published in the caller during owner callback"
        );
        p.calls.set(p.calls.get() + 1);
        Outcome::loaded(result)
    }
}
pub(super) unsafe fn check(h: &mut H) {
    unsafe {
        h.clear();
        h.collect();
        let gc = h.named(b"GC");
        let integer = h.named(b"Integer");
        let integer_vt = h.named(b"IntegerVt");
        let model = h.named(b"ObjectModel");
        let truth = h.named(b"true");
        let falsity = h.named(b"false");
        let method = h.named(b"Method");
        let method_vt = vt(h, method);
        let attach = lookup(h, method, ATTACH);
        let candidate = h.eval(&source("+", "<"));
        install(h.id, method_vt, ATTACH, unannotated, ptr::null_mut());
        let baseline = h.eval(&source("+", "<"));
        restore(h, method_vt, ATTACH, attach);
        h.bind(b"NumericPredicate", candidate);
        inputs(h, [4, 5, 20]);
        let caller=h.compile(b"let predicateAnswer := [NumericPredicate value: PredicateA value: PredicateB value: PredicateC]; predicateAnswer");
        let block = h.block(caller);
        send(h.id, gc, b"budget:", &[Operand::word(usize::MAX)]);
        let before = instances(h, integer_vt);
        assert_eq!(run(h, block, false), truth);
        let optimized = instances(h, integer_vt) - before;
        h.bind(b"NumericPredicate", baseline);
        let before = instances(h, integer_vt);
        assert_eq!(run(h, block, false), truth);
        let ordinary = instances(h, integer_vt) - before;
        let native_arithmetic = object(send(h.id,integer,b"hasBootstrapArithmetic:",&[Operand::object(h.id.intern_name(b"+").unwrap())])) == truth;
        if native_arithmetic {
            assert!(optimized < ordinary, "native predicate must remove Integer allocations: {optimized} vs {ordinary}");
        } else {
            assert_eq!(optimized,ordinary,"authored arithmetic must use its actual messages, not the Rust numeric shortcut");
        }
        h.bind(b"NumericPredicate", candidate);
        paused(h, block, 9, truth);

        // Checked arithmetic, signed extrema and existing comparison rounding.
        for (first, then, values, expected) in [
            ("+", "<", [4, 5, 20], truth),
            ("-", ">", [4, 5, 0], falsity),
            ("*", "<=", [-3, 7, -21], truth),
            ("/", ">=", [-11, 3, -3], truth),
            ("mod:", "=", [i64::MIN, -1, 0], truth),
            ("+", "<", [1 << 53, 0, (1 << 53) + 1], falsity),
            ("+", "=", [1 << 53, 0, (1 << 53) + 1], falsity),
        ] {
            let predicate = h.eval(&source(first, then));
            h.bind(b"NumericPredicate", predicate);
            inputs(h, values);
            assert_eq!(run(h, block, false), expected);
        }
        for (first, values, kind) in [
            ("+", [i64::MAX, 1, 0], b"overflow".as_slice()),
            ("/", [1, 0, 0], b"zeroDivide"),
            ("/", [i64::MIN, -1, 0], b"overflow"),
        ] {
            let predicate = h.eval(&source(first, "<"));
            h.bind(b"NumericPredicate", predicate);
            inputs(h, values);
            let result = run(h, block, false);
            assert_eq!(
                send(h.id, result, b"kind", &[]).as_object(),
                Some(h.id.intern_name(kind).unwrap())
            );
        }
        h.bind(b"NumericPredicate", candidate);
        let original_inputs = inputs(h, [4, 5, 20]);

        for (receiver, selector) in [
            (original_inputs[0], b"+".as_slice()),
            (original_inputs[0], b"<"),
            (integer, b"fromWord:"),
            (integer, b"wordOf:"),
            (model, b"allocate:bytes:"),
            (gc, b"prepareAllocation"),
            (h.id.receiver, b"allocate:bytes:"),
            (h.id.receiver, b"lookup"),
        ] {
            let original = lookup(h, receiver, selector);
            let behavior = vt(h, receiver);
            let mut p = Box::new(Forward {
                id: h.id,
                gc,
                original,
                calls: Cell::new(0),
                collect: false,
                change: None,
            });
            install(
                h.id,
                behavior,
                selector,
                forward,
                ptr::from_mut(&mut *p).cast(),
            );
            let before = instances(h, integer_vt);
            assert_eq!(run(h, block, false), truth);
            assert!(
                p.calls.get() > 0,
                "current replacement {selector:?} executes"
            );
            if selector == b"+" || selector == b"<" {
                assert_eq!(p.calls.get(), 1);
            }
            assert!(
                instances(h, integer_vt) - before >= ordinary,
                "replacement takes baseline"
            );
            restore(h, behavior, selector, original);
        }

        // A numeric result and an inspectable intermediate binding use baseline.
        let returning_number = h.eval(&source("+", "*"));
        h.bind(b"NumericPredicate", returning_number);
        assert_eq!(h.integer(run(h, block, false)), 180);
        let bound = h.eval(b"[:a :b :c | let sum := [a + b]; let answer := [sum < c]; answer]");
        h.bind(b"NumericPredicate", bound);
        let before = instances(h, integer_vt);
        assert_eq!(run(h, block, false), truth);
        assert!(instances(h, integer_vt) - before >= ordinary);
        h.bind(b"NumericPredicate", candidate);

        // Alias recognition relies on selected entries, not selector spelling.
        let add = lookup(h, original_inputs[0], b"+");
        let less = lookup(h, original_inputs[0], b"<");
        install(h.id, integer_vt, b"sum:", add.method.unwrap(), add.context);
        install(
            h.id,
            integer_vt,
            b"below:",
            less.method.unwrap(),
            less.context,
        );
        let alias = h.eval(&source("sum:", "below:"));
        h.bind(b"NumericPredicate", alias);
        let before = instances(h, integer_vt);
        assert_eq!(run(h, block, false), truth);
        assert_eq!(instances(h, integer_vt) - before, optimized);
        h.bind(b"NumericPredicate", candidate);

        // Guard-time collection and replacement happen before consuming the body.
        let evaluate = lookup(h, integer, EVALUATE);
        let mut changed = Box::new(Forward {
            id: h.id,
            gc,
            original: add,
            calls: Cell::new(0),
            collect: false,
            change: None,
        });
        let mut checking = Box::new(Forward {
            id: h.id,
            gc,
            original: evaluate,
            calls: Cell::new(0),
            collect: true,
            change: None,
        });
        install(
            h.id,
            integer_vt,
            EVALUATE,
            forward,
            ptr::from_mut(&mut *checking).cast(),
        );
        assert_eq!(run(h, block, false), truth);
        assert!(checking.calls.get() > 0);
        for value in original_inputs {
            assert!(present(h.id, value));
        }
        checking.collect = false;
        checking.change = Some((
            integer_vt,
            h.id.intern_name(b"+").unwrap(),
            NativeEntry::new(forward, ptr::from_mut(&mut *changed).cast()),
        ));
        assert_eq!(run(h, block, false), truth);
        assert_eq!(changed.calls.get(), 1);
        restore(h, integer_vt, b"+", add);
        restore(h, integer_vt, EVALUATE, evaluate);

        let machine = object(send(h.id, h.named(b"Machine"), b"new", &[]));
        h.keep(machine);
        let mut probe = Box::new(CommitProbe {
            id: h.id,
            gc,
            roots: h.roots,
            integer,
            machine,
            calls: Cell::new(0),
            expected: truth,
        });
        let execution = h.named(b"Execution");
        let execution_vt = vt(h, execution);
        let commit = lookup(h, execution, b"commitNumericPredicate:");
        install(h.id, execution_vt, b"savedPredicateCommit:", commit.method.unwrap(), commit.context);
        install(h.id, execution_vt, b"commitNumericPredicate:", after_commit, ptr::from_mut(&mut *probe).cast());
        assert!(h.id.send_named(execution, b"savedPredicateCommit:", &[Operand::object(truth)]).is_err());
        assert_eq!(
            object(send(h.id, machine, b"runBlock:", &[Operand::object(block)])),
            truth
        );
        assert_eq!(probe.calls.get(), usize::from(native_arithmetic), "authored arithmetic must not publish through the Rust predicate consumer");
        restore(h, execution_vt, b"commitNumericPredicate:", commit);
        send(h.id, execution_vt, b"removeMethod:", &[Operand::object(h.id.intern_name(b"savedPredicateCommit:").unwrap())]);

        // Effectful lookup cannot run between the owner's final reads and commit.
        let execution_lookup = lookup(h, execution_vt, b"lookup");
        let mut lookup_probe = Forward { id: h.id, gc, original: execution_lookup, calls: Cell::new(0), collect: false, change: None };
        let old_meta = vt(h, execution_vt);
        let custom_meta = object(send(h.id, old_meta, b"delegated", &[]));
        h.keep(custom_meta);
        install(h.id, custom_meta, b"lookup", forward, ptr::from_mut(&mut lookup_probe).cast());
        send(h.id, h.id.receiver, b"reassociateBehaviorOf:with:", &[Operand::object(execution_vt), Operand::object(custom_meta)]);
        let before = instances(h, integer_vt);
        assert_eq!(run(h, block, false), truth);
        assert!(instances(h, integer_vt) - before >= ordinary);
        send(h.id, h.id.receiver, b"reassociateBehaviorOf:with:", &[Operand::object(execution_vt), Operand::object(old_meta)]);

        // Live error-family designation must retain ordinary error propagation.
        let error_vt = h.named(b"ErrorVt");
        h.bind(b"ErrorVt", integer_vt);
        assert_eq!(run(h, block, false), original_inputs[2]);
        h.bind(b"ErrorVt", error_vt);
        let float = h.eval(b"4.5");
        h.bind(b"PredicateA", float);
        assert_eq!(run(h, block, false), truth);
        h.bind(b"PredicateA", original_inputs[0]);

        let mut elapsed = [0u128; 2];
        let mut allocations = [0usize; 2];
        for round in 0..6 {
            for mode in [round % 2, 1 - round % 2] {
                h.bind(
                    b"NumericPredicate",
                    if mode == 0 { baseline } else { candidate },
                );
                h.collect();
                let before = send(h.id, h.id.receiver, b"allocationCount", &[])
                    .as_word()
                    .unwrap();
                let start = Instant::now();
                for _ in 0..30 {
                    assert_eq!(run(h, block, false), truth);
                }
                elapsed[mode] += start.elapsed().as_nanos();
                allocations[mode] += send(h.id, h.id.receiver, b"allocationCount", &[])
                    .as_word()
                    .unwrap()
                    - before;
            }
        }
        if native_arithmetic { assert!(allocations[1] < allocations[0]); }
        else { assert!(allocations[1] >= allocations[0],"refused native predicate must not elide authored arithmetic"); }
        eprintln!(
            "numeric-predicate: 180 runs baseline={}ns/{} allocations optimized={}ns/{} allocations; Integer boxes/run {ordinary}->{optimized}",
            elapsed[0], allocations[0], elapsed[1], allocations[1]
        );
        for name in [
            b"NumericPredicate".as_slice(),
            b"PredicateA",
            b"PredicateB",
            b"PredicateC",
            b"predicateAnswer",
        ] {
            h.bind(name, h.named(b"nil"));
        }
        h.clear();
        h.collect();
        assert_eq!(h.units(), 0);
        assert_eq!(h.native_bytes(), 0);
        send(h.id, gc, b"budget:", &[Operand::word(1 << 20)]);
    }
}
