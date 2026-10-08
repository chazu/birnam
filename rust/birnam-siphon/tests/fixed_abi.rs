//! Fixed carrier shapes avoid signature/container permission machinery.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use std::{cell::Cell, ffi::c_void, os::unix::ffi::OsStrExt};
use support::*;
struct Hook {
    id: Protocol,
    roots: Oop,
    gc: Oop,
    word: Oop,
    word_vt: Oop,
    target_vt: Oop,
    original_bits: NativeEntry,
    original_factory: NativeEntry,
    root_to_drop: Cell<usize>,
    expected: Cell<Oop>,
    calls: Cell<usize>,
    factory_calls: Cell<usize>,
    mode: Cell<u8>,
    replace_factory: Cell<bool>,
    replace_target: Cell<bool>,
}
unsafe extern "C" fn factory(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let h = unsafe { &*c.cast::<Hook>() };
    h.factory_calls.set(h.factory_calls.get() + 1);
    unsafe {
        collect(h.id, h.gc);
        (h.original_factory.method.unwrap())(h.original_factory.context, r, a, n)
    }
}
unsafe extern "C" fn optional(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let h = &*c.cast::<Hook>();
        let [value] = borrowed_slice(a, n).unwrap() else {
            panic!("one Object argument")
        };
        assert_eq!(value.as_object(), Some(h.expected.get()));
        let token = h.root_to_drop.replace(0);
        if token != 0 {
            release(h.id, h.roots, token);
        }
        collect(h.id, h.gc);
        assert!(
            present(h.id, h.expected.get()),
            "typed call must scope input before callback"
        );
        h.calls.set(h.calls.get() + 1);
        if h.replace_factory.get() {
            install(h.id, h.word_vt, b"fromBits:", factory, c);
        }
        match h.mode.get() {
            1 => Ok(Operand::unit()),
            2 => Ok(Operand::object(h.word)),
            3 => Err(Outcome::failed(813, 29)),
            4 => Ok(Operand::word(usize::MAX)),
            5 => Ok(Operand::word(0)),
            _ => Ok(Operand::word(37)),
        }
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
unsafe extern "C" fn object_word(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let h = &*c.cast::<Hook>();
        let [value] = borrowed_slice(a, n).unwrap() else {
            panic!("one Word argument")
        };
        assert_eq!(value.as_word(), Some(37));
        collect(h.id, h.gc);
        assert!(
            present(h.id, h.expected.get()),
            "Word argument stays live during target callback"
        );
        h.calls.set(h.calls.get() + 1);
        match h.mode.get() {
            1 => Ok(Operand::object(std::ptr::null_mut())),
            2 => Ok(Operand::word(37)),
            3 => Err(Outcome::failed(814, 31)),
            _ => {
                Ok(h.id.send_named(h.word, b"fromBits:", &[Operand::word(71)])?)
            }
        }
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
unsafe extern "C" fn bits(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    let h = unsafe { &*c.cast::<Hook>() };
    assert_eq!(n, 0);
    let token = h.root_to_drop.replace(0);
    if token != 0 {
        unsafe { release(h.id, h.roots, token) }
    };
    unsafe { collect(h.id, h.gc) };
    assert!(unsafe { present(h.id, h.expected.get()) });
    if h.replace_target.get() {
        unsafe { install(h.id, h.target_vt, b"object:", deny, c) };
    }
    Outcome::loaded(Operand::word(37))
}
unsafe extern "C" fn deny(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(815, 33)
}
#[test]
#[ignore = "requires production Execution and Integer artifacts"]
fn fixed_carriers_preserve_current_targets_factories_and_callback_lifetime() {
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
        let mut tokens = Vec::new();
        let mut keep = |value: Oop| {
            tokens.push(retain(id, roots, value));
            value
        };
        let word = named(b"Word");
        let execution = named(b"Execution");
        let gc = named(b"GC");
        let nil = named(b"nil");
        let vt = |o| object(send(id, id.receiver, b"behaviorOf:", &[Operand::object(o)]));
        let bind = |o, name: &[u8]| {
            send(
                id,
                id.receiver,
                b"bind:to:",
                &[
                    Operand::object(id.intern_name(name).unwrap()),
                    Operand::object(o),
                ],
            )
            .as_entry()
            .unwrap()
        };
        let word_vt = vt(word);
        let original_factory = bind(word, b"fromBits:");
        let original_bits = bind(word, b"bits");
        let target_vt = keep(behavior(id));
        let target = keep(allocate(id, target_vt, 0));
        let mut h = Hook {
            id,
            roots,
            gc,
            word,
            word_vt,
            target_vt,
            original_bits,
            original_factory,
            root_to_drop: Cell::new(0),
            expected: Cell::new(nil),
            calls: Cell::new(0),
            factory_calls: Cell::new(0),
            mode: Cell::new(0),
            replace_factory: Cell::new(false),
            replace_target: Cell::new(false),
        };
        let context = (&raw mut h).cast();
        install(id, target_vt, b"optional:", optional, context);
        install(id, target_vt, b"object:", object_word, context);
        let optional_selector = keep(id.intern_name(b"optional:").unwrap());
        let object_selector = keep(id.intern_name(b"object:").unwrap());
        let optional_name = b"optionalWordResultOf:selector:argument:resultFactory:";
        let object_name = b"objectResultOf:selector:wordArgument:";
        let value = allocate(id, behavior(id), 0);
        h.expected.set(value);
        h.root_to_drop.set(retain(id, roots, value));
        let optargs = [
            Operand::object(target),
            Operand::object(optional_selector),
            Operand::object(value),
            Operand::object(word),
        ];
        h.replace_factory.set(true);
        let result = keep(object(send(id, execution, optional_name, &optargs)));
        assert_eq!(send(id, result, b"bits", &[]).as_word(), Some(37));
        assert_eq!(h.factory_calls.get(), 1);
        install(
            id,
            word_vt,
            b"fromBits:",
            original_factory.method.unwrap(),
            original_factory.context,
        );
        h.replace_factory.set(false);
        // Scope keeps the temporary value through the call; future calls need
        // their own caller-side lifetime cover, as required by the native ABI.
        keep(value);
        h.mode.set(1);
        assert_eq!(
            send(id, execution, optional_name, &optargs).as_object(),
            Some(nil)
        );
        assert_eq!(h.factory_calls.get(), 1);
        h.mode.set(2);
        assert_eq!(
            id.send_named(execution, optional_name, &optargs)
                .unwrap_err()
                .status,
            1
        );
        h.mode.set(3);
        let fail = id
            .send_named(execution, optional_name, &optargs)
            .unwrap_err();
        assert_eq!((fail.status, fail.detail), (813, 29));
        h.mode.set(4);
        let result = keep(object(send(id, execution, optional_name, &optargs)));
        assert_eq!(send(id, result, b"bits", &[]).as_word(), Some(usize::MAX));
        h.mode.set(5);
        let zero = keep(object(send(id, execution, optional_name, &optargs)));
        assert_ne!(zero, nil);
        assert_eq!(send(id, zero, b"bits", &[]).as_word(), Some(0));
        h.mode.set(0);
        let raw_word = object(send(id, word, b"fromBits:", &[Operand::word(9)]));
        h.expected.set(raw_word);
        h.root_to_drop.set(retain(id, roots, raw_word));
        let private_word_vt = object(send(
            id,
            id.receiver,
            b"ensurePrivateVtableFor:",
            &[Operand::object(raw_word)],
        ));
        install(id, private_word_vt, b"bits", bits, context);
        let objargs = [
            Operand::object(target),
            Operand::object(object_selector),
            Operand::object(raw_word),
        ];
        let result = keep(object(send(id, execution, object_name, &objargs)));
        assert_eq!(send(id, result, b"bits", &[]).as_word(), Some(71));
        keep(raw_word);
        h.mode.set(1);
        assert_eq!(
            send(id, execution, object_name, &objargs).as_object(),
            Some(nil)
        );
        h.mode.set(2);
        assert_eq!(
            id.send_named(execution, object_name, &objargs)
                .unwrap_err()
                .status,
            1
        );
        h.mode.set(3);
        let fail = id.send_named(execution, object_name, &objargs).unwrap_err();
        assert_eq!((fail.status, fail.detail), (814, 31));
        h.mode.set(0);
        for (name, args) in [
            (optional_name.as_slice(), optargs.as_slice()),
            (object_name.as_slice(), objargs.as_slice()),
        ] {
            for length in 0..args.len() {
                assert!(id.send_named(execution, name, &args[..length]).is_err());
            }
            let mut too_many = args.to_vec();
            too_many.push(Operand::object(nil));
            assert!(id.send_named(execution, name, &too_many).is_err());
            for index in 0..args.len() {
                for invalid in [Operand::word(0), Operand::object(std::ptr::null_mut())] {
                    let mut wrong = args.to_vec();
                    wrong[index] = invalid;
                    assert!(id.send_named(execution, name, &wrong).is_err());
                }
            }
        }
        let exe_vt = vt(execution);
        let original = bind(execution, optional_name);
        install(id, exe_vt, optional_name, deny, std::ptr::null_mut());
        let fail = id
            .send_named(execution, optional_name, &optargs)
            .unwrap_err();
        assert_eq!((fail.status, fail.detail), (815, 33));
        install(
            id,
            exe_vt,
            optional_name,
            original.method.unwrap(),
            original.context,
        );
        h.replace_target.set(true);
        let failed = id.send_named(execution, object_name, &objargs).unwrap_err();
        assert_eq!((failed.status, failed.detail), (815, 33));
        h.replace_target.set(false);
        install(id, target_vt, b"object:", object_word, context);
        install(id, private_word_vt, b"bits", deny, std::ptr::null_mut());
        let calls = h.calls.get();
        let failed = id.send_named(execution, object_name, &objargs).unwrap_err();
        assert_eq!((failed.status, failed.detail), (815, 33));
        assert_eq!(h.calls.get(), calls);
        install(
            id,
            private_word_vt,
            b"bits",
            h.original_bits.method.unwrap(),
            h.original_bits.context,
        );
        install(id, private_word_vt, b"bits", bits, context);
        install(id, target_vt, b"object:", deny, std::ptr::null_mut());
        let failed = id.send_named(execution, object_name, &objargs).unwrap_err();
        assert_eq!((failed.status, failed.detail), (815, 33));
        install(
            id,
            private_word_vt,
            b"bits",
            h.original_bits.method.unwrap(),
            h.original_bits.context,
        );
        for token in tokens {
            release(id, roots, token);
        }
    }
}
