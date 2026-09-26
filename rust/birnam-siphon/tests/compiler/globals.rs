//! Generated global accesses use the existing Globals/Store language protocol.
use super::*;
use birnam_abi::id_protocol as native;

unsafe fn key(h: &H, name: &[u8]) -> Oop {
    unsafe { h.id.intern_name(name).unwrap() }
}
unsafe fn read(h: &H, name: &[u8]) -> Oop {
    unsafe {
        object(send(
            h.id,
            h.named(b"Globals"),
            b"at:",
            &[Operand::object(key(h, name))],
        ))
    }
}
unsafe fn number(h: &mut H, value: usize) -> Oop {
    let o = unsafe {
        object(send(
            h.id,
            h.named(b"Integer"),
            b"fromWord:",
            &[Operand::word(value)],
        ))
    };
    unsafe { h.keep(o) }
}
unsafe fn run(h: &mut H, block: Oop) -> Oop {
    let o = unsafe { object(send(h.id, block, b"value", &[])) };
    unsafe { h.keep(o) }
}
struct Replacement {
    id: Protocol,
    gc: Oop,
    alias: Oop,
    target: Oop,
    value: Oop,
    calls: Cell<usize>,
    failure: bool,
}
unsafe extern "C" fn replacing(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Replacement>();
        let args = borrowed_slice(a, n).unwrap();
        collect(p.id, p.gc);
        if args.first().and_then(|a| a.as_object()) == Some(p.target) {
            p.calls.set(p.calls.get() + 1);
            if p.failure {
                return Outcome::failed(native::INVALID_REQUEST, 917);
            }
            return Outcome::loaded(Operand::object(p.value));
        }
        p.id.send(r, p.alias, args)
            .map_or_else(|e| e, Outcome::loaded)
    }
}
unsafe fn save(h: &H, selector: &[u8], alias: &[u8]) -> NativeEntry {
    unsafe {
        let vt = h.named(b"GlobalsVt");
        let original = send(
            h.id,
            h.id.receiver,
            b"bindingOf:in:",
            &[Operand::object(key(h, selector)), Operand::object(vt)],
        )
        .as_entry()
        .unwrap();
        send(
            h.id,
            vt,
            b"addMethod",
            &[Operand::object(key(h, alias)), Operand::entry(original)],
        );
        original
    }
}
unsafe fn restore(h: &H, selector: &[u8], alias: &[u8], original: NativeEntry) {
    unsafe {
        let vt = h.named(b"GlobalsVt");
        send(
            h.id,
            vt,
            b"addMethod",
            &[Operand::object(key(h, selector)), Operand::entry(original)],
        );
        send(
            h.id,
            vt,
            b"removeMethod:",
            &[Operand::object(key(h, alias))],
        );
    }
}
pub(super) unsafe fn check(h: &mut H) {
    unsafe {
        let globals = h.named(b"Globals");
        let store = h.named(b"Store");
        let nil = h.named(b"nil");
        let v = h.eval(b"let sourceGlobal := 41; sourceGlobal");
        assert_eq!(h.integer(v), 41);
        assert_eq!(read(h, b"sourceGlobal"), v);
        assert_eq!(
            send(
                h.id,
                h.ns,
                b"at:",
                &[Operand::object(key(h, b"sourceGlobal"))]
            )
            .kind(),
            native::UNIT
        );
        let method = h.compile(b"sourceGlobal");
        let reader = h.block(method);
        let v = run(h, reader);
        assert_eq!(h.integer(v), 41);
        let value = number(h, 42);
        h.publish(b"sourceGlobal", value);
        let v = run(h, reader);
        assert_eq!(h.integer(v), 42);
        // Preserve Globals-first lookup and the historical nil-to-Namespace fallback.
        let fallback = number(h, 99);
        h.bind(b"sourceGlobal", fallback);
        assert_eq!(run(h, reader), value);
        h.publish(b"sourceGlobal", nil);
        assert_eq!(run(h, reader), fallback);
        assert_eq!(
            object(send(
                h.id,
                globals,
                b"includes:",
                &[Operand::object(key(h, b"sourceGlobal"))]
            )),
            h.named(b"true")
        );
        assert_eq!(h.eval(b"missingGlobalForCompilerTest"), nil);
        assert_eq!(h.eval(b"let sourceNil := nil; sourceNil"), nil);
        assert_eq!(
            object(send(
                h.id,
                globals,
                b"includes:",
                &[Operand::object(key(h, b"sourceNil"))]
            )),
            h.named(b"true")
        );
        // Inner lets remain lexical; neither argument nor local leaks to Globals.
        let v=h.eval(b"let lexicalGlobal := 10; [[:arg | let lexicalGlobal := arg; let onlyLocal := 7; [lexicalGlobal + onlyLocal]] value: 35]");
        assert_eq!(h.integer(v), 42);
        assert_eq!(h.integer(read(h, b"lexicalGlobal")), 10);
        for name in [b"onlyLocal".as_slice(), b"arg"] {
            assert_eq!(
                object(send(
                    h.id,
                    globals,
                    b"includes:",
                    &[Operand::object(key(h, name))]
                )),
                h.named(b"false")
            );
        }
        // Source writes are real historied Store facts, visible to normal queries.
        send(
            h.id,
            store,
            b"assert:attr:value:",
            &[
                Operand::object(key(h, b"historiedSourceGlobal")),
                Operand::object(key(h, b"history")),
                Operand::object(h.named(b"true")),
            ],
        );
        h.eval(b"let historiedSourceGlobal := 11");
        let basis = object(send(h.id, store, b"basis", &[]));
        h.keep(basis);
        h.eval(b"let historiedSourceGlobal := 22");
        let old = object(send(
            h.id,
            store,
            b"valueOf:attr:asOf:",
            &[
                Operand::object(key(h, b"__globals__")),
                Operand::object(key(h, b"historiedSourceGlobal")),
                Operand::object(basis),
            ],
        ));
        assert_eq!(h.integer(old), 11);
        assert_eq!(h.integer(read(h, b"historiedSourceGlobal")), 22);
        let v = h.eval(
            b"[[db findAll: ?v where: #[ #[#__globals__ #historiedSourceGlobal ?v] ]] at: 0]",
        );
        assert_eq!(h.integer(v), 22);
        // A previously compiled reader observes a replacement getter and survives GC.
        let original = save(h, b"at:", b"savedCompilerGlobalAt:");
        let mut replacement = Replacement {
            id: h.id,
            gc: h.named(b"GC"),
            alias: key(h, b"savedCompilerGlobalAt:"),
            target: key(h, b"sourceGlobal"),
            value,
            calls: Cell::new(0),
            failure: false,
        };
        install(
            h.id,
            h.named(b"GlobalsVt"),
            b"at:",
            replacing,
            ptr::from_mut(&mut replacement).cast(),
        );
        assert_eq!(run(h, reader), value);
        assert_eq!(replacement.calls.get(), 1);
        replacement.failure = true;
        let failure = h.id.send_named(reader, b"value", &[]).unwrap_err();
        assert_eq!(failure.detail, 917);
        restore(h, b"at:", b"savedCompilerGlobalAt:", original);
        // A refused binding returns its exact result and skips subsequent effects.
        let text = h.string(b"refused global write");
        let error = object(send(
            h.id,
            h.named(b"Error"),
            b"kind:message:",
            &[
                Operand::object(key(h, b"historyResourceLimit")),
                Operand::object(text),
            ],
        ));
        h.keep(error);
        let setter = save(h, b"at:put:", b"savedCompilerGlobalAt:put:");
        replacement.alias = h.keep(key(h, b"savedCompilerGlobalAt:put:"));
        replacement.target = h.keep(key(h, b"refusedGlobal"));
        replacement.value = error;
        replacement.failure = false;
        replacement.calls.set(0);
        install(
            h.id,
            h.named(b"GlobalsVt"),
            b"at:put:",
            replacing,
            ptr::from_mut(&mut replacement).cast(),
        );
        let method=h.compile(b"let precedingGlobal := 7; let refusedGlobal := [Object clone]; [Globals at: #afterRefusedGlobal put: true]; 99");
        let writer = h.block(method);
        assert_eq!(run(h, writer), error);
        assert_eq!(replacement.calls.get(), 1);
        assert_eq!(h.integer(read(h, b"precedingGlobal")), 7);
        assert_eq!(read(h, b"refusedGlobal"), nil);
        assert_eq!(read(h, b"afterRefusedGlobal"), nil);
        replacement.value = nil;
        assert_eq!(run(h, writer), nil);
        assert_eq!(read(h, b"afterRefusedGlobal"), nil);
        replacement.failure = true;
        let failure = h.id.send_named(writer, b"value", &[]).unwrap_err();
        assert_eq!(failure.detail, 917);
        restore(h, b"at:put:", b"savedCompilerGlobalAt:put:", setter);
        // A successful collecting setter publishes the exact value, including Blocks.
        replacement.target = h.keep(key(h, b"unusedSetterTarget"));
        replacement.failure = false;
        let setter = save(h, b"at:put:", b"savedCompilerGlobalAt:put:");
        install(
            h.id,
            h.named(b"GlobalsVt"),
            b"at:put:",
            replacing,
            ptr::from_mut(&mut replacement).cast(),
        );
        h.eval(b"let retainedGlobalBlock := [| 73]");
        restore(h, b"at:put:", b"savedCompilerGlobalAt:put:", setter);
        h.clear();
        h.collect();
        assert!(h.units() > 0);
        let v = h.eval(b"[retainedGlobalBlock value]");
        assert_eq!(h.integer(v), 73);
        h.publish(b"retainedGlobalBlock", nil);
        send(h.id, store, b"compact", &[]);
        h.clear();
        h.collect();
        assert_eq!(h.units(), 0);
        assert_eq!(h.native_bytes(), 0);
    }
}
