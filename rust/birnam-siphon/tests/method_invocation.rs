//! Direct method construction, live overrides, reentry and code lifetime.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{as_size, size, Oop};
use std::{os::unix::ffi::OsStrExt, path::Path, ptr};

#[test]
#[ignore = "requires production Oop modules"]
fn default_construction_shares_code_and_observes_current_policy() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let named = |n: &[u8]| h.named(n).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let int = |n| call(named(b"Integer"), b"fromWord:", &[size(n).unwrap()]);
    let number = |n| q.word_bits(call(n, b"word", &[])).unwrap();
    let eval = |source: &[u8]| {
        let value = call(named(b"Codegen"), b"eval:", &[q.string(source).unwrap()]);
        q.check_result(value).unwrap();
        value
    };
    let gc = named(b"GC");
    let tcc = named(b"TCC");
    let reflect = named(b"Reflect");
    let nil = named(b"nil");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let module = h.global(&q, b"Module").unwrap();
    call(
        module,
        b"libDir:",
        &[q.string(h.resources.lib_dir.as_os_str().as_bytes())
            .unwrap()],
    );
    let store_path = q
        .string(
            h.resources
                .lib_dir
                .join("libbirnam_store.dylib")
                .as_os_str()
                .as_bytes(),
        )
        .unwrap();
    q.check_result(call(named(b"Codegen"), b"load:", &[store_path]))
        .unwrap();
    call(module, b"markLoaded:", &[key(b"store")]);
    let baseline = false;
    let target = call(named(b"Object"), b"clone", &[]);
    let body = eval(b"[:self :argument | argument]");
    let selector = key(b"sharedProbe:");
    let path = std::env::temp_dir().join(format!(
        "birnam-method-invocation-{}.dylib",
        std::process::id()
    ));
    assert!(std::process::Command::new("clang")
        .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/oop-host/method-invocation.c")
        )
        .arg("-o")
        .arg(&path)
        .status()
        .unwrap()
        .success());
    let fixture = h.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let method = |n: &[u8], c| {
        call(
            call(fixture, b"methodAt:", &[key(n)]),
            b"withContext:",
            &[c],
        )
    };
    let monitor = call(named(b"Object"), b"clone", &[]);
    let monitor_vt = call(h.id.root, b"ensurePrivateVtableFor:", &[monitor]);
    for name in [
        b"watch".as_slice(),
        b"remaining",
        b"allocation:bytes:identity:",
    ] {
        call(
            monitor_vt,
            b"addMethod",
            &[key(name), method(name, monitor)],
        );
    }
    let units = || q.word_bits(call(tcc, b"liveNativeUnits", &[])).unwrap();
    let collect = || {
        call(gc, b"collectPrecisely", &[]);
        call(tcc, b"checkpoint", &[]);
    };
    collect();
    let binding_basis = number(call(named(b"Globals"), b"bindingBasis", &[]));
    let before = units();
    let resident_bytes = || {
        if call(tcc, b"methodFor:", &[key(b"liveNativeBytes")]) == nil {
            None
        } else {
            Some(q.word_bits(call(tcc, b"liveNativeBytes", &[])).unwrap())
        }
    };
    let before_bytes = resident_bytes();
    {
        let bindings = h.scope();
        let mut methods = Vec::new();
        let started = std::time::Instant::now();
        for _ in 0..16 {
            assert_eq!(
                bindings
                    .call(reflect, b"defineMethod:on:as:", &[selector, target, body])
                    .unwrap(),
                target
            );
            let captured = bindings.call(target, b"methodFor:", &[selector]).unwrap();
            assert!(
                !methods.contains(&captured),
                "logical bindings must be distinct"
            );
            methods.push(captured);
        }
        let elapsed = started.elapsed().as_nanos();
        h.send(monitor, b"watch", &methods).unwrap();
        collect();
        assert_eq!(as_size(call(monitor, b"remaining", &[])), Some(16));
        // Both constructors share declared code now; neither emits another TCC unit.
        assert_eq!(units(), before);
        assert_eq!(resident_bytes(), before_bytes);
        let argument = named(b"true");
        assert_eq!(call(target, b"sharedProbe:", &[argument]), argument);
        for _ in 0..100 {
            unsafe {
                h.id.send(target, selector, &[argument]).unwrap();
            }
        }
        let started = std::time::Instant::now();
        for _ in 0..10000 {
            assert_eq!(
                unsafe { h.id.send(target, selector, &[argument]) }.unwrap(),
                argument
            );
        }
        eprintln!(
            "declared method construction: baseline={baseline}, definitions=16, added_tcc_units=0, elapsed_ns={elapsed}; pre-interned invocation calls=10000 elapsed_ns={}",
            started.elapsed().as_nanos()
        );
        call(reflect, b"removeMethod:on:", &[selector, target]);
    }
    collect();
    assert_eq!(units(), before);
    assert_eq!(as_size(call(monitor, b"remaining", &[])), Some(0));
    assert_eq!(
        number(call(named(b"Globals"), b"bindingBasis", &[])),
        binding_basis
    );
    let lookup = |r, n: &[u8]| call(h.id.root, b"bind:to:", &[key(n), r]);
    let replace = |r, n: &[u8], m| {
        let vt = call(h.id.root, b"ensurePrivateVtableFor:", &[r]);
        call(vt, b"addMethod", &[key(n), m]);
    };
    let error = call(named(b"Error"), b"kind:", &[key(b"templateRejected")]);
    let state = hooks.context(&[gc, error, size(0).unwrap(), ptr::null_mut()]);
    let probe = method(b"probe", state);
    let original = lookup(reflect, b"methodForArity:implementation:");
    replace(reflect, b"methodForArity:implementation:", probe);
    assert_eq!(
        call(reflect, b"defineMethod:on:as:", &[selector, target, body]),
        error
    );
    assert_eq!(hooks.count(state, 2), 1);
    assert_eq!(call(target, b"methodFor:", &[selector]), nil);
    hooks.put(state, 3, hooks.fault(17));
    assert!(h
        .send(reflect, b"defineMethod:on:as:", &[selector, target, body])
        .is_err());
    assert_eq!(hooks.count(state, 2), 2);
    replace(reflect, b"methodForArity:implementation:", original);
    hooks.put(state, 3, ptr::null_mut());
    let compile = lookup(tcc, b"compile:");
    replace(tcc, b"compile:", probe);
    assert_eq!(
        call(reflect, b"defineMethod:on:as:", &[selector, target, body]),
        target
    );
    assert_eq!(hooks.count(state, 2), 2, "binding must share existing code");
    replace(tcc, b"compile:", compile);
    call(reflect, b"removeMethod:on:", &[selector, target]);
    let nested_target = call(named(b"Object"), b"clone", &[]);
    let nested_selector = key(b"nestedProbe");
    let nested_body = eval(b"[:self | self]");
    let reflect_vt = call(h.id.root, b"behaviorOf:", &[reflect]);
    let alias = key(b"fixtureConstruction:implementation:");
    call(reflect_vt, b"addMethod", &[alias, original]);
    let nested = hooks.context(&[
        alias,
        reflect,
        nested_target,
        nested_selector,
        nested_body,
        gc,
        size(0).unwrap(),
    ]);
    replace(
        reflect,
        b"methodForArity:implementation:",
        method(b"reenter", nested),
    );
    assert_eq!(
        call(reflect, b"defineMethod:on:as:", &[selector, target, body]),
        target
    );
    assert_eq!(
        hooks.count(nested, 6),
        2,
        "constructor callback must not replay"
    );
    assert_eq!(call(nested_target, b"nestedProbe", &[]), nested_target);
    replace(reflect, b"methodForArity:implementation:", original);
    call(reflect_vt, b"removeMethod:", &[alias]);
    assert_eq!(
        call(target, b"sharedProbe:", &[named(b"true")]),
        named(b"true")
    );
    assert_eq!(call(nested_target, b"nestedProbe", &[]), nested_target);
    // Construction does not revise Globals, Store or installed bindings.
    let globals = number(call(named(b"Globals"), b"bindingBasis", &[]));
    let facts = number(call(named(b"db"), b"basis", &[]));
    let installed = lookup(target, b"sharedProbe:");
    let m = call(reflect, b"methodForArity:implementation:", &[int(1), body]);
    q.check_result(m).unwrap();
    assert_eq!(
        number(call(named(b"Globals"), b"bindingBasis", &[])),
        globals
    );
    assert_eq!(number(call(named(b"db"), b"basis", &[])), facts);
    assert_eq!(lookup(target, b"sharedProbe:"), installed);
    let facade = call(reflect, b"clone", &[]);
    let facade_target = call(named(b"Object"), b"clone", &[]);
    let facade_selector = key(b"facadeProbe:");
    let facade_constructor = lookup(facade, b"methodForArity:implementation:");
    replace(facade, b"methodForArity:implementation:", probe);
    assert_eq!(
        call(
            facade,
            b"defineMethod:on:as:",
            &[facade_selector, facade_target, body]
        ),
        error
    );
    assert_eq!(call(facade_target, b"methodFor:", &[facade_selector]), nil);
    assert_eq!(
        call(
            reflect,
            b"defineMethod:on:as:",
            &[facade_selector, facade_target, body]
        ),
        facade_target
    );
    replace(
        facade,
        b"methodForArity:implementation:",
        facade_constructor,
    ); // Check the actual publication contract in a fresh full-language process.
    prepared_generation(&h.resources.prefix);
}

