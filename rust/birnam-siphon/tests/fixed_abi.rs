//! Explicit fixed native adapters preserve live conversion and callback lifetime.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
#[test]
#[ignore = "requires production Oop Execution and numeric owners"]
fn fixed_adapters_preserve_current_targets_factories_and_callback_lifetime() {
    let host = oop::bootstrap();
    let q = host.scope();
    let named = |n: &[u8]| host.named(n).unwrap();
    let key = |n: &[u8]| q.keep(host.intern(n).unwrap()).unwrap();
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let word = named(b"Word");
    let execution = named(b"Execution");
    let gc = named(b"GC");
    let nil = named(b"nil");
    let roots = host.roots;
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let word_vt = named(b"WordVt");
    let target_vt = call(named(b"ObjectVt"), b"delegated", &[]);
    let target = call(target_vt, b"allocate", &[size(0).unwrap()]);
    let original_factory = call(host.id.root, b"bind:to:", &[key(b"fromBits:"), word]);
    let original_bits = call(host.id.root, b"bind:to:", &[key(b"bits"), word]);
    let factory_alias = key(b"fixedOriginalFactory:");
    call(word_vt, b"addMethod", &[factory_alias, original_factory]);
    let full = q.word(usize::MAX).unwrap();
    let invalid_word = call(named(b"Object"), b"clone", &[]);
    let fault = |status, detail| {
        let f = unsafe {
            host.id.send(
                execution,
                key(b"failNative:detail:"),
                &[size(status).unwrap(), size(detail).unwrap()],
            )
        }
        .unwrap_err();
        q.keep(f).unwrap()
    };
    let optional_failure = fault(813, 29);
    let object_failure = fault(814, 31);
    let denied = fault(815, 33);
    let path = std::env::temp_dir().join(format!(
        "birnam-fixed-adapters-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/fixed-adapters.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = host.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let monitor = call(named(b"Object"), b"clone", &[]);
    let monitor_vt = call(host.id.root, b"ensurePrivateVtableFor:", &[monitor]);
    let z = size(0).unwrap();
    let one = size(1).unwrap();
    let null = std::ptr::null_mut();
    let context = call(
        host.id.root,
        b"sequence",
        &[
            gc,
            roots,
            word,
            word_vt,
            target_vt,
            z,
            null,
            z,
            z,
            z,
            z,
            z,
            null,
            null,
            full,
            nil,
            invalid_word,
            optional_failure,
            object_failure,
            denied,
            monitor,
            z,
        ],
    );
    let method = |n: &[u8]| {
        call(
            call(fixture, b"methodAt:", &[key(n)]),
            b"withContext:",
            &[context],
        )
    };
    let optional = method(b"optional:");
    let object = method(b"object:");
    let bits = method(b"bits");
    let deny = method(b"deny");
    let factory = method(b"factory");
    call(
        monitor_vt,
        b"addMethod",
        &[
            key(b"allocation:bytes:identity:"),
            method(b"allocation:bytes:identity:"),
        ],
    );
    let at = key(b"at:");
    let put = key(b"at:put:");
    let add = key(b"addMethod");
    let retain = key(b"retain:");
    let identity = key(b"identityOf:");
    let sequence = key(b"sequence");
    let allocate = key(b"allocate");
    let ensure = key(b"ensurePrivateVtableFor:");
    let optional_selector = key(b"optional:");
    let object_selector = key(b"object:");
    let bits_selector = key(b"bits");
    let from_bits = key(b"fromBits:");
    let optional_name = b"optionalWordResultOf:selector:argument:resultFactory:";
    let object_name = b"objectResultOf:selector:wordArgument:";
    let optional_adapter = key(optional_name);
    let object_adapter = key(object_name);
    let raw = |r, selector, args: &[Oop]| unsafe { host.id.send(r, selector, args) };
    let set = |vt, n, m| {
        raw(vt, add, &[n, m]).unwrap();
    };
    let state = |i, v| {
        raw(context, put, &[size(i).unwrap(), v]).unwrap();
    };
    let count = |i| as_size(raw(context, at, &[size(i).unwrap()]).unwrap()).unwrap();
    let protect_pair = |result, input| {
        let pair = raw(host.id.root, sequence, &[result, input]).unwrap();
        q.keep(pair).unwrap();
        result
    };
    state(12, factory);
    state(13, deny);
    // Every fixed form rejects absent required arguments before any dependency.
    // Valid argument counts deliberately enter a distinct failing Method, making
    // premature getter/target/factory sends observable in the null cases.
    let guard = call(named(b"Object"), b"clone", &[]);
    let guard_vt = call(host.id.root, b"ensurePrivateVtableFor:", &[guard]);
    let guard_selector = key(b"fixedGuardTarget");
    for selector in [
        guard_selector,
        bits_selector,
        key(b"bytes"),
        from_bits,
        key(b"fromBytes:"),
    ] {
        set(guard_vt, selector, deny);
    }
    for (name, width) in [
        (optional_name.as_slice(), 4),
        (object_name.as_slice(), 3),
        (b"discardResultOf:selector:wordArgument:", 3),
        (b"objectResultOf:selector:byteArgument:", 3),
        (b"bytesResultOf:selector:resultFactory:", 3),
        (b"wordResultOf:selector:", 2),
        (b"bytesResultOf:selector:", 2),
    ] {
        let adapter = key(name);
        let mut valid = vec![guard; width];
        valid[1] = guard_selector;
        assert_eq!(raw(execution, adapter, &valid).unwrap_err(), denied);
        for index in 0..width {
            let mut wrong = valid.clone();
            wrong[index] = null;
            assert_eq!(
                raw(execution, adapter, &wrong).unwrap_err(),
                birnam_abi::object_reference(host.id.invalid),
                "{} null argument {index} must fail before dependency sends",
                String::from_utf8_lossy(name)
            );
        }
    }
    set(target_vt, optional_selector, optional);
    set(target_vt, object_selector, object);
    let value = raw(named(b"ObjectVt"), allocate, &[z]).unwrap();
    let token = raw(roots, retain, &[value]).unwrap();
    let value_id = raw(host.id.root, identity, &[value]).unwrap();
    state(5, value_id);
    state(6, token);
    let optargs = [target, optional_selector, value, word];
    state(10, one);
    let result = protect_pair(raw(execution, optional_adapter, &optargs).unwrap(), value);
    assert_eq!(
        q.word_bits(raw(result, bits_selector, &[]).unwrap())
            .unwrap(),
        37
    );
    assert_eq!(
        count(8),
        1,
        "factory replacement during the target call must be observed"
    );
    assert!(raw(context, at, &[size(6).unwrap()]).unwrap().is_null());
    set(word_vt, from_bits, original_factory);
    state(10, z);
    for mode in [1, 6] {
        state(9, size(mode).unwrap());
        set(word_vt, from_bits, deny);
        assert_eq!(
            raw(execution, optional_adapter, &optargs).unwrap(),
            nil,
            "absence and language nil do not invoke the result factory"
        );
        set(word_vt, from_bits, original_factory);
    }
    for (mode, expected) in [(2, 0), (4, usize::MAX), (5, 0)] {
        state(9, size(mode).unwrap());
        let result = q
            .keep(raw(execution, optional_adapter, &optargs).unwrap())
            .unwrap();
        assert_ne!(result, nil);
        assert_eq!(
            q.word_bits(raw(result, bits_selector, &[]).unwrap())
                .unwrap(),
            expected
        );
    }
    state(9, size(7).unwrap());
    assert!(
        raw(execution, optional_adapter, &optargs).is_err(),
        "Word owner rejects a result without its value protocol"
    );
    state(9, size(3).unwrap());
    assert_eq!(
        raw(execution, optional_adapter, &optargs).unwrap_err(),
        optional_failure
    );
    state(9, z);
    let input = raw(word, from_bits, &[size(9).unwrap()]).unwrap();
    let token = raw(roots, retain, &[input]).unwrap();
    let input_id = raw(host.id.root, identity, &[input]).unwrap();
    state(5, input_id);
    state(6, token);
    let private_vt = raw(host.id.root, ensure, &[input]).unwrap();
    set(private_vt, bits_selector, bits);
    let objargs = [target, object_selector, input];
    let result = protect_pair(raw(execution, object_adapter, &objargs).unwrap(), input);
    assert_eq!(
        q.word_bits(raw(result, bits_selector, &[]).unwrap())
            .unwrap(),
        71
    );
    assert!(raw(context, at, &[size(6).unwrap()]).unwrap().is_null());
    state(9, one);
    assert_eq!(raw(execution, object_adapter, &objargs).unwrap(), nil);
    state(9, size(2).unwrap());
    assert_eq!(
        raw(execution, object_adapter, &objargs).unwrap(),
        size(37).unwrap(),
        "an immediate is a valid Oop result"
    );
    state(9, size(3).unwrap());
    assert_eq!(
        raw(execution, object_adapter, &objargs).unwrap_err(),
        object_failure
    );
    state(9, z);
    state(5, z);
    let immediate_args = [target, object_selector, size(37).unwrap()];
    let result = q
        .keep(raw(execution, object_adapter, &immediate_args).unwrap())
        .unwrap();
    assert_eq!(
        q.word_bits(raw(result, bits_selector, &[]).unwrap())
            .unwrap(),
        71
    );
    for (adapter, args, expected) in [
        (optional_adapter, optargs.as_slice(), value_id),
        (object_adapter, objargs.as_slice(), input_id),
    ] {
        state(5, expected);
        q.keep(raw(execution, adapter, args).unwrap()).unwrap();
        for length in 0..args.len() {
            assert!(raw(execution, adapter, &args[..length]).is_err());
        }
        let mut extra = args.to_vec();
        extra.push(nil);
        assert!(raw(execution, adapter, &extra).is_err());
        for index in 0..args.len() {
            let mut wrong = args.to_vec();
            wrong[index] = null;
            let callbacks = [count(7), count(8), count(21)];
            assert_eq!(
                raw(execution, adapter, &wrong).unwrap_err(),
                birnam_abi::object_reference(host.id.invalid),
                "null argument {index}"
            );
            assert_eq!(
                [count(7), count(8), count(21)],
                callbacks,
                "invalid null arguments must fail before getter, target or factory callbacks"
            );
        }
        let mut wrong = args.to_vec();
        wrong[1] = z;
        assert!(
            raw(execution, adapter, &wrong).is_err(),
            "selector must be a Symbol"
        );
    }
    let execution_vt = call(host.id.root, b"behaviorOf:", &[execution]);
    let original = call(host.id.root, b"bind:to:", &[optional_adapter, execution]);
    set(execution_vt, optional_adapter, deny);
    assert_eq!(
        raw(execution, optional_adapter, &optargs).unwrap_err(),
        denied
    );
    set(execution_vt, optional_adapter, original);
    state(5, input_id);
    state(11, one);
    assert_eq!(
        raw(execution, object_adapter, &objargs).unwrap_err(),
        denied,
        "getter mutation must change the selected target"
    );
    state(11, z);
    set(target_vt, object_selector, object);
    set(private_vt, bits_selector, deny);
    let before = count(7);
    assert_eq!(
        raw(execution, object_adapter, &objargs).unwrap_err(),
        denied
    );
    assert_eq!(
        count(7),
        before,
        "getter failure must prevent the target call"
    );
    set(private_vt, bits_selector, bits);
    set(target_vt, object_selector, deny);
    assert_eq!(
        raw(execution, object_adapter, &objargs).unwrap_err(),
        denied
    );
    set(private_vt, bits_selector, original_bits);
    set(target_vt, object_selector, object);
    call(word_vt, b"removeMethod:", &[factory_alias]);
}
