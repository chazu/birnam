//! Byte arguments/results are copied before reentrant target/factory callbacks.
mod support;
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{Operand, Outcome, Protocol},
};
use std::{cell::Cell, ffi::c_void, os::unix::ffi::OsStrExt};
use support::*;
struct Hook {
    id: Protocol,
    gc: Oop,
    roots: Oop,
    bytes: Oop,
    input: Oop,
    token: Cell<usize>,
    calls: Cell<usize>,
    mode: Cell<u8>,
    recorded: Cell<usize>,
}
unsafe fn callback_input(h: &Hook) -> Result<(), Outcome> {
    let token = h.token.replace(0);
    if token != 0 {
        unsafe { release(h.id, h.roots, token) }
    };
    unsafe {
        h.id.send_named(
            h.input,
            b"appendBytes:",
            &[Operand::bytes(b"changed-buffer")],
        )?;
        collect(h.id, h.gc);
    }
    assert!(
        unsafe { present(h.id, h.input) },
        "helper must protect the byte owner through callback"
    );
    Ok(())
}
unsafe extern "C" fn byte_target(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let h = &*c.cast::<Hook>();
        let [value] = borrowed_slice(a, n).unwrap() else {
            panic!("one bytes argument")
        };
        let raw = value.as_bytes().unwrap();
        callback_input(h)?;
        assert_eq!(borrowed_bytes(raw.pointer, raw.length).unwrap(), b"ab");
        h.calls.set(h.calls.get() + 1);
        match h.mode.get() {
            1 => Ok(Operand::object(std::ptr::null_mut())),
            2 => Ok(Operand::unit()),
            3 => Err(Outcome::failed(821, 71)),
            _ => Ok(Operand::object(h.input)),
        }
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
unsafe extern "C" fn byte_factory(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let h = &*c.cast::<Hook>();
        let [value] = borrowed_slice(a, n).unwrap() else {
            panic!("one bytes result")
        };
        let raw = value.as_bytes().unwrap();
        callback_input(h)?;
        assert_eq!(borrowed_bytes(raw.pointer, raw.length).unwrap(), b"xy");
        h.calls.set(h.calls.get() + 1);
        if h.mode.get() == 3 {
            return Err(Outcome::failed(822, 73));
        }
        h.id.send_named(h.bytes, b"fromBytes:", &[*value])
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
unsafe extern "C" fn ignored_word(_: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    let [value] = unsafe { borrowed_slice(a, n) }.unwrap() else {
        panic!("one word argument")
    };
    assert_eq!(value.as_word(), Some(5));
    Outcome::loaded(Operand::bytes(b"ignored"))
}
unsafe extern "C" fn record_word(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    let h = unsafe { &*c.cast::<Hook>() };
    let [value] = unsafe { borrowed_slice(a, n) }.unwrap() else {
        panic!("one Word")
    };
    h.recorded.set(value.as_word().unwrap());
    unsafe { collect(h.id, h.gc) };
    Outcome::loaded(Operand::unit())
}
#[test]
#[ignore = "requires rebuilt typed outbound call modules"]
fn typed_byte_and_discard_calls_preserve_copies_factories_and_failures() {
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
        let roots = named(b"Roots");
        let gc = named(b"GC");
        let bytes = named(b"Bytes");
        let exe = named(b"Execution");
        let nil = named(b"nil");
        let mut tokens = Vec::new();
        let mut keep = |o: Oop| {
            tokens.push(retain(id, roots, o));
            o
        };
        let behavior = keep(behavior(id));
        let target = keep(allocate(id, behavior, 0));
        let factory = keep(allocate(id, behavior, 0));
        let mut h = Hook {
            id,
            gc,
            roots,
            bytes,
            input: nil,
            token: Cell::new(0),
            calls: Cell::new(0),
            mode: Cell::new(0),
            recorded: Cell::new(usize::MAX),
        };
        let c = (&raw mut h).cast();
        install(id, behavior, b"consume:", byte_target, c);
        install(id, behavior, b"fromBytes:", byte_factory, c);
        install(
            id,
            behavior,
            b"discard:",
            ignored_word,
            std::ptr::null_mut(),
        );
        let selector = keep(id.intern_name(b"consume:").unwrap());
        let from_bytes = keep(id.intern_name(b"bytes").unwrap());
        let object_name = b"objectResultOf:selector:byteArgument:";
        let byte_name = b"bytesResultOf:selector:resultFactory:";
        let discard_name = b"discardResultOf:selector:wordArgument:";
        for mode in 0..4 {
            h.mode.set(mode);
            h.input = object(send(id, bytes, b"fromBytes:", &[Operand::bytes(b"ab")]));
            h.token.set(retain(id, roots, h.input));
            let outcome = id.send_named(
                exe,
                object_name,
                &[
                    Operand::object(target),
                    Operand::object(selector),
                    Operand::object(h.input),
                ],
            );
            match mode {
                0 => {
                    assert_eq!(outcome.unwrap().as_object(), Some(h.input));
                    keep(h.input);
                }
                1 => assert_eq!(outcome.unwrap().as_object(), Some(nil)),
                2 => assert_eq!(outcome.unwrap_err().status, 1),
                _ => {
                    let e = outcome.unwrap_err();
                    assert_eq!((e.status, e.detail), (821, 71));
                }
            }
        }
        for mode in [0, 3] {
            h.mode.set(mode);
            h.input = object(send(id, bytes, b"fromBytes:", &[Operand::bytes(b"xy")]));
            h.token.set(retain(id, roots, h.input));
            let outcome = id.send_named(
                exe,
                byte_name,
                &[
                    Operand::object(h.input),
                    Operand::object(from_bytes),
                    Operand::object(factory),
                ],
            );
            if mode == 0 {
                let result = keep(object(outcome.unwrap()));
                let raw = send(id, result, b"bytes", &[]).as_bytes().unwrap();
                assert_eq!(borrowed_bytes(raw.pointer, raw.length).unwrap(), b"xy");
            } else {
                let e = outcome.unwrap_err();
                assert_eq!((e.status, e.detail), (822, 73));
            }
        }
        h.mode.set(0);
        let value = keep(object(send(
            id,
            named(b"Word"),
            b"fromBits:",
            &[Operand::word(5)],
        )));
        let discard = keep(id.intern_name(b"discard:").unwrap());
        assert_eq!(
            send(
                id,
                exe,
                discard_name,
                &[
                    Operand::object(target),
                    Operand::object(discard),
                    Operand::object(value)
                ]
            )
            .as_object(),
            Some(nil)
        );
        for name in [
            object_name.as_slice(),
            byte_name.as_slice(),
            discard_name.as_slice(),
        ] {
            let args = [
                Operand::object(target),
                Operand::object(selector),
                Operand::object(value),
            ];
            for count in 0..3 {
                assert!(id.send_named(exe, name, &args[..count]).is_err());
            }
            for index in 0..3 {
                let mut bad = args;
                bad[index] = Operand::word(0);
                assert!(id.send_named(exe, name, &bad).is_err());
            }
        }
        let wrong = id
            .send_named(
                exe,
                byte_name,
                &[
                    Operand::object(named(b"Word")),
                    Operand::object(id.intern_name(b"bits").unwrap()),
                    Operand::object(factory),
                ],
            )
            .unwrap_err();
        assert_eq!(wrong.status, 1);
        install(id, behavior, b"record:", record_word, c);
        let record = keep(id.intern_name(b"record:").unwrap());
        for (boolean, expected) in [(named(b"true"), 1), (named(b"false"), 0)] {
            let returned = send(
                id,
                named(b"Boolean"),
                b"sendWordOf:to:selector:",
                &[
                    Operand::object(boolean),
                    Operand::object(target),
                    Operand::object(record),
                ],
            );
            assert_eq!(returned.as_object(), Some(target));
            assert_eq!(h.recorded.get(), expected);
        }
        let snapshot_source = keep(object(send(
            id,
            bytes,
            b"fromBytes:",
            &[Operand::bytes(b"policy snapshot")],
        )));
        let snapshot = keep(object(send(
            id,
            bytes,
            b"fromByteResultOf:selector:",
            &[
                Operand::object(snapshot_source),
                Operand::object(from_bytes),
            ],
        )));
        let raw = send(id, snapshot, b"bytes", &[]).as_bytes().unwrap();
        assert_eq!(
            borrowed_bytes(raw.pointer, raw.length).unwrap(),
            b"policy snapshot"
        );
        let integer = named(b"Integer");
        let zero = keep(object(send(
            id,
            integer,
            b"fromWordResultOf:selector:",
            &[
                Operand::object(integer),
                Operand::object(id.intern_name(b"word").unwrap()),
            ],
        )));
        assert_eq!(send(id, zero, b"word", &[]).as_word(), Some(0));
        for old in [
            b"sendNativeTo:selector:arguments:signature:resultFactory:".as_slice(),
            b"tryNativeArguments:",
        ] {
            assert!(
                send(
                    id,
                    id.receiver,
                    b"bindingOf:in:",
                    &[
                        Operand::object(id.intern_name(old).unwrap()),
                        Operand::object(object(send(
                            id,
                            id.receiver,
                            b"behaviorOf:",
                            &[Operand::object(exe)]
                        )))
                    ]
                )
                .as_entry()
                .is_none()
            );
        }
        for token in tokens {
            release(id, roots, token);
        }
    }
}