fn prepared_generation(prefix: &Path) {
    let dir = std::env::temp_dir().join(format!("birnam-method-generation-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let artifact = dir.join("candidate.o");
    let script = dir.join("test.birnam");
    std::fs::write(
        &script,
        format!(
            "let MethodInvocationGenerationPath := {};\n[System println: {}];\n",
            serde_json::to_string(artifact.to_str().unwrap()).unwrap(),
            include_str!("../../../tests/fixtures/method-invocation-generation.birnam")
        ),
    )
    .unwrap();
    let mut command = std::process::Command::new(prefix.join("bin/birnam"));
    for (name, _) in std::env::vars_os() {
        if name.as_encoded_bytes().starts_with(b"BIRNAM_") {
            command.env_remove(name);
        }
    }
    let output = command
        .arg(&script)
        .current_dir(&dir)
        .env("BIRNAM_FORCE_SOURCE", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success() && output.stderr.is_empty(),
        "prepared generation: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout,
        format!("#[{}]\n", vec!["true"; 14].join(" ")).as_bytes()
    );
    assert!(!artifact.exists() && !dir.join("candidate.o.candidate").exists());
    std::fs::remove_dir_all(dir).unwrap();
    eprintln!("prepared generation survives pure Method construction: 14 checks passed");
}
