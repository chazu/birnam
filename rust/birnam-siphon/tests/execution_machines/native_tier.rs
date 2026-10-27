//! Pending native numeric consumer acceptance. PC candidates use Oops in candidates.rs.
use super::*;
struct Observer {
    id: Protocol,
    gc: Oop,
    entry: NativeEntry,
    calls: Cell<usize>,
    evaluated: Cell<usize>,
    execution: Oop,
}
unsafe extern "C" fn observe(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let c = &*c.cast::<Observer>();
        c.calls.set(c.calls.get() + 1);
        collect(c.id, c.gc);
        let args = borrowed_slice(a, n).unwrap();
        assert_eq!(args[3].as_object(), Some(c.execution));
        c.entry.method.unwrap()(c.entry.context, r, args.as_ptr(), args.len())
    }
}
pub(super) unsafe fn numeric(h: &mut H) {
    unsafe {
        let native_seed = std::env::var_os("BIRNAM_EXECUTION_NATIVE_SEED").is_some();
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
            execution: h.named(b"Execution"),
        }));
        install(
            h.id,
            vt,
            b"evaluate:receiver:argument:with:",
            observe,
            ptr::from_mut(observer).cast(),
        );
        let execution_vt = object(send(h.id, h.id.receiver, b"behaviorOf:", &[Operand::object(observer.execution)]));
        let evaluation_selector = h.id.intern_name(b"numericOperation:left:right:").unwrap();
        let evaluator = send(h.id, execution_vt, b"lookup", &[Operand::object(evaluation_selector)]).as_entry().unwrap();
        install(h.id, execution_vt, b"savedNumericOperation:left:right:", evaluator.method.unwrap(), evaluator.context);
        install(h.id, execution_vt, b"numericOperation:left:right:", evaluated, ptr::from_mut(observer).cast());
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
        assert!(observer.calls.get() >= 35, "hot calls query the current Integer owner");
        if native_seed {
            assert!(observer.evaluated.get() >= 35,
                "bootstrap arithmetic uses the owner-validated Execution evaluator");
        } else {
            assert_eq!(observer.evaluated.get(), 0,
                "the native Integer evaluator must decline authored arithmetic entries");
        }
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
        // Keep the authored Method alive while its dictionary binding is absent.
        // Restoring a borrowed NativeEntry after collection would be a dangling
        // executable context; the native bootstrap entry has no Method wrapper.
        if let Some(method) = send(h.id, observer.execution, b"methodOfEntry:",
            &[Operand::entry(original_plus)]).as_object() {
            h.keep(method);
        }
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
        let query = [Operand::entry(original_plus), Operand::object(left), Operand::object(right), Operand::object(observer.execution)];
        install(h.id, execution_vt, b"numericOperation:left:right:", decline_numeric, ptr::null_mut());
        let answer = send(h.id, integer, b"evaluate:receiver:argument:with:", &query);
        if native_seed {
            assert_eq!(h.value(answer), h.value(Operand::object(left)) + h.value(Operand::object(right)));
        } else {
            assert_eq!(answer.kind(), birnam_abi::id_protocol::UNIT);
        }
        install(h.id, execution_vt, b"numericOperation:left:right:", fail_numeric, ptr::null_mut());
        if native_seed {
            assert_eq!(h.id.send_named(integer, b"evaluate:receiver:argument:with:", &query).unwrap_err().detail, 917);
        } else {
            assert_eq!(send(h.id, integer, b"evaluate:receiver:argument:with:", &query).kind(),
                birnam_abi::id_protocol::UNIT,
                "declining an authored entry must not invoke the native evaluator");
        }
        let mut old_visitor = query;
        old_visitor[3] = Operand::entry(evaluator);
        if native_seed {
            assert!(h.id.send_named(integer, b"evaluate:receiver:argument:with:", &old_visitor).is_err());
        }
        install(h.id, execution_vt, b"numericOperation:left:right:", evaluator.method.unwrap(), evaluator.context);
        send(h.id, execution_vt, b"removeMethod:", &[Operand::object(h.id.intern_name(b"savedNumericOperation:left:right:").unwrap())]);

    }
}
unsafe extern "C" fn replacement(_: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    Outcome::loaded(unsafe { borrowed_slice(a, n) }.unwrap()[0])
}

unsafe extern "C" fn decline_numeric(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::unit())
}
unsafe extern "C" fn fail_numeric(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(birnam_abi::id_protocol::INVALID_REQUEST, 917)
}
unsafe extern "C" fn evaluated(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let c = &*c.cast::<Observer>();
        assert_eq!(r, c.execution);
        c.evaluated.set(c.evaluated.get() + 1);
        collect(c.id, c.gc);
        c.id.send_named(r, b"savedNumericOperation:left:right:", borrowed_slice(a, n).unwrap()).map_or_else(|e| e, Outcome::loaded)
    }
}
