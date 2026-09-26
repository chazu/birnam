//! Real String/Error libraries loaded by Bootstrap through Siphon and Id.
mod support;
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use core::{cell::Cell, ffi::c_void, ptr};
use std::os::unix::ffi::OsStrExt;
use support::*;
unsafe fn text(id: Protocol, value: Oop) -> Vec<u8> {
    let bytes = unsafe { send(id, value, b"bytes", &[]) }
        .as_bytes()
        .unwrap();
    unsafe { borrowed_bytes(bytes.pointer, bytes.length) }
        .unwrap()
        .to_vec()
}
unsafe fn make(id: Protocol, string: Oop, bytes: &[u8]) -> Oop {
    object(unsafe { send(id, string, b"fromBytes:", &[Operand::bytes(bytes)]) })
}
unsafe fn selected(id: Protocol, behavior: Oop, name: &[u8]) -> NativeEntry {
    unsafe {
        send(
            id,
            behavior,
            b"lookup",
            &[Operand::object(id.intern_name(name).unwrap())],
        )
    }
    .as_entry()
    .unwrap()
}
unsafe fn alias(id: Protocol, vt: Oop, name: &[u8], entry: NativeEntry) -> Oop {
    unsafe {
        let selector = id.intern_name(name).unwrap();
        send(
            id,
            vt,
            b"addMethod",
            &[Operand::object(selector), Operand::entry(entry)],
        );
        selector
    }
}
unsafe fn restore(id: Protocol, vt: Oop, name: &[u8], entry: NativeEntry, alias: Oop) {
    unsafe {
        send(
            id,
            vt,
            b"addMethod",
            &[
                Operand::object(id.intern_name(name).unwrap()),
                Operand::entry(entry),
            ],
        );
        send(id, vt, b"removeMethod:", &[Operand::object(alias)]);
    }
}
struct Counted {
    id: Protocol,
    alias: Oop,
    calls: Cell<usize>,
}
unsafe extern "C" fn counted(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let c = unsafe { &*context.cast::<Counted>() };
    c.calls.set(c.calls.get() + 1);
    unsafe {
        c.id.send(receiver, c.alias, borrowed_slice(args, count).unwrap())
    }
    .map_or_else(|e| e, Outcome::loaded)
}
struct Replacement {
    id: Protocol,
    gc: Oop,
    value: Oop,
}
unsafe extern "C" fn replacement(
    context: *mut c_void,
    _: Oop,
    _: *const Operand,
    _: usize,
) -> Outcome {
    let r = unsafe { &*context.cast::<Replacement>() };
    match unsafe { r.id.send_named(r.gc, b"collectPrecisely", &[]) } {
        Ok(_) => Outcome::loaded(Operand::object(r.value)),
        Err(e) => e,
    }
}
struct Allocating {
    id: Protocol,
    gc: Oop,
    alias: Oop,
    input: *mut u8,
    calls: Cell<usize>,
}
unsafe extern "C" fn allocating(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let a = unsafe { &*context.cast::<Allocating>() };
    a.calls.set(a.calls.get() + 1);
    if !a.input.is_null() {
        unsafe { a.input.write(b'z') };
    }
    if let Err(e) = unsafe { a.id.send_named(a.gc, b"collectPrecisely", &[]) } {
        return e;
    }
    unsafe {
        a.id.send(receiver, a.alias, borrowed_slice(args, count).unwrap())
    }
    .map_or_else(|e| e, Outcome::loaded)
}
struct Finalized {
    id: Protocol,
    gc: Oop,
    alias: Oop,
    calls: Cell<usize>,
    string: Oop,
}
unsafe extern "C" fn finalized(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let f = unsafe { &*context.cast::<Finalized>() };
    f.calls.set(f.calls.get() + 1);
    // Retiring a byte buffer must not allow allocating text from a finalizer.
    if unsafe {
        f.id.send_named(f.string, b"fromBytes:", &[Operand::bytes(b"forbidden")])
    }
    .is_ok()
    {
        return Outcome::failed(native::INVALID_REQUEST, 1);
    }
    if unsafe { f.id.send_named(f.gc, b"collectPrecisely", &[]) }
        .ok()
        .and_then(|v| v.as_word())
        != Some(0)
    {
        return Outcome::failed(native::INVALID_REQUEST, 2);
    }
    unsafe {
        f.id.send(receiver, f.alias, borrowed_slice(args, count).unwrap())
    }
    .map_or_else(|e| e, Outcome::loaded)
}
#[test]
#[ignore = "scripts/native-message-test.sh builds production dylibs"]
fn owned_text_builders_and_errors_use_live_messages_and_gc() {
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
        let string = named(b"String");
        let sb = named(b"StringBuilder");
        let error = named(b"Error");
        let svt = named(b"StringVt");
        let evt = named(b"ErrorFactoryVt");
        let base = named(b"ObjectVt");
        let gc = named(b"GC");
        let roots = named(b"Roots");
        let truth = named(b"true");
        let nil = named(b"nil");
        assert_eq!(load(id, "string", &[Operand::object(ns)]), string);
        assert_eq!(load(id, "error", &[Operand::object(ns)]), error);
        let mut tokens = Vec::new();
        let mut keep = |value| {
            tokens.push(retain(id, roots, value));
            value
        };
        for (value, expected) in [
            (truth, b"true".as_slice()),
            (named(b"false"), b"false"),
            (nil, b"nil"),
        ] {
            let printed = keep(object(send(id, value, b"printString", &[])));
            assert_eq!(text(id, printed), expected);
        }
        let raw = [0, 0xff, b'a', b' ', b'\t'];
        let value = keep(make(id, string, &raw));
        assert_eq!(text(id, value), raw);
        assert_eq!(send(id, value, b"length", &[]).as_word(), Some(raw.len()));
        assert_eq!(object(send(id, value, b"printString", &[])), value);
        let clone = keep(object(send(id, value, b"clone", &[])));
        assert_ne!(clone, value);
        assert_eq!(text(id, clone), raw);
        let a = send(id, value, b"bytes", &[]).as_bytes().unwrap();
        let b = send(id, clone, b"bytes", &[]).as_bytes().unwrap();
        assert_ne!(a.pointer, b.pointer);
        assert_eq!(
            object(send(id, value, b"=", &[Operand::object(clone)])),
            truth
        );
        assert_eq!(
            object(send(id, string, b"=", &[Operand::object(nil)])),
            named(b"false")
        );
        // Raw payload corruption and private behavior mutation cannot redirect ownership.
        value.cast::<usize>().write(usize::MAX);
        value.cast::<usize>().add(1).write(usize::MAX);
        send(
            id,
            id.receiver,
            b"ensurePrivateVtableFor:",
            &[Operand::object(value)],
        );
        collect(id, gc);
        assert_eq!(text(id, value), raw);
        assert_eq!(text(id, keep(object(send(id, value, b"new", &[])))), raw);
        let trimmed = keep(make(id, string, b" \t\r\n\x0b\x0c hello \x0c"));
        assert_eq!(
            text(id, keep(object(send(id, trimmed, b"trimmed", &[])))),
            b"hello"
        );
        let needle = keep(make(id, string, b"aa"));
        let repl = keep(make(id, string, b"b"));
        let source = keep(make(id, string, b"aaaaa"));
        assert_eq!(
            text(
                id,
                keep(object(send(
                    id,
                    source,
                    b"replace:with:",
                    &[Operand::object(needle), Operand::object(repl)]
                )))
            ),
            b"bba"
        );
        assert_eq!(
            text(
                id,
                keep(object(send(
                    id,
                    source,
                    b"replace:with:",
                    &[Operand::object(string), Operand::object(repl)]
                )))
            ),
            b"aaaaa"
        );
        assert_eq!(
            text(
                id,
                keep(object(send(id, source, b",", &[Operand::object(truth)])))
            ),
            b"aaaaatrue"
        );
        let symbolic = keep(make(id, string, b"opaque\0tail"));
        assert_eq!(
            object(send(id, symbolic, b"asSymbol", &[])),
            id.intern_name(b"opaque").unwrap()
        );
        // Numeric text is installed after both completed number families.
        for name in [b"size".as_slice(), b"at:", b"asInteger", b"asNumber"] {
            assert_eq!(
                send(
                    id,
                    svt,
                    b"lookup",
                    &[Operand::object(id.intern_name(name).unwrap())]
                )
                .kind(),
                native::NATIVE_ENTRY
            );
        }
        let builder = keep(object(send(id, sb, b"new", &[])));
        send(id, builder, b"add:", &[Operand::object(source)]);
        send(id, builder, b"add:", &[Operand::object(truth)]);
        assert_eq!(
            text(id, keep(object(send(id, builder, b"asString", &[])))),
            b"aaaaatrue"
        );
        send(id, builder, b"add:", &[Operand::object(builder)]);
        assert_eq!(
            text(id, keep(object(send(id, builder, b"printString", &[])))),
            b"aaaaatrueaaaaatrue"
        );
        let empty = keep(object(send(id, builder, b"clone", &[])));
        assert_eq!(
            text(id, keep(object(send(id, empty, b"asString", &[])))),
            b""
        );
        send(
            id,
            id.receiver,
            b"ensurePrivateVtableFor:",
            &[Operand::object(builder)],
        );
        collect(id, gc);
        send(id, builder, b"add:", &[Operand::object(nil)]);
        assert!(text(id, keep(object(send(id, builder, b"asString", &[])))).ends_with(b"nil"));

        // Every construction by printing/operations observes the current String factory.
        let original = selected(id, svt, b"fromBytes:");
        let counter = Box::leak(Box::new(Counted {
            id,
            alias: alias(id, svt, b"fixtureFromBytes:", original),
            calls: Cell::new(0),
        }));
        install(
            id,
            svt,
            b"fromBytes:",
            counted,
            ptr::from_mut(counter).cast(),
        );
        keep(object(send(id, truth, b"printString", &[])));
        keep(object(send(id, nil, b"printString", &[])));
        keep(object(send(id, source, b"trimmed", &[])));
        keep(object(send(id, builder, b"asString", &[])));
        assert_eq!(counter.calls.get(), 4);
        restore(id, svt, b"fromBytes:", original, counter.alias);

        // Bytes are copied before an allocating callback can mutate their source.
        let allocation = selected(id, id.receiver, b"allocate");
        let mut input = *b"abc";
        let alloc = Box::leak(Box::new(Allocating {
            id,
            gc,
            alias: alias(id, id.receiver, b"fixtureAllocate", allocation),
            input: input.as_mut_ptr(),
            calls: Cell::new(0),
        }));
        install(
            id,
            id.receiver,
            b"allocate",
            allocating,
            ptr::from_mut(alloc).cast(),
        );
        let copied = keep(make(id, string, &input));
        assert_eq!(&input, b"zbc");
        assert_eq!(text(id, copied), b"abc");
        let snapshot = keep(object(send(id, copied, b"clone", &[])));
        assert_eq!(text(id, snapshot), b"abc");
        assert_eq!(alloc.calls.get(), 2);
        restore(id, id.receiver, b"allocate", allocation, alloc.alias);

        // A printString callback may collect and reenter String construction.
        let custom_behavior = keep(behavior(id));
        let custom = keep(allocate(id, custom_behavior, 0));
        let response = keep(make(id, string, b"callback"));
        let replacement = Box::leak(Box::new(Replacement {
            id,
            gc,
            value: response,
        }));
        install(
            id,
            custom_behavior,
            b"printString",
            self::replacement,
            ptr::from_mut(replacement).cast(),
        );
        assert_eq!(
            text(
                id,
                keep(object(send(id, source, b",", &[Operand::object(custom)])))
            ),
            b"aaaaacallback"
        );
        send(id, builder, b"add:", &[Operand::object(custom)]);

        let kind = keep(id.intern_name(b"problem").unwrap());
        let message = make(id, string, b"retained by error");
        let fault = keep(object(send(
            id,
            error,
            b"kind:message:",
            &[Operand::object(kind), Operand::object(message)],
        )));
        collect(id, gc);
        assert!(present(id, message));
        assert_eq!(object(send(id, fault, b"kind", &[])), kind);
        assert_eq!(object(send(id, fault, b"message", &[])), message);
        assert_eq!(object(send(id, fault, b"isError", &[])), truth);
        assert_eq!(object(send(id, source, b"isError", &[])), named(b"false"));
        assert_eq!(
            object(send(
                id,
                fault,
                b"unknownMessage:",
                &[Operand::object(source)]
            )),
            fault
        );
        let copied_error = keep(object(send(id, fault, b"clone", &[])));
        assert_ne!(copied_error, fault);
        collect(id, gc);
        assert_eq!(object(send(id, copied_error, b"message", &[])), message);
        let only_kind = keep(object(send(id, error, b"kind:", &[Operand::object(kind)])));
        assert_eq!(object(send(id, only_kind, b"message", &[])), nil);
        let only_message = keep(object(send(
            id,
            error,
            b"message:",
            &[Operand::object(message)],
        )));
        assert_eq!(
            object(send(id, only_message, b"kind", &[])),
            id.intern_name(b"error").unwrap()
        );
        let null = keep(object(send(
            id,
            error,
            b"kind:",
            &[Operand::object(ptr::null_mut())],
        )));
        assert_eq!(object(send(id, null, b"kind", &[])), nil);
        let dnu = keep(object(send(id, source, b"missingTextMethod", &[])));
        assert_eq!(
            object(send(id, dnu, b"kind", &[])),
            id.intern_name(b"doesNotUnderstand").unwrap()
        );
        assert_eq!(
            text(id, object(send(id, dnu, b"message", &[]))),
            b"does not understand #missingTextMethod"
        );
        let factory = selected(id, evt, b"kind:message:");
        replacement.value = fault;
        install(
            id,
            evt,
            b"kind:message:",
            self::replacement,
            ptr::from_mut(replacement).cast(),
        );
        assert_eq!(
            object(send(id, source, b"anotherMissingMethod", &[])),
            fault
        );
        send(
            id,
            evt,
            b"addMethod",
            &[
                Operand::object(id.intern_name(b"kind:message:").unwrap()),
                Operand::entry(factory),
            ],
        );

        // Native buffers are weak: finalization frees each separately, including
        // builders. Repeated allocations also exercise address reuse.
        collect(id, gc);
        let fin = selected(id, svt, b"finalize:");
        let finalizer = Box::leak(Box::new(Finalized {
            id,
            gc,
            alias: alias(id, svt, b"fixtureFinalize:", fin),
            calls: Cell::new(0),
            string,
        }));
        install(
            id,
            svt,
            b"finalize:",
            finalized,
            ptr::from_mut(finalizer).cast(),
        );
        for _ in 0..32 {
            let doomed = make(id, string, b"temporary");
            let doomed_builder = object(send(id, sb, b"new", &[]));
            assert_eq!(collect(id, gc), 2);
            assert!(!present(id, doomed));
            assert!(!present(id, doomed_builder));
        }
        assert_eq!(finalizer.calls.get(), 64);
        restore(id, svt, b"finalize:", fin, finalizer.alias);
        // Automatic collections across construction, appending and Error slot writes.
        send(id, gc, b"budget:", &[Operand::word(1)]);
        for _ in 0..8 {
            let s = keep(make(id, string, b"budget"));
            let e = keep(object(send(id, error, b"message:", &[Operand::object(s)])));
            assert_eq!(object(send(id, e, b"message", &[])), s);
            send(id, builder, b"add:", &[Operand::object(s)]);
        }
        send(id, gc, b"budget:", &[Operand::word(1024 * 1024)]);
        for token in tokens {
            release(id, roots, token);
        }
        collect(id, gc);
        assert!(!present(id, message));
        assert!(present(id, string));
        assert!(present(id, error));
        assert!(present(id, sb));
        // Copying a String method to unrelated payload never exposes raw pointers.
        let clone = selected(id, svt, b"clone");
        send(
            id,
            base,
            b"addMethod",
            &[
                Operand::object(id.intern_name(b"badStringClone").unwrap()),
                Operand::entry(clone),
            ],
        );
        assert!(
            id.send_named(named(b"Object"), b"badStringClone", &[])
                .is_err()
        );
    }
}
