//! Explicit incoming carriers, rooting and primitive-only initialization.
mod support;
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
};
use std::os::unix::ffi::OsStrExt;
use support::*;
struct Early {
    id: Protocol,
    load: NativeEntry,
    seen: Cell<usize>,
}
unsafe extern "C" fn early_load(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let s = unsafe { &*c.cast::<Early>() };
    let result = (|| unsafe {
        let args = borrowed_slice(a, n).unwrap();
        let raw = args[0].as_bytes().unwrap();
        let path = borrowed_bytes(raw.pointer, raw.length).unwrap();
        if path.ends_with(b"libbirnam_execution.dylib")
            || path.ends_with(b"libbirnam_portable_code.dylib")
        {
            let ns = args[1].as_object().unwrap();
            for name in [
                b"Integer".as_slice(),
                b"String",
                b"Float",
                b"Vector",
                b"Map",
            ] {
                assert_eq!(
                    s.id.send_named(ns, b"at:", &[Operand::object(s.id.intern_name(name)?)])?
                        .kind(),
                    native::UNIT
                );
            }
            s.seen.set(s.seen.get() + 1);
        }
        (s.load.method.unwrap())(s.load.context, r, a, n).into_result()
    })();
    result.map_or_else(|e| e, Outcome::loaded)
}
struct Mutate {
    id: Protocol,
    entry: NativeEntry,
    gc: Oop,
    input: RefCell<Vec<u8>>,
    calls: Cell<usize>,
}
unsafe extern "C" fn mutate_word_factory(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    let s = unsafe { &*c.cast::<Mutate>() };
    s.calls.set(s.calls.get() + 1);
    s.input.borrow_mut().fill(b'z');
    if let Err(e) = unsafe { s.id.send_named(s.gc, b"collectPrecisely", &[]) } {
        return e;
    }
    unsafe { (s.entry.method.unwrap())(s.entry.context, r, a, n) }
}
#[test]
#[ignore = "production native modules required"]
fn incoming_signatures_preserve_carriers_and_early_bootstrap() {
    unsafe {
        let id = id();
        let load_entry = send(id, id.receiver, b"lookup", &[Operand::object(id.load)])
            .as_entry()
            .unwrap();
        let early = Box::leak(Box::new(Early {
            id,
            load: load_entry,
            seen: Cell::new(0),
        }));
        send(
            id,
            id.receiver,
            b"addMethod",
            &[
                Operand::object(id.load),
                Operand::entry(NativeEntry::new(
                    early_load,
                    core::ptr::from_mut(early).cast(),
                )),
            ],
        );
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        send(
            id,
            id.receiver,
            b"addMethod",
            &[Operand::object(id.load), Operand::entry(load_entry)],
        );
        assert_eq!(early.seen.get(), 2);
        let named = |name: &[u8]| {
            object(send(
                id,
                ns,
                b"at:",
                &[Operand::object(id.intern_name(name).unwrap())],
            ))
        };
        let (string, roots, gc, word) = (
            named(b"String"),
            named(b"Roots"),
            named(b"GC"),
            named(b"Word"),
        );
        let code=br#"
          let IncomingProbe := [Object clone];
          let echo := [Execution block: [:self :bits :data | [System gc]; data] withNativeSignature: "bwb"];
          [Reflect defineMethod: #echoWord:bytes: on: IncomingProbe as: echo];
          let wordEcho := [Execution block: [:self :bits | bits] withNativeSignature: "ww"];
          [Reflect defineMethod: #word: on: IncomingProbe as: wordEcho];
          let optional := [Execution block: [:self :value | nil] withNativeSignature: "Wo"];
          [Reflect defineMethod: #optional: on: IncomingProbe as: optional];
          [Globals at: #IncomingProbe put: IncomingProbe];
          let code := [wordEcho codeObject];
          let signature := [code nativeSignature];
          [Execution sendNativeTo: signature selector: #appendBytes: arguments: #["ignored"] signature: "ob" resultFactory: Word];
          [Globals at: #IncomingCode put: code];
          IncomingProbe
        "#;
        let source = object(send(id, string, b"fromBytes:", &[Operand::bytes(code)]));
        let source_token = retain(id, roots, source);
        let probe = object(send(
            id,
            named(b"Codegen"),
            b"eval:",
            &[Operand::object(source)],
        ));
        let probe_token = retain(id, roots, probe);
        release(id, roots, source_token);
        assert_eq!(object(send(id, probe, b"isError", &[])), named(b"false"));
        let wb = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(word)],
        ));
        let original = send(
            id,
            wb,
            b"lookup",
            &[Operand::object(id.intern_name(b"fromBits:").unwrap())],
        )
        .as_entry()
        .unwrap();
        let mutate = Box::leak(Box::new(Mutate {
            id,
            entry: original,
            gc,
            input: RefCell::new(b"original".to_vec()),
            calls: Cell::new(0),
        }));
        install(
            id,
            wb,
            b"fromBits:",
            mutate_word_factory,
            core::ptr::from_mut(mutate).cast(),
        );
        let raw = {
            let data = mutate.input.borrow();
            Operand::bytes(&data)
        };
        let result = send(
            id,
            probe,
            b"echoWord:bytes:",
            &[Operand::word(usize::MAX), raw],
        );
        let b = result.as_bytes().unwrap();
        assert_eq!(borrowed_bytes(b.pointer, b.length).unwrap(), b"original");
        assert_eq!(&*mutate.input.borrow(), b"zzzzzzzz");
        assert!(mutate.calls.get() >= 1);
        send(
            id,
            wb,
            b"addMethod",
            &[
                Operand::object(id.intern_name(b"fromBits:").unwrap()),
                Operand::entry(original),
            ],
        );
        for bits in [0, 1, 1usize << 63, usize::MAX] {
            assert_eq!(
                send(id, probe, b"word:", &[Operand::word(bits)]).as_word(),
                Some(bits)
            );
        }
        assert!(
            id.send_named(probe, b"word:", &[Operand::object(word)])
                .is_err()
        );
        assert!(id.send_named(probe, b"word:", &[]).is_err());
        assert_eq!(
            send(
                id,
                probe,
                b"optional:",
                &[Operand::object(core::ptr::null_mut())]
            )
            .kind(),
            native::UNIT
        );
        let method = object(send(
            id,
            probe,
            b"methodFor:",
            &[Operand::object(id.intern_name(b"word:").unwrap())],
        ));
        let method_token = retain(id, roots, method);
        let pb = object(send(
            id,
            id.receiver,
            b"ensurePrivateVtableFor:",
            &[Operand::object(probe)],
        ));
        send(
            id,
            pb,
            b"addMethod:as:",
            &[
                Operand::object(method),
                Operand::object(id.intern_name(b"copiedWord:").unwrap()),
            ],
        );
        collect(id, gc);
        assert_eq!(
            send(id, probe, b"copiedWord:", &[Operand::word(99)]).as_word(),
            Some(99)
        );
        let source = object(send(
            id,
            string,
            b"fromBytes:",
            &[Operand::bytes(
                br#"[[IncomingCode nativeSignature] asString]"#,
            )],
        ));
        let token = retain(id, roots, source);
        let signature = object(send(
            id,
            named(b"Codegen"),
            b"eval:",
            &[Operand::object(source)],
        ));
        let signature_token = retain(id, roots, signature);
        let raw = send(id, signature, b"bytes", &[]).as_bytes().unwrap();
        assert_eq!(borrowed_bytes(raw.pointer, raw.length).unwrap(), b"ww");
        release(id, roots, signature_token);
        release(id, roots, token);
        release(id, roots, method_token);
        release(id, roots, probe_token);
    }
}
