//! Real String/Error ownership, replacement and collection through Oop messages.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use std::{path::Path, ptr};

#[test]
#[ignore = "requires production Oop modules"]
fn owned_text_builders_and_errors_use_live_messages_and_gc() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let named = |name: &[u8]| h.named(name).unwrap();
    let key = |name: &[u8]| hooks.symbol(name);
    let call = |r, name: &[u8], args: &[Oop]| q.call(r, name, args).unwrap();
    let text = |v| q.read_bytes(call(v, b"bytes", &[])).unwrap();
    let string = named(b"String");
    let sb = named(b"StringBuilder");
    let error = named(b"Error");
    let svt = named(b"StringVt");
    let evt = named(b"ErrorFactoryVt");
    let gc = named(b"GC");
    let integer = named(b"Integer");
    let truth = named(b"true");
    let falsity = named(b"false");
    let nil = named(b"nil");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    assert_eq!(
        h.load_native(
            &q,
            &h.resources.lib_dir.join("libbirnam_string.dylib"),
            &[h.namespace]
        )
        .unwrap(),
        string
    );
    let path = q
        .string(
            h.resources
                .lib_dir
                .join("libbirnam_error.dylib")
                .as_os_str()
                .as_encoded_bytes(),
        )
        .unwrap();
    assert_eq!(call(named(b"Codegen"), b"load:", &[path]), error);
    for (v, expected) in [
        (truth, b"true".as_slice()),
        (falsity, b"false"),
        (nil, b"nil"),
    ] {
        assert_eq!(text(call(v, b"printString", &[])), expected);
    }
    let raw = [0, 0xff, b'a', b' ', b'\t'];
    let value = q.string(&raw).unwrap();
    let zero = call(integer, b"fromWord:", &[size(0).unwrap()]);
    let one = call(integer, b"fromWord:", &[size(1).unwrap()]);
    let two = call(integer, b"fromWord:", &[size(2).unwrap()]);
    let negative = call(integer, b"fromWord:", &[q.word(usize::MAX).unwrap()]);
    assert_eq!(text(call(value, b"at:", &[one])), &[0xff]);
    let byte = call(value, b"byteAt:", &[one]);
    assert_eq!(q.word_bits(call(integer, b"wordOf:", &[byte])).unwrap(), 255);
    assert_eq!(text(call(value, b"copyFrom:to:", &[zero, two])), &[0, 0xff]);
    assert_eq!(text(call(value, b"copyFrom:to:", &[one, one])), b"");
    for index in [negative, call(integer, b"fromWord:", &[size(raw.len()).unwrap()])] {
        assert_eq!(call(value, b"at:", &[index]), nil);
        assert_eq!(call(value, b"byteAt:", &[index]), nil);
    }
    assert_eq!(call(value, b"copyFrom:to:", &[two, one]), nil);
    assert_eq!(text(value), raw);
    assert_eq!(as_size(call(value, b"length", &[])), Some(raw.len()));
    assert_eq!(call(value, b"printString", &[]), value);
    let clone = call(value, b"clone", &[]);
    assert_ne!(clone, value);
    assert_eq!(text(clone), raw);
    let a = call(value, b"bytes", &[]);
    let b = call(clone, b"bytes", &[]);
    assert_ne!(a, b);
    call(a, b"appendByteValues", &[size(b'!' as usize).unwrap()]);
    assert_eq!(q.read_bytes(b).unwrap(), raw);
    assert_eq!(text(value), raw);
    assert_eq!(call(value, b"=", &[clone]), truth);
    assert_eq!(call(value, b"=", &[value]), truth);
    assert_eq!(call(value, b"=", &[q.string(b"different").unwrap()]), falsity);
    assert_eq!(call(string, b"=", &[nil]), falsity);
    // String owns native bytes behind a zero-payload handle. No foreign payload writes.
    assert_eq!(
        as_size(call(h.id.root, b"allocationSizeOf:", &[value])),
        Some(0)
    );
    assert!(
        h.send(
            named(b"ObjectModel"),
            b"slot:in:",
            &[size(0).unwrap(), value]
        )
        .is_err()
    );
    call(h.id.root, b"ensurePrivateVtableFor:", &[value]);
    call(gc, b"collectPrecisely", &[]);
    assert_eq!(text(value), raw);
    assert_eq!(text(call(value, b"new", &[])), raw);
    let trimmed = q.string(b" \t\r\n\x0b\x0c hello \x0c").unwrap();
    assert_eq!(text(call(trimmed, b"trimmed", &[])), b"hello");
    let needle = q.string(b"aa").unwrap();
    let repl = q.string(b"b").unwrap();
    let source = q.string(b"aaaaa").unwrap();
    assert_eq!(
        text(call(source, b"replace:with:", &[needle, repl])),
        b"bba"
    );
    assert_eq!(
        text(call(source, b"replace:with:", &[string, repl])),
        b"aaaaa"
    );
    assert_eq!(text(call(source, b",", &[truth])), b"aaaaatrue");
    assert_eq!(
        call(q.string(b"opaque\0tail").unwrap(), b"asSymbol", &[]),
        key(b"opaque")
    );
    for name in [b"size".as_slice(), b"at:", b"asInteger", b"asNumber"] {
        let method = call(svt, b"lookup", &[key(name)]);
        assert!(!method.is_null());
        assert_eq!(
            call(h.id.root, b"behaviorOf:", &[method]),
            call(h.id.root, b"methodBehavior", &[])
        );
    }
    let builder = call(sb, b"new", &[]);
    assert_eq!(call(value, b"=", &[builder]), falsity);
    let equality = call(svt, b"lookup", &[key(b"=")]);
    let self_argument = call(h.id.root, b"sequence", &[builder]);
    assert!(h.send(equality, b"invokeOn:arguments:", &[builder, self_argument]).is_err(),
            "identity alone cannot bypass String's receiver representation check");
    let byte_reader = call(svt, b"lookup", &[key(b"byteAt:")]);
    let args = call(h.id.root, b"sequence", &[negative]);
    assert!(h.send(byte_reader, b"invokeOn:arguments:", &[builder, args]).is_err(),
            "the receiver representation is checked before rejecting the index");
    call(builder, b"add:", &[source]);
    call(builder, b"add:", &[truth]);
    assert_eq!(text(call(builder, b"asString", &[])), b"aaaaatrue");
    call(builder, b"add:", &[builder]);
    assert_eq!(
        text(call(builder, b"printString", &[])),
        b"aaaaatrueaaaaatrue"
    );
    let empty = call(builder, b"clone", &[]);
    assert_eq!(text(call(empty, b"asString", &[])), b"");
    call(h.id.root, b"ensurePrivateVtableFor:", &[builder]);
    call(gc, b"collectPrecisely", &[]);
    call(builder, b"add:", &[nil]);
    assert!(text(call(builder, b"asString", &[])).ends_with(b"nil"));

    let path = std::env::temp_dir().join(format!("birnam-text-error-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/text-error.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let module = h.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let method = |name: &[u8], context| {
        call(
            call(module, b"methodAt:", &[key(name)]),
            b"withContext:",
            &[context],
        )
    };
    let install = |vt, name: &[u8], m| {
        call(vt, b"addMethod", &[key(name), m]);
    };
    let selected = |vt, name: &[u8]| call(vt, b"lookup", &[key(name)]);

    // Collection keys retain live vtable queries and identity semantics for
    // objects with changed behavior. The byte hash itself stays unchanged.
    let key_left = q.string(&raw).unwrap();
    let key_right = q.string(&raw).unwrap();
    let expected_hash = raw.iter().fold(2_166_136_261u32,
        |hash, &byte| (hash ^ u32::from(byte)).wrapping_mul(16_777_619));
    let root_vt = call(h.id.root, b"behaviorOf:", &[h.id.root]);
    let original_behavior = selected(root_vt, b"behaviorOf:");
    let behavior_alias = key(b"fixtureBehaviorOf:");
    call(root_vt, b"addMethod", &[behavior_alias, original_behavior]);
    let queries = hooks.context(&[behavior_alias, size(0).unwrap()]);
    install(root_vt, b"behaviorOf:", method(b"counted", queries));
    assert_eq!(as_size(h.send(key_left, b"collectionHash", &[]).unwrap()), Some(expected_hash as usize));
    assert_eq!(h.send(key_left, b"collectionEquals:", &[key_right]).unwrap(), truth);
    assert_eq!(hooks.count(queries, 1), 3);
    assert_eq!(h.send(key_left, b"collectionEquals:", &[key_left]).unwrap(), truth);
    assert_eq!(hooks.count(queries, 1), 3, "self-equality needs no vtable query");
    install(root_vt, b"behaviorOf:", original_behavior);
    call(root_vt, b"removeMethod:", &[behavior_alias]);
    call(h.id.root, b"ensurePrivateVtableFor:", &[key_right]);
    assert_eq!(call(key_left, b"collectionEquals:", &[key_right]), falsity);
    assert_eq!(call(key_right, b"collectionEquals:", &[key_right]), truth);
    let key_equality = selected(svt, b"collectionEquals:");
    assert_eq!(call(key_equality, b"invokeOn:arguments:", &[builder, self_argument]), truth,
               "collection identity differs from ordinary String content equality");

    // Printing and text operations must observe the current String factory.
    let original = selected(svt, b"fromBytes:");
    let alias = key(b"fixtureFromBytes:");
    call(svt, b"addMethod", &[alias, original]);
    let counter = hooks.context(&[alias, size(0).unwrap()]);
    install(svt, b"fromBytes:", method(b"counted", counter));
    call(truth, b"printString", &[]);
    call(nil, b"printString", &[]);
    call(source, b"trimmed", &[]);
    call(builder, b"asString", &[]);
    assert_eq!(text(call(value, b"at:", &[one])), &[0xff]);
    assert_eq!(text(call(value, b"copyFrom:to:", &[zero, two])), &[0, 0xff]);
    assert_eq!(hooks.count(counter, 1), 6);
    install(svt, b"fromBytes:", original);
    call(svt, b"removeMethod:", &[alias]);

    // Numeric conversion and result construction may both collect. Neither
    // callback may run while String's private buffer table is borrowed.
    let ivt = named(b"IntegerVt");
    let word_of = selected(ivt, b"wordOf:");
    let indices = hooks.context(&[word_of, gc, size(0).unwrap()]);
    let results = hooks.context(&[original, gc, size(0).unwrap()]);
    install(ivt, b"wordOf:", hooks.method(b"collectEvery", indices));
    install(svt, b"fromBytes:", hooks.method(b"collectEvery", results));
    assert_eq!(text(call(value, b"at:", &[one])), &[0xff]);
    assert_eq!(text(call(value, b"copyFrom:to:", &[zero, two])), &[0, 0xff]);
    assert!(hooks.count(indices, 2) >= 3);
    assert_eq!(hooks.count(results, 2), 2);
    install(ivt, b"wordOf:", word_of);
    install(svt, b"fromBytes:", original);

    // Snapshot the byte provider before the String allocation callback mutates it.
    let meta = call(h.id.root, b"behaviorOf:", &[svt]);
    let allocation = selected(meta, b"allocate");
    let allocate_alias = key(b"fixtureAllocate");
    call(meta, b"addMethod", &[allocate_alias, allocation]);
    let input = q.byte_values(b"abc").unwrap();
    let alloc = hooks.context(&[gc, allocate_alias, input, size(0).unwrap(), svt]);
    install(meta, b"allocate", method(b"allocating", alloc));
    let copied = call(string, b"fromBytes:", &[input]);
    assert_eq!(q.read_bytes(input).unwrap(), b"zbc");
    assert_eq!(text(copied), b"abc");
    assert_eq!(text(call(copied, b"clone", &[])), b"abc");
    // Each String has one Id allocation; its byte vector belongs to String.
    assert_eq!(hooks.count(alloc, 3), 2);
    install(meta, b"allocate", allocation);
    call(meta, b"removeMethod:", &[allocate_alias]);

    let custom = call(named(b"Object"), b"clone", &[]);
    let custom_behavior = call(h.id.root, b"ensurePrivateVtableFor:", &[custom]);
    let response = q.string(b"callback").unwrap();
    install(
        custom_behavior,
        b"printString",
        method(b"replacement", hooks.context(&[gc, response])),
    );
    assert_eq!(text(call(source, b",", &[custom])), b"aaaaacallback");
    call(builder, b"add:", &[custom]);
    assert!(text(call(builder, b"asString", &[])).ends_with(b"callback"));

    let observations = h.scope();
    let probe = observations
        .keep(call(named(b"Object"), b"clone", &[]))
        .unwrap();
    let probe_vt = call(h.id.root, b"ensurePrivateVtableFor:", &[probe]);
    for name in [
        b"watch".as_slice(),
        b"remaining",
        b"allocation:bytes:identity:",
        b"finalized",
        b"violations",
    ] {
        install(probe_vt, name, method(name, probe));
    }
    let watch = key(b"watch");
    // Keep the Error, but no separate root for its message or getter results.
    {
        let errors = h.scope();
        let kind = key(b"problem");
        let provider = q.byte_values(b"retained by error").unwrap();
        let message = h.send(string, b"fromBytes:", &[provider]).unwrap();
        let fault = errors
            .call(error, b"kind:message:", &[kind, message])
            .unwrap();
        unsafe {
            h.id.send(probe, watch, &[message]).unwrap();
        }
        call(gc, b"collectPrecisely", &[]);
        assert_eq!(as_size(call(probe, b"remaining", &[])), Some(1));
        let check = |r, name: &[u8], args: &[Oop]| errors.call(r, name, args).unwrap();
        assert_eq!(check(fault, b"kind", &[]), kind);
        assert_eq!(check(fault, b"message", &[]), message);
        assert_eq!(check(fault, b"isError", &[]), truth);
        assert_eq!(
            check(fault, b"unknownImmediate:", &[size(7).unwrap()]),
            fault
        );
        assert_eq!(call(source, b"isError", &[]), falsity);
        assert_eq!(check(fault, b"unknownMessage:", &[source]), fault);
        let copied_error = check(fault, b"clone", &[]);
        assert_ne!(copied_error, fault);
        call(gc, b"collectPrecisely", &[]);
        assert_eq!(check(copied_error, b"message", &[]), message);
        assert_eq!(check(check(error, b"kind:", &[kind]), b"message", &[]), nil);
        assert_eq!(
            check(check(error, b"message:", &[message]), b"kind", &[]),
            key(b"error")
        );
        assert_eq!(
            check(check(error, b"kind:", &[ptr::null_mut()]), b"kind", &[]),
            nil
        );
        for (name, args, expected) in [
            (
                b"missingTextMethod".as_slice(),
                vec![],
                b"does not understand #missingTextMethod".as_slice(),
            ),
            (
                b"missingImmediate:",
                vec![size(9).unwrap()],
                b"does not understand #missingImmediate:",
            ),
        ] {
            let dnu = check(source, name, &args);
            assert_eq!(check(dnu, b"kind", &[]), key(b"doesNotUnderstand"));
            assert_eq!(
                errors.string_bytes(check(dnu, b"message", &[])).unwrap(),
                expected
            );
        }
        let factory = selected(evt, b"kind:message:");
        // Context and callback are rooted only until the factory is restored.
        let context = errors.call(h.id.root, b"sequence", &[gc, fault]).unwrap();
        let declared = call(module, b"methodAt:", &[key(b"replacement")]);
        let replacement = errors.call(declared, b"withContext:", &[context]).unwrap();
        h.send(evt, b"addMethod", &[key(b"kind:message:"), replacement])
            .unwrap();
        assert_eq!(check(source, b"anotherMissingMethod", &[]), fault);
        install(evt, b"kind:message:", factory);
    }
    call(gc, b"collectPrecisely", &[]);
    assert_eq!(as_size(call(probe, b"remaining", &[])), Some(0));

    // String and builder handles are weak keys in String's native byte storage.
    // Check exact allocation identities, not unrelated temporary Name allocations.
    let fin = selected(svt, b"finalize:");
    let fin_alias = key(b"fixtureFinalize:");
    call(svt, b"addMethod", &[fin_alias, fin]);
    let provider = q.byte_values(b"temporary").unwrap();
    let finalizer = method(
        b"finalize",
        hooks.context(&[gc, string, provider, fin_alias]),
    );
    let from = key(b"fromBytes:");
    let new = key(b"new");
    call(gc, b"collectPrecisely", &[]);
    install(svt, b"finalize:", finalizer);
    for _ in 0..32 {
        unsafe {
            let doomed = h.id.send(string, from, &[provider]).unwrap();
            let doomed_builder = h.id.send(sb, new, &[]).unwrap();
            h.id.send(probe, watch, &[doomed, doomed_builder]).unwrap();
        }
        assert_eq!(as_size(call(probe, b"remaining", &[])), Some(2));
        call(gc, b"collectPrecisely", &[]);
        assert_eq!(as_size(call(probe, b"remaining", &[])), Some(0));
    }
    assert_eq!(as_size(call(probe, b"finalized", &[])), Some(64));
    assert_eq!(as_size(call(probe, b"violations", &[])), Some(0));
    install(svt, b"finalize:", fin);
    call(svt, b"removeMethod:", &[fin_alias]);

    call(gc, b"budget:", &[size(1).unwrap()]);
    for _ in 0..8 {
        let s = q.string(b"budget").unwrap();
        let e = call(error, b"message:", &[s]);
        assert_eq!(call(e, b"message", &[]), s);
        call(builder, b"add:", &[s]);
    }
    call(gc, b"budget:", &[size(1024 * 1024).unwrap()]);
    unsafe {
        h.id.send(probe, watch, &[string, error, sb]).unwrap();
    }
    call(gc, b"collectPrecisely", &[]);
    assert_eq!(as_size(call(probe, b"remaining", &[])), Some(3));
    // Copying a String Method to an unrelated object cannot expose native bytes.
    let clone = selected(svt, b"clone");
    install(custom_behavior, b"badStringClone", clone);
    assert!(h.send(custom, b"badStringClone", &[]).is_err());
    drop(hooks);
    drop(q);
    h.send(gc, b"collectPrecisely", &[]).unwrap();
    assert_eq!(
        as_size(observations.call(probe, b"remaining", &[]).unwrap()),
        Some(3)
    );
}
