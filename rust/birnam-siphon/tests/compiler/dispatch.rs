//! Generated source keeps owner dispatch and roots without speculative PC entries.
use super::*;

unsafe fn lookup(h: &H, receiver: Oop, name: &[u8]) -> NativeEntry {
    unsafe {
        send(
            h.id,
            h.id.receiver,
            b"bind:to:",
            &[
                Operand::object(h.id.intern_name(name).unwrap()),
                Operand::object(receiver),
            ],
        )
        .as_entry()
        .unwrap()
    }
}
unsafe fn restore(h: &H, behavior: Oop, name: &[u8], entry: NativeEntry) {
    unsafe { install(h.id, behavior, name, entry.method.unwrap(), entry.context) }
}
unsafe fn run(h: &mut H, block: Oop) -> Oop {
    unsafe {
        let machine = object(send(h.id, h.named(b"Machine"), b"new", &[]));
        let token = retain(h.id, h.roots, machine);
        let result = object(send(h.id, machine, b"runBlock:", &[Operand::object(block)]));
        h.keep(result);
        release(h.id, h.roots, token);
        result
    }
}

struct Forward {
    id: Protocol,
    gc: Oop,
    entry: NativeEntry,
    calls: Cell<usize>,
    collecting: bool,
}
unsafe extern "C" fn forward(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Forward>();
        p.calls.set(p.calls.get() + 1);
        if p.collecting {
            collect(p.id, p.gc);
        }
        p.entry.method.unwrap()(p.entry.context, r, a, n)
    }
}
// Ordinary compilation must not construct speculative native candidates.
unsafe extern "C" fn reject_candidate(
    c: *mut c_void,
    _: Oop,
    _: *const Operand,
    _: usize,
) -> Outcome {
    unsafe {
        *c.cast::<usize>() += 1;
    }
    Outcome::failed(birnam_abi::id_protocol::INVALID_REQUEST, 91)
}
unsafe extern "C" fn right_argument(
    _: *mut c_void,
    _: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    unsafe { Outcome::loaded(borrowed_slice(a, n).unwrap()[0]) }
}

pub(super) unsafe fn check(h: &mut H) {
    unsafe {
        h.clear();
        h.collect();
        assert_eq!(h.units(), 0);
        let method = h.named(b"Method");
        let method_vt = object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(method)],
        ));
        let constructor = lookup(h, method, b"fromNative:owner:");
        let mut candidate_calls = Box::new(0usize);
        install(
            h.id,
            method_vt,
            b"fromNative:owner:",
            reject_candidate,
            ptr::from_mut(&mut *candidate_calls).cast(),
        );
        let origin = h.compile(b"let compiledAnswer := [20 + 22]; compiledAnswer");
        let block = h.block(origin);
        // Both cold and warm runs update an existing binding. Authored Globals
        // deliberately takes a different path when inserting a new name.
        h.publish(b"compiledAnswer", h.named(b"nil"));
        let integer = h.named(b"Integer");
        let integer_vt = object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(integer)],
        ));
        let mut factory = Box::new(Forward {
            id: h.id,
            gc: h.named(b"GC"),
            entry: lookup(h, integer, b"fromWord:"),
            calls: Cell::new(0),
            collecting: false,
        });
        install(
            h.id,
            integer_vt,
            b"fromWord:",
            forward,
            ptr::from_mut(&mut *factory).cast(),
        );
        let result = run(h, block);
        assert_eq!(h.integer(result), 42);
        // This replacement has a visible call-count effect. Elision must not
        // bypass it as though it were the ordinary pure constructor. This is
        // dispatch/effect coverage, not a requirement to match physical heaps.
        let cold_factory_calls = factory.calls.get();
        for _ in 0..40 {
            let result = run(h, block);
            assert_eq!(h.integer(result), 42);
        }
        factory.calls.set(0);
        let result = run(h, block);
        assert_eq!(h.integer(result), 42);
        assert_eq!(
            factory.calls.get(),
            cold_factory_calls,
            "execution preserves the replaced factory's call effects"
        );
        restore(h, integer_vt, b"fromWord:", factory.entry);

        // Repeated execution cannot freeze the numeric operation selected by
        // an ordinary source send. The changed implementation supplies 22.
        let value = object(send(h.id, integer, b"fromWord:", &[Operand::word(20)]));
        h.keep(value);
        let value_vt = object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(value)],
        ));
        let plus = lookup(h, value, b"+");
        install(h.id, value_vt, b"+", right_argument, ptr::null_mut());
        let result = run(h, block);
        assert_eq!(h.integer(result), 22);
        restore(h, value_vt, b"+", plus);

        // A current conversion still runs inside the compiled entry. Collecting
        // here exercises the generated Call's roots, including its live PC token.
        let mut converter = Box::new(Forward {
            id: h.id,
            gc: h.named(b"GC"),
            entry: lookup(h, integer, b"wordOf:"),
            calls: Cell::new(0),
            collecting: true,
        });
        install(
            h.id,
            integer_vt,
            b"wordOf:",
            forward,
            ptr::from_mut(&mut *converter).cast(),
        );
        let result = run(h, block);
        assert_eq!(h.integer(result), 42);
        assert!(
            converter.calls.get() > 0,
            "compiled conversion is late-bound"
        );
        restore(h, integer_vt, b"wordOf:", converter.entry);

        // Machine operations remain live sends, including collecting replacements.
        let machine = h.named(b"Machine");
        let machine_vt = object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(machine)],
        ));
        let mut reader = Box::new(Forward {
            id: h.id,
            gc: h.named(b"GC"),
            entry: lookup(h, machine, b"oopAt:"),
            calls: Cell::new(0),
            collecting: true,
        });
        install(
            h.id,
            machine_vt,
            b"oopAt:",
            forward,
            ptr::from_mut(&mut *reader).cast(),
        );
        let result = run(h, block);
        assert_eq!(h.integer(result), 42);
        assert!(reader.calls.get() > 0);
        restore(h, machine_vt, b"oopAt:", reader.entry);
        let result = run(h, block);
        assert_eq!(h.integer(result), 42);
        h.clear();
        h.collect();
        assert_eq!(h.units(), 0);
        assert_eq!(h.native_bytes(), 0);

        // Larger functions use the same ordinary production path.
        let source = format!("{}42", "[1 + 2]; ".repeat(64));
        let origin = h.compile(source.as_bytes());
        let block = h.block(origin);
        let result = run(h, block);
        assert_eq!(h.integer(result), 42);
        h.clear();
        h.collect();
        assert_eq!(h.units(), 0);
        assert_eq!(h.native_bytes(), 0);

        // Nested Blocks retain the actual captured values and their shared
        // Unit, then release the entire cycle after the last Block disappears.
        let block =
            h.eval(b"let candidateCapture := 35; [ | let sum := [candidateCapture + 7]; sum]");
        for _ in 0..40 {
            let result = run(h, block);
            assert_eq!(h.integer(result), 42);
        }
        h.clear();
        h.collect();
        assert_eq!(h.units(), 0);
        assert_eq!(h.native_bytes(), 0);

        assert_eq!(
            *candidate_calls, 0,
            "source compilation needs no speculative candidates"
        );
        restore(h, method_vt, b"fromNative:owner:", constructor);
    }
}
