//! The primitive owners and the explicit native-carrier boundary remain replaceable.
mod support;
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use core::{cell::Cell, ffi::c_void};
use std::os::unix::ffi::OsStrExt;
use support::*;
struct Factory {
    id: Protocol,
    entry: NativeEntry,
    gc: Oop,
    calls: Cell<usize>,
}
unsafe extern "C" fn collecting_factory(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    let f = unsafe { &*c.cast::<Factory>() };
    f.calls.set(f.calls.get() + 1);
    if let Err(e) = unsafe { f.id.send_named(f.gc, b"collectPrecisely", &[]) } {
        return e;
    }
    unsafe { (f.entry.method.unwrap())(f.entry.context, r, a, n) }
}
struct Reentry {
    id: Protocol,
    bytes: Oop,
    gc: Oop,
}
unsafe extern "C" fn byte_callback(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    let s = unsafe { &*c.cast::<Reentry>() };
    let Some([a, b]) = (unsafe { borrowed_slice(a, n) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some(a) = a.as_bytes() else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some(b) = b.as_bytes() else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let answer = (|| unsafe {
        // Deliberately mutate the provider and collect before reading the inputs.
        s.id.send_named(s.bytes, b"appendBytes:", &[Operand::bytes(b"changed")])?;
        s.id.send_named(s.gc, b"collectPrecisely", &[])?;
        let a = borrowed_bytes(a.pointer, a.length).unwrap();
        let b = borrowed_bytes(b.pointer, b.length).unwrap();
        assert_eq!(a, b"ab");
        assert_eq!(b, b"ab");
        Ok(Operand::word(a.len() + b.len()))
    })();
    answer.map_or_else(|e| e, Outcome::loaded)
}
unsafe extern "C" fn failure(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 987)
}
#[test]
#[ignore = "production dylibs and authored nativeValues artifact required"]
fn primitive_ownership_and_explicit_abi_use_live_id_messages() {
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
        let (word, bytes, string, integer, execution, roots, gc, nil) = (
            named(b"Word"),
            named(b"Bytes"),
            named(b"String"),
            named(b"Integer"),
            named(b"Execution"),
            named(b"Roots"),
            named(b"GC"),
            named(b"nil"),
        );
        let mut tokens = Vec::new();
        let mut keep = |v: Operand| {
            let v = object(v);
            tokens.push(retain(id, roots, v));
            v
        };
        let w = |v: usize| send(id, word, b"fromBits:", &[Operand::word(v)]);
        let zero = keep(w(0));
        let one = keep(w(1));
        let max = keep(w(usize::MAX));
        let sixty_four = keep(w(64));
        let bits = |v: Operand| send(id, object(v), b"bits", &[]).as_word().unwrap();
        assert_eq!(
            bits(send(id, max, b"wrappingAdd:", &[Operand::object(one)])),
            0
        );
        assert_eq!(
            bits(send(
                id,
                zero,
                b"wrappingSubtract:",
                &[Operand::object(one)]
            )),
            usize::MAX
        );
        assert_eq!(
            bits(send(
                id,
                max,
                b"shiftRight:",
                &[Operand::object(sixty_four)]
            )),
            0
        );
        assert_eq!(
            send(id, max, b"unsignedLessThan:", &[Operand::object(one)]).as_object(),
            Some(named(b"false"))
        );
        assert!(
            id.send_named(one, b"unsignedDivide:", &[Operand::object(zero)])
                .is_err()
        );
        let rendered = keep(send(id, max, b"printString", &[]));
        let raw = send(id, rendered, b"bytes", &[]).as_bytes().unwrap();
        assert_eq!(
            borrowed_bytes(raw.pointer, raw.length).unwrap(),
            b"0xffffffffffffffff"
        );
        let error = keep(send(id, max, b"asInteger", &[]));
        assert_eq!(
            object(send(id, error, b"kind", &[])),
            id.intern_name(b"overflow").unwrap()
        );
        let data = keep(send(id, bytes, b"fromBytes:", &[Operand::bytes(b"ab")]));
        assert_eq!(
            send(id, data, b"byteAtWord:", &[Operand::word(1)]).as_word(),
            Some(b'b' as usize)
        );
        assert_eq!(
            send(id, data, b"byteAtWord:", &[Operand::word(usize::MAX)]).kind(),
            native::UNIT
        );
        let vector = named(b"Vector");
        let mut args = keep(send(id, vector, b"new", &[]));
        for _ in 0..2 {
            args = keep(send(id, args, b"add:", &[Operand::object(data)]));
        }
        let signature = keep(send(id, string, b"fromBytes:", &[Operand::bytes(b"wbb")]));
        let reentry = Box::leak(Box::new(Reentry {
            id,
            bytes: data,
            gc,
        }));
        install(
            id,
            id.receiver,
            b"byteCallback",
            byte_callback,
            core::ptr::from_mut(reentry).cast(),
        );
        let result = keep(send(
            id,
            execution,
            b"sendNativeTo:selector:arguments:signature:resultFactory:",
            &[
                Operand::object(id.receiver),
                Operand::object(id.intern_name(b"byteCallback").unwrap()),
                Operand::object(args),
                Operand::object(signature),
                Operand::object(word),
            ],
        ));
        assert_eq!(bits(Operand::object(result)), 4);
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
        let factory = Box::leak(Box::new(Factory {
            id,
            entry: original,
            gc,
            calls: Cell::new(0),
        }));
        install(
            id,
            wb,
            b"fromBits:",
            collecting_factory,
            core::ptr::from_mut(factory).cast(),
        );
        let result = keep(send(
            id,
            execution,
            b"wordResultOf:selector:",
            &[
                Operand::object(integer),
                Operand::object(id.intern_name(b"word").unwrap()),
            ],
        ));
        assert_eq!(bits(Operand::object(result)), 0);
        assert_eq!(factory.calls.get(), 1);
        send(
            id,
            wb,
            b"addMethod",
            &[
                Operand::object(id.intern_name(b"fromBits:").unwrap()),
                Operand::entry(original),
            ],
        );
        let snapshot = keep(send(
            id,
            execution,
            b"bytesResultOf:selector:",
            &[
                Operand::object(data),
                Operand::object(id.intern_name(b"bytes").unwrap()),
            ],
        ));
        send(id, data, b"appendBytes:", &[Operand::bytes(b"!")]);
        collect(id, gc);
        let raw = send(id, snapshot, b"bytes", &[]).as_bytes().unwrap();
        assert_eq!(
            borrowed_bytes(raw.pointer, raw.length).unwrap(),
            b"abchanged"
        );
        install(
            id,
            id.receiver,
            b"abiFailure",
            failure,
            core::ptr::null_mut(),
        );
        let empty = keep(send(id, vector, b"new", &[]));
        let sig = keep(send(id, string, b"fromBytes:", &[Operand::bytes(b"w")]));
        let e = id
            .send_named(
                execution,
                b"sendNativeTo:selector:arguments:signature:resultFactory:",
                &[
                    Operand::object(id.receiver),
                    Operand::object(id.intern_name(b"abiFailure").unwrap()),
                    Operand::object(empty),
                    Operand::object(sig),
                    Operand::object(word),
                ],
            )
            .unwrap_err();
        assert_eq!((e.status, e.detail), (native::INVALID_REQUEST, 987));
        assert_eq!(nil, named(b"nil"));
        for token in tokens {
            release(id, roots, token);
        }
    }
}
