//! Native tier consumers exercise real Machine dispatch, not compiler setup.
use super::*;
unsafe fn install_steps(h: &mut H, baseline: Oop, steps: &[Oop]) {
    unsafe {
        let sequence = h.vector(steps);
        send(
            h.id,
            baseline,
            b"optimizeWith:",
            &[Operand::object(sequence)],
        );
    }
}
unsafe fn executable(h: &mut H, owner: Oop) -> Oop {
    unsafe {
        let value = load(
            h.id,
            "execution_machine_fixture",
            &[
                Operand::object(h.ns),
                Operand::object(owner),
                Operand::word(8),
            ],
        );
        h.keep(value)
    }
}
pub(super) unsafe fn check(h: &mut H) {
    unsafe {
        let baseline_owner = allocate(h.id, behavior(h.id), 0);
        h.keep(baseline_owner);
        let optimized_owner = allocate(h.id, behavior(h.id), 0);
        h.keep(optimized_owner);
        let baseline = h.method("minimal-return", 1, baseline_owner);
        let step = executable(h, optimized_owner);
        install_steps(h, baseline, &[step]);
        let block = h.block(baseline, &[]);
        for _ in 0..63 {
            assert_eq!(h.invoke(baseline, block), baseline_owner);
        }
        assert_eq!(h.invoke(baseline, block), optimized_owner);
        collect(h.id, h.gc);
        assert_eq!(h.invoke(baseline, block), optimized_owner);
        // Invalid installation preserves the current candidate and hotness.
        let bad = h.vector(&[step, step]);
        assert!(
            h.id.send_named(baseline, b"optimizeWith:", &[Operand::object(bad)])
                .is_err()
        );
        assert_eq!(h.invoke(baseline, block), optimized_owner);
        let machine = h.machine();
        let vt = object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(machine)],
        ));
        let selector = h.id.intern_name(b"callSite:").unwrap();
        let original = send(
            h.id,
            h.id.receiver,
            b"bind:to:",
            &[Operand::object(selector), Operand::object(machine)],
        )
        .as_entry()
        .unwrap();
        install(h.id, vt, b"callSite:", unused, ptr::null_mut());
        assert_eq!(h.invoke(baseline, block), baseline_owner);
        send(
            h.id,
            vt,
            b"addMethod",
            &[Operand::object(selector), Operand::entry(original)],
        );
        assert_eq!(
            h.invoke(baseline, block),
            baseline_owner,
            "withdrawn candidate stays disabled"
        );
        install_steps(h, baseline, &[step]);
        for _ in 0..64 {
            h.invoke(baseline, block);
        }
        assert_eq!(h.invoke(baseline, block), optimized_owner);
        install_steps(h, baseline, &[]);
        assert_eq!(h.invoke(baseline, block), baseline_owner);
        numeric(h);
        active_lifetime(h);
        retirement_churn(h);
    }
}
unsafe extern "C" fn unused(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::unit())
}
struct Observer {
    id: Protocol,
    gc: Oop,
    entry: NativeEntry,
    calls: Cell<usize>,
    evaluated: Cell<usize>,
}
unsafe extern "C" fn observe(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let c = &*c.cast::<Observer>();
        c.calls.set(c.calls.get() + 1);
        collect(c.id, c.gc);
        let mut args = borrowed_slice(a, n).unwrap().to_vec();
        let mut proxy = Proxy {
            observer: c,
            entry: args[3].as_entry().unwrap(),
        };
        args[3] = Operand::entry(NativeEntry::new(
            evaluated,
            ptr::from_mut(&mut proxy).cast(),
        ));
        c.entry.method.unwrap()(c.entry.context, r, args.as_ptr(), args.len())
    }
}
unsafe fn numeric(h: &mut H) {
    unsafe {
        let integer = h.named(b"Integer");
        let vt = object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(integer)],
        ));
        let selector =
            h.id.intern_name(b"evaluate:receiver:argument:with:")
                .unwrap();
        let original = send(
            h.id,
            h.id.receiver,
            b"bind:to:",
            &[Operand::object(selector), Operand::object(integer)],
        )
        .as_entry()
        .unwrap();
        let observer = Box::leak(Box::new(Observer {
            id: h.id,
            gc: h.gc,
            entry: original,
            calls: Cell::new(0),
            evaluated: Cell::new(0),
        }));
        install(
            h.id,
            vt,
            b"evaluate:receiver:argument:with:",
            observe,
            ptr::from_mut(observer).cast(),
        );
        for (name, left, right, expected) in [
            (b"+".as_slice(), 42i64, 7i64, 49i64),
            (b"-", 42, 7, 35),
            (b"*", 42, 7, 294),
            (b"/", 42, 7, 6),
            (b"mod:", i64::MIN, -1, 0),
        ] {
            let l = object(h.integer(left as usize));
            h.keep(l);
            let r = object(h.integer(right as usize));
            h.keep(r);
            let selector = h.id.intern_name(name).unwrap();
            h.keep(selector);
            let owner = h.vector(&[l, selector, r]);
            let caller = h.method("call", 0, owner);
            for _ in 0..70 {
                let result = h.invoke(caller, integer);
                assert_eq!(h.value(Operand::object(result)) as i64, expected);
            }
        }
        assert!(
            observer.evaluated.get() >= 35,
            "hot numeric calls use the owner-validated Execution evaluator"
        );
        // These entries are already hot. Refused arithmetic must retain the
        // owner's ordinary language errors, including MIN / -1.
        for (name, left, right, kind) in [
            (b"+".as_slice(), i64::MAX, 1i64, b"overflow".as_slice()),
            (b"/", 7, 0, b"zeroDivide"),
            (b"/", i64::MIN, -1, b"overflow"),
        ] {
            let l = object(h.integer(left as usize));
            h.keep(l);
            let r = object(h.integer(right as usize));
            h.keep(r);
            let name = h.id.intern_name(name).unwrap();
            let owner = h.vector(&[l, name, r]);
            let caller = h.method("call", 0, owner);
            let result = h.invoke(caller, integer);
            assert_eq!(
                object(send(h.id, result, b"kind", &[])),
                h.id.intern_name(kind).unwrap()
            );
        }
        let left = object(h.integer(42));
        h.keep(left);
        let right = object(h.integer(7));
        h.keep(right);
        let plus = h.id.intern_name(b"+").unwrap();
        let owner = h.vector(&[left, plus, right]);
        let caller = h.method("call", 0, owner);
        let error_vt = h.named(b"ErrorVt");
        h.keep(error_vt);
        let integer_vt = h.named(b"IntegerVt");
        let error_name = h.id.intern_name(b"ErrorVt").unwrap();
        let before = observer.evaluated.get();
        send(
            h.id,
            h.ns,
            b"at:put:",
            &[Operand::object(error_name), Operand::object(integer_vt)],
        );
        assert_eq!(
            h.invoke(caller, integer),
            right,
            "live ErrorVt controls propagation"
        );
        assert_eq!(observer.evaluated.get(), before);
        send(
            h.id,
            h.ns,
            b"at:put:",
            &[Operand::object(error_name), Operand::object(error_vt)],
        );
        // A replacement with a different native context/function must not be
        // mistaken for Integer arithmetic even after crossing the hot gate.
        let original_plus = send(
            h.id,
            h.id.receiver,
            b"bind:to:",
            &[Operand::object(plus), Operand::object(left)],
        )
        .as_entry()
        .unwrap();
        install(h.id, integer_vt, b"+", replacement, ptr::null_mut());
        let before_calls = observer.calls.get();
        for _ in 0..70 {
            assert_eq!(h.invoke(caller, integer), right);
        }
        assert!(observer.calls.get() > before_calls);
        assert_eq!(observer.evaluated.get(), before);
        send(
            h.id,
            integer_vt,
            b"addMethod",
            &[Operand::object(plus), Operand::entry(original_plus)],
        );
        send(
            h.id,
            vt,
            b"addMethod",
            &[Operand::object(selector), Operand::entry(original)],
        );
    }
}
unsafe extern "C" fn replacement(_: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    Outcome::loaded(unsafe { borrowed_slice(a, n) }.unwrap()[0])
}

