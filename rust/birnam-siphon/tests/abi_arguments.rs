//! Literal ABI vectors are omitted only while their owners permit it.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use std::{cell::Cell, ffi::c_void, os::unix::ffi::OsStrExt};
use support::*;
struct Hook {
    original: NativeEntry,
    calls: Cell<usize>,
    family: Option<Oop>,
}
unsafe extern "C" fn hook(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let h = unsafe { &*c.cast::<Hook>() };
    if h.family.is_none() || unsafe { borrowed_slice(a, n) }.unwrap()[0].as_object() == h.family {
        h.calls.set(h.calls.get() + 1);
    }
    unsafe { (h.original.method.unwrap())(h.original.context, r, a, n) }
}
unsafe extern "C" fn many(_: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    let args = unsafe { borrowed_slice(a, n) }.unwrap();
    assert_eq!(n, 9);
    assert!(args.iter().all(|a| a.as_object() == args[0].as_object()));
    Outcome::loaded(args[0])
}
unsafe extern "C" fn deny_lookup(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    let calls = unsafe { &*c.cast::<Cell<usize>>() };
    calls.set(calls.get()+1);
    Outcome::loaded(Operand::word(0))
}
struct Permission {
    entry: NativeEntry,
    allow: Cell<bool>,
}
unsafe extern "C" fn permission(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let p = unsafe { &*c.cast::<Permission>() };
    if p.allow.get() {
        unsafe { (p.entry.method.unwrap())(p.entry.context, r, a, n) }
    } else {
        Outcome::loaded(Operand::word(0))
    }
}
struct SignatureHook {
    id: Protocol,
    entry: NativeEntry,
    vector_behavior: Oop,
    add: *mut Hook,
    calls: Cell<usize>,
}
unsafe extern "C" fn signature_hook(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    let s = unsafe { &*c.cast::<SignatureHook>() };
    s.calls.set(s.calls.get() + 1);
    unsafe {
        install(s.id, s.vector_behavior, b"add:", hook, s.add.cast());
    }
    unsafe { (s.entry.method.unwrap())(s.entry.context, r, a, n) }
}
struct Census {
    id: Protocol,
    family: Oop,
    count: usize,
}
unsafe extern "C" fn census(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    let s = unsafe { &mut *c.cast::<Census>() };
    if unsafe {
        send(
            s.id,
            s.id.receiver,
            b"behaviorOf:",
            &[borrowed_slice(a, n).unwrap()[0]],
        )
    }
    .as_object()
        == Some(s.family)
    {
        s.count += 1;
    }
    Outcome::loaded(Operand::unit())
}
unsafe fn count(id: Protocol, family: Oop) -> usize {
    let mut s = Census {
        id,
        family,
        count: 0,
    };
    unsafe {
        inventory(id, census, (&raw mut s).cast());
    }
    s.count
}
#[test]
#[ignore = "production compiler, Execution and Vector owners required"]
fn literal_abi_arguments_preserve_replacements_and_allocation_hooks() {
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
        let source=br#"let Probe := [Object clone]; [Reflect defineMethod: #convert: on: Probe as: [Execution block: [:self :value | let bridge := Execution; let target := Integer; let signature := "Wo"; let factory := Word; [bridge sendNativeTo: target selector: #wordOf: arguments: #[value] signature: signature resultFactory: factory]] withNativeSignature: "oO"]]; Probe"#;
        let text = object(send(
            id,
            named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(source)],
        ));
        let tt = retain(id, roots, text);
        let probe = object(send(
            id,
            named(b"Codegen"),
            b"eval:",
            &[Operand::object(text)],
        ));
        let pt = retain(id, roots, probe);
        release(id, roots, tt);
        assert_eq!(object(send(id, probe, b"isError", &[])), named(b"false"));
        let value = object(send(
            id,
            named(b"Integer"),
            b"fromWord:",
            &[Operand::word(123)],
        ));
        let vt = retain(id, roots, value);
        let run = || {
            let result = object(send(id, probe, b"convert:", &[Operand::object(value)]));
            assert_eq!(send(id, result, b"bits", &[]).as_word(), Some(123));
        };
        run();
        collect(id, gc);
        send(id, gc, b"budget:", &[Operand::word(usize::MAX)]);
        let family = named(b"VectorVt");
        let before = count(id, family);
        run();
        assert_eq!(count(id, family) - before, 0);
        for (receiver, selector, behavior, expected) in [
            (named(b"Vector"), b"new".as_slice(), family, 1),
            (named(b"Vector"), b"add:".as_slice(), family, 1),
            (named(b"Vector"), b"sizeWord".as_slice(), family, 1),
            (named(b"Vector"), b"atWord:".as_slice(), family, 1),
            (
                named(b"Execution"),
                b"sendNativeTo:selector:arguments:signature:resultFactory:".as_slice(),
                object(send(
                    id,
                    id.receiver,
                    b"behaviorOf:",
                    &[Operand::object(named(b"Execution"))],
                )),
                1,
            ),
        ] {
            let original = send(
                id,
                id.receiver,
                b"bind:to:",
                &[
                    Operand::object(id.intern_name(selector).unwrap()),
                    Operand::object(receiver),
                ],
            )
            .as_entry()
            .unwrap();
            let mut h = Hook {
                original,
                calls: Cell::new(0),
                family: None,
            };
            install(id, behavior, selector, hook, (&raw mut h).cast());
            let before = count(id, family);
            run();
            assert_eq!(
                h.calls.get(),
                expected,
                "{} replacement must execute once",
                String::from_utf8_lossy(selector)
            );
            assert_eq!(count(id, family) - before, 2);
            install(
                id,
                behavior,
                selector,
                original.method.unwrap(),
                original.context,
            );
        }
        // Signature inspection is not an effect-free guard when replaced.
        // It runs once, after the original Vector has already been constructed.
        let add = send(
            id,
            id.receiver,
            b"bind:to:",
            &[
                Operand::object(id.intern_name(b"add:").unwrap()),
                Operand::object(named(b"Vector")),
            ],
        )
        .as_entry()
        .unwrap();
        let bytes = send(
            id,
            id.receiver,
            b"bind:to:",
            &[
                Operand::object(id.intern_name(b"bytes").unwrap()),
                Operand::object(named(b"String")),
            ],
        )
        .as_entry()
        .unwrap();
        let mut add_hook = Hook {
            original: add,
            calls: Cell::new(0),
            family: None,
        };
        let mut sig = SignatureHook {
            id,
            entry: bytes,
            vector_behavior: family,
            add: &raw mut add_hook,
            calls: Cell::new(0),
        };
        install(
            id,
            named(b"StringVt"),
            b"bytes",
            signature_hook,
            (&raw mut sig).cast(),
        );
        run();
        assert_eq!(sig.calls.get(), 1);
        assert_eq!(
            add_hook.calls.get(),
            0,
            "Vector construction must precede the mutating signature accessor"
        );
        install(
            id,
            named(b"StringVt"),
            b"bytes",
            bytes.method.unwrap(),
            bytes.context,
        );
        install(id, family, b"add:", add.method.unwrap(), add.context);
        // The underlying Bytes accessor can be replaced independently too.
        let bytes_receiver = named(b"Bytes");
        let bytes_behavior = named(b"BytesVt");
        let bytes = send(
            id,
            id.receiver,
            b"bind:to:",
            &[
                Operand::object(id.intern_name(b"bytes").unwrap()),
                Operand::object(bytes_receiver),
            ],
        )
        .as_entry()
        .unwrap();
        let mut bytes_hook = Hook {
            original: bytes,
            calls: Cell::new(0),
            family: None,
        };
        install(
            id,
            bytes_behavior,
            b"bytes",
            hook,
            (&raw mut bytes_hook).cast(),
        );
        run();
        assert_eq!(bytes_hook.calls.get(), 1);
        install(
            id,
            bytes_behavior,
            b"bytes",
            bytes.method.unwrap(),
            bytes.context,
        );
        let model = named(b"ObjectModel");
        let behavior = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(model)],
        ));
        let selector = b"allocate:bytes:";
        let original = send(
            id,
            id.receiver,
            b"bind:to:",
            &[
                Operand::object(id.intern_name(selector).unwrap()),
                Operand::object(model),
            ],
        )
        .as_entry()
        .unwrap();
        let mut h = Hook {
            original,
            calls: Cell::new(0),
            family: Some(family),
        };
        install(id, behavior, selector, hook, (&raw mut h).cast());
        run();
        assert_eq!(h.calls.get(), 2);
        install(
            id,
            behavior,
            selector,
            original.method.unwrap(),
            original.context,
        );
        // A permission provider may change its verdict without changing Id's
        // method epoch. The cached Vector meanings must not cache that verdict.
        let name = b"canOmitAllocationFor:";
        let original = send(
            id,
            id.receiver,
            b"bind:to:",
            &[
                Operand::object(id.intern_name(name).unwrap()),
                Operand::object(model),
            ],
        )
        .as_entry()
        .unwrap();
        let mut p = Permission {
            entry: original,
            allow: Cell::new(true),
        };
        install(id, behavior, name, permission, (&raw mut p).cast());
        let before = count(id, family);
        run();
        assert_eq!(count(id, family) - before, 0);
        let vector = named(b"Vector");
        assert_eq!(
            send(id, vector, b"canOmitNativeArguments", &[]).as_word(),
            Some(1)
        );
        let epoch = send(id, id.receiver, b"behaviorEpoch", &[])
            .as_word()
            .unwrap();
        p.allow.set(false);
        assert_eq!(
            send(id, vector, b"canOmitNativeArguments", &[]).as_word(),
            Some(0)
        );
        assert_eq!(
            send(id, id.receiver, b"behaviorEpoch", &[]).as_word(),
            Some(epoch)
        );
        let before = count(id, family);
        run();
        assert_eq!(count(id, family) - before, 2);
        install(
            id,
            behavior,
            name,
            original.method.unwrap(),
            original.context,
        );
        // A replaced guard service is not a pure dictionary fact. It must run
        // through the original path, and may deny admission without mutation.
        let uses = id.intern_name(b"usesDefaultLookup:").unwrap();
        let binding = id.intern_name(b"bind:to:").unwrap();
        let query = id.send(id.receiver, binding, &[
            Operand::object(uses), Operand::object(id.receiver),
        ]).unwrap().as_entry().unwrap();
        let declaration = |entry, role: &[u8]| send(id, id.receiver,
            b"nativeBindingQuerySemantics:for:", &[Operand::entry(entry), Operand::bytes(role)]).as_word();
        assert_eq!(declaration(query, b"usesDefaultLookup:"), Some(1));
        assert_eq!(declaration(query, b"bind:to:"), Some(0), "query roles are distinct");
        assert_eq!(declaration(NativeEntry::new(query.method.unwrap(), std::ptr::null_mut()),
            b"usesDefaultLookup:"), Some(0), "a foreign context is not canonical");
        let mut row = [id.receiver.addr(), uses.addr(), query.method.unwrap() as usize, query.context.addr()];
        let matches = |row: &[usize; 4]| {
            let bytes = std::slice::from_raw_parts(row.as_ptr().cast::<u8>(), std::mem::size_of_val(row));
            send(id, id.receiver, b"nativeBindingsMatch:", &[Operand::bytes(bytes)]).as_word()
        };
        assert_eq!(matches(&row), Some(1));
        row[3] = 0;
        assert_eq!(matches(&row), Some(0), "same method with another context must decline");
        let calls = Cell::new(0usize);
        assert_eq!(declaration(NativeEntry::new(deny_lookup, (&raw const calls).cast_mut().cast()),
            b"usesDefaultLookup:"), Some(0), "an override present at startup cannot be admitted");
        install(id, id.receiver, b"usesDefaultLookup:", deny_lookup, (&raw const calls).cast_mut().cast());
        let before = count(id, family);
        run();
        assert_eq!(calls.get(), 1, "the current guard executes exactly once");
        assert_eq!(count(id, family)-before, 2, "denial restores literal Vector construction");
        install(id, id.receiver, b"usesDefaultLookup:", query.method.unwrap(), query.context);

        // Changed interning remains observable. Compare the complete forwarding
        // callback count on the ordinary and declined-batch paths.
        let intern = id.send(id.receiver, binding, &[
            Operand::object(id.intern), Operand::object(id.receiver),
        ]).unwrap().as_entry().unwrap();
        assert_eq!(send(id, gc, b"internQuerySemantics:", &[Operand::entry(intern)]).as_word(), Some(1));
        assert_eq!(send(id, gc, b"internQuerySemantics:", &[Operand::entry(query)]).as_word(), Some(0));
        let mut interning = Hook { original: intern, calls: Cell::new(0), family: None };
        let batches = id.intern_name(b"bindingProofBatches:").unwrap();
        let execution = named(b"Execution");
        install(id, id.receiver, b"intern:", hook, (&raw mut interning).cast());
        id.send(execution, batches, &[Operand::word(0)]).unwrap();
        interning.calls.set(0);
        run();
        let ordinary_interns = interning.calls.get();
        id.send(execution, batches, &[Operand::word(1)]).unwrap();
        interning.calls.set(0);
        run();
        assert_eq!(interning.calls.get(), ordinary_interns);
        assert!(ordinary_interns > 0);
        install(id, id.receiver, b"intern:", intern.method.unwrap(), intern.context);
        send(id, gc, b"budget:", &[Operand::word(1)]);
        run();
        assert!(present(id, value));
        // Larger native calls retain the ordinary dynamic operand buffer.
        let probe_behavior = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(probe)],
        ));
        install(id, probe_behavior, b"many:", many, std::ptr::null_mut());
        let mut arguments = object(send(id, named(b"Vector"), b"new", &[]));
        let mut old_token = retain(id, roots, arguments);
        for _ in 0..9 {
            arguments = object(send(id, arguments, b"add:", &[Operand::object(value)]));
            let token = retain(id, roots, arguments);
            release(id, roots, old_token);
            old_token = token;
        }
        let signature = object(send(
            id,
            named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(b"oooooooooo")],
        ));
        let st = retain(id, roots, signature);
        let result = send(
            id,
            named(b"Execution"),
            b"sendNativeTo:selector:arguments:signature:resultFactory:",
            &[
                Operand::object(probe),
                Operand::object(id.intern_name(b"many:").unwrap()),
                Operand::object(arguments),
                Operand::object(signature),
                Operand::object(named(b"Word")),
            ],
        );
        assert_eq!(result.as_object(), Some(value));
        release(id, roots, st);
        release(id, roots, old_token);
        for token in [vt, pt] {
            release(id, roots, token);
        }
    }
}
