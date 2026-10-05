//! Authored Boolean behavior; the native seed remains required for cold startup.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use core::{cell::Cell, ffi::c_void};
use std::os::unix::ffi::OsStrExt;
use support::*;
struct Callback {
    id: Protocol,
    gc: Oop,
    answer: Cell<Operand>,
    calls: Cell<usize>,
    fail: Cell<bool>,
}
unsafe extern "C" fn value(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    let s = unsafe { &*c.cast::<Callback>() };
    if !unsafe { borrowed_slice(a, n) }.is_some_and(|a| a.is_empty()) {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    }
    s.calls.set(s.calls.get() + 1);
    if s.fail.get() {
        return Outcome::failed(native::MAPPING_FAILED, 413);
    }
    if let Err(e) = unsafe { s.id.send_named(s.gc, b"collectPrecisely", &[]) } {
        return e;
    }
    Outcome::loaded(s.answer.get())
}
#[test]
#[ignore = "production Boolean artifact required"]
fn authored_boolean_preserves_identity_branching_carriers_and_replacements() {
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
        let (truth, falsity, nil, gc, roots) = (
            named(b"true"),
            named(b"false"),
            named(b"nil"),
            named(b"GC"),
            named(b"Roots"),
        );
        let vt = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(truth)],
        ));
        let other = allocate(id, vt, 0);
        let other_token = retain(id, roots, other);
        let mut callback = Callback {
            id,
            gc,
            answer: Cell::new(Operand::object(truth)),
            calls: Cell::new(0),
            fail: Cell::new(false),
        };
        let cvt = behavior(id);
        install(id, cvt, b"value", value, (&raw mut callback).cast());
        let block = allocate(id, cvt, 0);
        let token = retain(id, roots, block);
        assert_eq!(send(id, truth, b"not", &[]).as_object(), Some(falsity));
        assert_eq!(send(id, falsity, b"not", &[]).as_object(), Some(truth));
        assert_eq!(send(id, other, b"not", &[]).as_object(), Some(nil));
        for (receiver, and_answer, or_answer) in [
            (truth, block, truth),
            (falsity, falsity, block),
            (other, nil, nil),
        ] {
            assert_eq!(
                send(id, receiver, b"&", &[Operand::object(block)]).as_object(),
                Some(and_answer)
            );
            assert_eq!(
                send(id, receiver, b"|", &[Operand::object(block)]).as_object(),
                Some(or_answer)
            );
        }
        for (receiver, selector, expected) in [
            (falsity, b"ifTrue:".as_slice(), nil),
            (truth, b"ifFalse:", nil),
            (falsity, b"and:", falsity),
            (truth, b"or:", truth),
            (other, b"ifTrue:", nil),
            (other, b"and:", nil),
        ] {
            assert_eq!(
                send(id, receiver, selector, &[Operand::object(block)]).as_object(),
                Some(expected)
            );
        }
        assert_eq!(callback.calls.get(), 0);
        let raw_entry = NativeEntry::new(value, (&raw mut callback).cast());
        for result in [
            Operand::object(falsity),
            Operand::unit(),
            Operand::word(usize::MAX),
            Operand::bytes(b"raw-bytes"),
            Operand::entry(raw_entry),
            Operand::protocol(&id),
        ] {
            callback.answer.set(result);
            let returned = send(id, truth, b"ifTrue:", &[Operand::object(block)]);
            assert_eq!(returned.kind(), result.kind());
            match result.kind() {
                native::OBJECT => assert_eq!(returned.as_object(), result.as_object()),
                native::WORD => assert_eq!(returned.as_word(), result.as_word()),
                native::NATIVE_ENTRY => assert_eq!(returned.as_entry(), result.as_entry()),
                native::BYTES => {
                    let raw = returned.as_bytes().unwrap();
                    assert_eq!(
                        birnam_abi::borrowed_bytes(raw.pointer, raw.length).unwrap(),
                        b"raw-bytes"
                    )
                }
                _ => {}
            }
        }
        assert_eq!(callback.calls.get(), 6);
        callback.fail.set(true);
        let error = id
            .send_named(falsity, b"ifFalse:", &[Operand::object(block)])
            .unwrap_err();
        assert_eq!((error.status, error.detail), (native::MAPPING_FAILED, 413));
        callback.fail.set(false);
        for selector in [b"ifTrue:".as_slice(), b"&", b"|"] {
            assert!(id.send_named(truth, selector, &[]).is_err());
            assert!(
                id.send_named(truth, selector, &[Operand::object(core::ptr::null_mut())])
                    .is_err()
            );
        }
        // The callback's current method remains live after an authored branch is selected.
        callback.answer.set(Operand::object(block));
        assert_eq!(
            send(
                id,
                truth,
                b"ifTrue:ifFalse:",
                &[Operand::object(block), Operand::object(nil)]
            )
            .as_object(),
            Some(block)
        );
        let method = send(
            id,
            vt,
            b"lookup",
            &[Operand::object(id.intern_name(b"ifTrue:").unwrap())],
        )
        .as_entry()
        .unwrap();
        assert!(
            send(
                id,
                named(b"Execution"),
                b"implementationOfEntry:",
                &[Operand::entry(method)]
            )
            .as_object()
            .is_some()
        );
        let source = object(send(
            id,
            named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(b"[Module load: #booleanValues]")],
        ));
        let st = retain(id, roots, source);
        let before = send(
            id,
            vt,
            b"lookup",
            &[Operand::object(id.intern_name(b"ifTrue:").unwrap())],
        )
        .as_entry();
        send(id, named(b"Codegen"), b"eval:", &[Operand::object(source)]);
        assert_eq!(
            send(
                id,
                vt,
                b"lookup",
                &[Operand::object(id.intern_name(b"ifTrue:").unwrap())]
            )
            .as_entry(),
            before
        );
        release(id, roots, st);
        release(id, roots, token);
        release(id, roots, other_token);
    }
}