struct Proxy<'a> {
    observer: &'a Observer,
    entry: NativeEntry,
}
unsafe extern "C" fn evaluated(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let c = &*c.cast::<Proxy<'_>>();
        c.observer.evaluated.set(c.observer.evaluated.get() + 1);
        c.entry.method.unwrap()(c.entry.context, r, a, n)
    }
}

struct ReplacingFactory {
    id: Protocol,
    gc: Oop,
    baseline: Oop,
    empty: Oop,
    owner: Oop,
    original: NativeEntry,
    calls: Cell<usize>,
}
unsafe extern "C" fn replace_candidate(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    unsafe {
        let c = &*c.cast::<ReplacingFactory>();
        if borrowed_slice(a, n)
            .and_then(|a| a.first())
            .and_then(|v| v.as_word())
            == Some(1)
            && c.calls.get() == 0
        {
            c.calls.set(1);
            send(
                c.id,
                c.baseline,
                b"optimizeWith:",
                &[Operand::object(c.empty)],
            );
            collect(c.id, c.gc);
            assert!(
                present(c.id, c.owner),
                "entered candidate retains its owner after withdrawal"
            );
        }
        c.original.method.unwrap()(c.original.context, r, a, n)
    }
}
unsafe fn active_lifetime(h: &mut H) {
    unsafe {
        let start = h.tokens.len();
        let nil = h.named(b"nil");
        let baseline_owner = allocate(h.id, behavior(h.id), 0);
        h.keep(baseline_owner);
        let baseline = h.method("minimal-return", 1, baseline_owner);
        let owner = allocate(h.id, behavior(h.id), 0);
        h.keep(owner);
        let step = executable(h, owner);
        install_steps(h, baseline, &[step]);
        for _ in 0..64 {
            h.invoke(baseline, nil);
        }
        let empty = h.vector(&[]);
        let base_root = retain(h.id, h.roots, baseline);
        let empty_root = retain(h.id, h.roots, empty);
        for token in h.tokens.drain(start..) {
            release(h.id, h.roots, token);
        }
        collect(h.id, h.gc);
        assert!(present(h.id, owner));
        let integer = h.named(b"Integer");
        let vt = object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(integer)],
        ));
        let selector = h.id.intern_name(b"fromWord:").unwrap();
        let original = send(
            h.id,
            h.id.receiver,
            b"bind:to:",
            &[Operand::object(selector), Operand::object(integer)],
        )
        .as_entry()
        .unwrap();
        let context = Box::leak(Box::new(ReplacingFactory {
            id: h.id,
            gc: h.gc,
            baseline,
            empty,
            owner,
            original,
            calls: Cell::new(0),
        }));
        install(
            h.id,
            vt,
            b"fromWord:",
            replace_candidate,
            ptr::from_mut(context).cast(),
        );
        assert_eq!(h.invoke(baseline, nil), owner);
        assert_eq!(context.calls.get(), 1);
        send(
            h.id,
            vt,
            b"addMethod",
            &[Operand::object(selector), Operand::entry(original)],
        );
        collect(h.id, h.gc);
        assert!(!present(h.id, owner));
        assert!(!present(h.id, step));
        assert_eq!(h.invoke(baseline, nil), baseline_owner);
        release(h.id, h.roots, base_root);
        release(h.id, h.roots, empty_root);
        collect(h.id, h.gc);
    }
}

// A language Method may die before Execution's next safe checkpoint. Its old
// native thunk key must not collide with another freshly registered Method.
unsafe fn retirement_churn(h: &mut H) {
    unsafe {
        for _ in 0..96 {
            let start = h.tokens.len();
            let owner = allocate(h.id, behavior(h.id), 0);
            h.keep(owner);
            let method = h.method("minimal-return", 1, owner);
            assert_eq!(h.invoke(method, h.named(b"nil")), owner);
            for token in h.tokens.drain(start..) {
                release(h.id, h.roots, token);
            }
            collect(h.id, h.gc);
            assert!(!present(h.id, owner));
            assert!(!present(h.id, method));
        }
        send(h.id, h.named(b"Execution"), b"checkpoint", &[]);
    }
}
