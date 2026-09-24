//! Observe the actual Codegen/TCC entries through ordinary replaceable Methods.
//! The probe wraps entries at construction; it does not synthesize candidates.
use super::*;
use std::cell::RefCell;

struct Probe {
    id: Protocol,
    gc: Oop,
    tcc: Oop,
    constructor: NativeEntry,
    // Stable native callback contexts, owned by this test until all Methods die.
    entries: RefCell<Vec<Box<Observed>>>,
    calls: Cell<usize>,
    collect_construction: Cell<bool>,
    reject_construction: Cell<bool>,
    withdraw: Cell<bool>,
    entered: Cell<Oop>,
    origin: Cell<Oop>,
    empty: Oop,
}
struct Observed {
    probe: *const Probe,
    entry: NativeEntry,
    method: Cell<Oop>,
}
unsafe extern "C" fn construct(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Probe>();
        if p.reject_construction.get() {
            return Outcome::failed(birnam_abi::id_protocol::INVALID_REQUEST, 91);
        }
        let [entry, owner] = borrowed_slice(a, n).unwrap() else {
            panic!("candidate constructor")
        };
        if p.collect_construction.get() {
            collect(p.id, p.gc);
        }
        let observed = Box::new(Observed {
            probe: p,
            entry: entry.as_entry().unwrap(),
            method: Cell::new(ptr::null_mut()),
        });
        let context = ptr::from_ref(&*observed).cast_mut().cast();
        let args = [Operand::entry(NativeEntry::new(invoke, context)), *owner];
        let result = p.constructor.method.unwrap()(p.constructor.context, r, args.as_ptr(), 2);
        if let Some(value) = result.value.as_object() {
            observed.method.set(value);
        }
        p.entries.borrow_mut().push(observed);
        result
    }
}
unsafe extern "C" fn invoke(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let observed = &*c.cast::<Observed>();
        let p = &*observed.probe;
        p.calls.set(p.calls.get() + 1);
        let previous = p.entered.replace(observed.method.get());
        let result = observed.entry.method.unwrap()(observed.entry.context, r, a, n);
        p.entered.set(previous);
        result
    }
}
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
    probe: *const Probe,
}
unsafe extern "C" fn forward(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Forward>();
        p.calls.set(p.calls.get() + 1);
        if !p.probe.is_null() {
            let probe = &*p.probe;
            if !probe.entered.get().is_null() && probe.withdraw.replace(false) {
                // This owner callback runs inside the generated C entry. Its
                // replacement/removal cannot release code still on that stack.
                send(
                    probe.id,
                    probe.origin.get(),
                    b"optimizeWith:",
                    &[Operand::object(probe.empty)],
                );
                collect(probe.id, probe.gc);
                send(probe.id, probe.tcc, b"checkpoint", &[]);
                assert!(
                    present(probe.id, probe.entered.get()),
                    "entered candidate remains rooted after withdrawal"
                );
                assert!(
                    send(probe.id, probe.tcc, b"liveNativeBytes", &[])
                        .as_word()
                        .unwrap()
                        > 0
                );
            }
        }
        if p.collecting {
            collect(p.id, p.gc);
        }
        p.entry.method.unwrap()(p.entry.context, r, a, n)
    }
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
        let empty = object(send(h.id, h.named(b"Vector"), b"new", &[]));
        let empty_pin = retain(h.id, h.roots, empty);
        let mut probe = Box::new(Probe {
            id: h.id,
            gc: h.named(b"GC"),
            tcc: h.named(b"TCC"),
            constructor: lookup(h, method, b"fromNative:owner:"),
            entries: RefCell::new(vec![]),
            calls: Cell::new(0),
            collect_construction: Cell::new(true),
            reject_construction: Cell::new(false),
            withdraw: Cell::new(false),
            entered: Cell::new(ptr::null_mut()),
            origin: Cell::new(ptr::null_mut()),
            empty,
        });
        install(
            h.id,
            method_vt,
            b"fromNative:owner:",
            construct,
            ptr::from_mut(&mut *probe).cast(),
        );
        let origin = h.compile(b"let candidateAnswer := [20 + 22]; candidateAnswer");
        probe.origin.set(origin);
        assert_eq!(
            probe.entries.borrow().len(),
            2,
            "one real compiled entry per PC"
        );
        let block = h.block(origin);
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
            probe: ptr::null(),
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
        assert_eq!(probe.calls.get(), 0, "cold execution uses the baseline");
        // This replacement has a visible call-count effect. Elision must not
        // bypass it as though it were the ordinary pure constructor. This is
        // dispatch/effect coverage, not a requirement to match physical heaps.
        let cold_factory_calls = factory.calls.get();
        for _ in 0..40 {
            let result = run(h, block);
            assert_eq!(h.integer(result), 42);
        }
        assert!(
            probe.calls.get() > 0,
            "hot execution enters generated candidates"
        );
        factory.calls.set(0);
        let result = run(h, block);
        assert_eq!(h.integer(result), 42);
        assert_eq!(
            factory.calls.get(),
            cold_factory_calls,
            "specialization preserves the replaced factory's call effects"
        );
        restore(h, integer_vt, b"fromWord:", factory.entry);

        // Hot specialization cannot freeze the numeric operation selected by
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
        let before = probe.calls.get();
        let result = run(h, block);
        assert_eq!(h.integer(result), 22);
        assert!(probe.calls.get() > before);
        restore(h, value_vt, b"+", plus);

        // A current conversion still runs inside the real candidate. Collecting
        // here exercises the generated Call's roots, including its live PC token.
        let mut converter = Box::new(Forward {
            id: h.id,
            gc: h.named(b"GC"),
            entry: lookup(h, integer, b"wordOf:"),
            calls: Cell::new(0),
            collecting: true,
            probe: ptr::null(),
        });
        install(
            h.id,
            integer_vt,
            b"wordOf:",
            forward,
            ptr::from_mut(&mut *converter).cast(),
        );
        let before = probe.calls.get();
        let result = run(h, block);
        assert_eq!(h.integer(result), 42);
        assert!(probe.calls.get() > before);
        assert!(
            converter.calls.get() > 0,
            "candidate conversion is late-bound"
        );
        restore(h, integer_vt, b"wordOf:", converter.entry);

        // Guard withdrawal preserves an observer installed on Machine. Restoring
        // the binding does not silently reattach the withdrawn candidate.
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
            probe: ptr::null(),
        });
        install(
            h.id,
            machine_vt,
            b"oopAt:",
            forward,
            ptr::from_mut(&mut *reader).cast(),
        );
        let before = probe.calls.get();
        let result = run(h, block);
        assert_eq!(h.integer(result), 42);
        assert!(reader.calls.get() > 0);
        assert_eq!(probe.calls.get(), before);
        restore(h, machine_vt, b"oopAt:", reader.entry);
        let result = run(h, block);
        assert_eq!(h.integer(result), 42);
        assert_eq!(probe.calls.get(), before);
        h.clear();
        h.collect();
        assert_eq!(h.units(), 0);
        assert_eq!(h.native_bytes(), 0);

        // An explicit removal during an entered generated candidate must not
        // retire that callable or unmap its actual TCC code before return.
        probe.collect_construction.set(false);
        probe.entries.borrow_mut().clear();
        let origin = h.compile(b"let candidateAnswer := [35 + 7]; candidateAnswer");
        probe.origin.set(origin);
        let block = h.block(origin);
        for _ in 0..40 {
            let result = run(h, block);
            assert_eq!(h.integer(result), 42);
        }
        let before = probe.calls.get();
        converter.probe = ptr::from_ref(&*probe);
        install(
            h.id,
            integer_vt,
            b"wordOf:",
            forward,
            ptr::from_mut(&mut *converter).cast(),
        );
        probe.withdraw.set(true);
        let result = run(h, block);
        assert_eq!(h.integer(result), 42);
        assert!(!probe.withdraw.get());
        assert_eq!(probe.calls.get(), before + 1);
        restore(h, integer_vt, b"wordOf:", converter.entry);
        h.collect();
        for entry in probe.entries.borrow().iter() {
            assert!(
                !present(h.id, entry.method.get()),
                "withdrawn candidate dies after return"
            );
        }
        let result = run(h, block);
        assert_eq!(h.integer(result), 42);
        assert_eq!(probe.calls.get(), before + 1);
        h.clear();
        h.collect();
        assert_eq!(h.units(), 0);
        assert_eq!(h.native_bytes(), 0);

        // Source outside the bounded producer still compiles and runs baseline.
        probe.entries.borrow_mut().clear();
        let source = format!("{}42", "[1 + 2]; ".repeat(64));
        let origin = h.compile(source.as_bytes());
        let block = h.block(origin);
        assert!(probe.entries.borrow().is_empty());
        let result = run(h, block);
        assert_eq!(h.integer(result), 42);
        h.clear();
        h.collect();
        assert_eq!(h.units(), 0);
        assert_eq!(h.native_bytes(), 0);

        // Nested candidates retain the actual captured values and their shared
        // Unit, then release the entire cycle after the last Block disappears.
        probe.entries.borrow_mut().clear();
        let block =
            h.eval(b"let candidateCapture := 35; [ | let sum := [candidateCapture + 7]; sum]");
        let before = probe.calls.get();
        for _ in 0..40 {
            let result = run(h, block);
            assert_eq!(h.integer(result), 42);
        }
        assert!(probe.calls.get() > before);
        h.clear();
        h.collect();
        assert_eq!(h.units(), 0);
        assert_eq!(h.native_bytes(), 0);

        // Failed candidate installation leaves the partially initialized Unit
        // and all constructed baseline Methods reclaimable.
        probe.reject_construction.set(true);
        let source = h.string(b"let candidateAnswer := [1 + 2]; candidateAnswer");
        let failure =
            h.id.send_named(h.named(b"Codegen"), b"compile:", &[Operand::object(source)])
                .unwrap_err();
        assert_eq!(failure.detail, 91);
        probe.reject_construction.set(false);
        h.clear();
        h.collect();
        assert_eq!(h.units(), 0);
        assert_eq!(h.native_bytes(), 0);
        restore(h, method_vt, b"fromNative:owner:", probe.constructor);
        release(h.id, h.roots, empty_pin);
    }
}
