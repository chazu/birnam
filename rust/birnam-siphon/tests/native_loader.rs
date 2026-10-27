//! Native dylib admission and authored loading policy through Oop messages.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use std::{os::unix::ffi::OsStrExt, path::Path, ptr};
#[test]
#[ignore = "requires production Oop modules"]
fn native_loader_uses_messages() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let named = |n: &[u8]| h.named(n).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let string = |b: &[u8]| q.string(b).unwrap();
    let gc = named(b"GC");
    let nil = named(b"nil");
    let system = named(b"System");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let vt = |r| call(h.id.root, b"behaviorOf:", &[r]);
    let bind = |v, n: &[u8], m| {
        call(v, b"addMethod", &[key(n), m]);
    };
    let original = |v, n: &[u8]| call(v, b"lookup", &[key(n)]);
    let error = |v, k: &[u8], m: &[u8]| {
        assert_eq!(call(v, b"kind", &[]), key(k));
        assert_eq!(q.string_bytes(call(v, b"message", &[])).unwrap(), m);
    };
    let dir = std::env::temp_dir().join(format!("birnam-native-loader-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/oop-host/native-loader.c");
    for (name, flag) in [
        ("target", Some("LOADER_TARGET")),
        ("missing", Some("LOADER_MISSING")),
        ("incompatible", Some("LOADER_INCOMPATIBLE")),
        ("hooks", None),
    ] {
        let mut cc = std::process::Command::new("clang");
        cc.args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"]);
        if let Some(flag) = flag {
            cc.arg(format!("-D{flag}"));
        }
        assert!(
            cc.arg(&source)
                .arg("-o")
                .arg(dir.join(format!("{name}.dylib")))
                .status()
                .unwrap()
                .success()
        );
    }
    let fixture = h.load_native(&q, &dir.join("hooks.dylib"), &[]).unwrap();
    let method = |n: &[u8], c| {
        call(
            call(fixture, b"methodAt:", &[key(n)]),
            b"withContext:",
            &[c],
        )
    };
    let target = dir.join("target.dylib");
    let path = string(target.as_os_str().as_bytes());
    let missing = string(dir.join("absent.dylib").as_os_str().as_bytes());
    assert_eq!(call(system, b"loadModule:", &[path]), system);
    let nil_argument = call(call(named(b"Vector"), b"new", &[]), b"add:", &[nil]);
    assert_eq!(
        call(system, b"loadModule:arguments:", &[path, nil_argument]),
        nil
    );
    for (name, args) in [
        (b"loadModule:".as_slice(), vec![]),
        (b"loadModule:", vec![path, path]),
        (b"loadModule:arguments:", vec![path]),
        (b"loadModule:", vec![nil]),
        (b"loadModule:", vec![size(0).unwrap()]),
    ] {
        assert_eq!(call(system, name, &args), nil);
    }
    error(
        call(system, b"loadModule:", &[missing]),
        b"ioError",
        b"could not load module dylib",
    );
    for name in ["missing", "incompatible"] {
        error(
            call(
                system,
                b"loadModule:",
                &[string(
                    dir.join(format!("{name}.dylib")).as_os_str().as_bytes(),
                )],
            ),
            b"notAModule",
            b"missing or incompatible Oop native module declaration",
        );
    }
    error(
        call(system, b"loadModule:", &[string(b"bad\0path")]),
        b"moduleError",
        b"native module initialization failed",
    );
    let values_vt = call(named(b"ObjectVt"), b"delegated", &[]);
    let path_bytes = q.byte_values(target.as_os_str().as_bytes()).unwrap();
    let seen = hooks.context(&[ptr::null_mut(); 32]);
    let sequence = hooks.context(&[
        gc,
        named(b"Integer"),
        values_vt,
        size(0).unwrap(),
        size(0).unwrap(),
        ptr::null_mut(),
        path_bytes,
        size(0).unwrap(),
        seen,
        size(0).unwrap(),
    ]);
    let seq = call(named(b"Object"), b"clone", &[]);
    let seq_vt = call(h.id.root, b"ensurePrivateVtableFor:", &[seq]);
    bind(seq_vt, b"size", method(b"size", sequence));
    bind(seq_vt, b"at:", method(b"index", sequence));
    bind(
        seq_vt,
        b"allocation:bytes:identity:",
        method(b"allocation:bytes:identity:", seq),
    );
    bind(seq_vt, b"remaining", method(b"remaining", seq));
    bind(
        seq_vt,
        b"lastCreatedIdentity",
        method(b"lastCreatedIdentity", seq),
    );
    assert_eq!(call(system, b"loadModule:arguments:", &[path, seq]), system);
    hooks.put(sequence, 4, size(1).unwrap());
    error(
        call(system, b"loadModule:arguments:", &[path, seq]),
        b"typeError",
        b"module arguments need an Integer size",
    );
    hooks.put(sequence, 4, size(0).unwrap());
    hooks.put(sequence, 3, q.word(usize::MAX).unwrap());
    error(
        call(system, b"loadModule:arguments:", &[path, seq]),
        b"typeError",
        b"module argument count must be nonnegative",
    );
    let name_vt = call(h.id.root, b"nameBehavior", &[]);
    let mut guards = vec![];
    for (v, name, alias) in [
        (
            vt(named(b"Integer")),
            b"fromWord:".as_slice(),
            b"loaderOriginalFromWord:".as_slice(),
        ),
        (vt(named(b"Integer")), b"wordOf:", b"loaderOriginalWordOf:"),
        (named(b"StringVt"), b"bytes", b"loaderOriginalBytes"),
        (
            vt(named(b"String")),
            b"fromBytes:",
            b"loaderOriginalFromBytes:",
        ),
        (
            vt(named(b"Error")),
            b"kind:message:",
            b"loaderOriginalError:message:",
        ),
        (name_vt, b"loadNative", b"loaderOriginalLoad"),
    ] {
        let saved = original(v, name);
        bind(v, alias, saved);
        let state = hooks.context(&[
            gc,
            key(alias),
            size(0).unwrap(),
            ptr::null_mut(),
            if name == b"bytes" {
                path
            } else {
                ptr::null_mut()
            },
            path_bytes,
            h.namespace,
            ptr::null_mut(),
        ]);
        bind(v, name, method(b"forward", state));
        guards.push((v, name, alias, saved, state));
    }
    hooks.put(sequence, 3, size(2).unwrap());
    hooks.put(sequence, 7, size(1).unwrap());
    let returned = call(system, b"loadModule:arguments:", &[path, seq]);
    assert_eq!(hooks.count(sequence, 9), 2);
    assert_eq!(hooks.field(seen, 0), size(0).unwrap());
    assert_eq!(hooks.field(seen, 1), size(1).unwrap());
    assert_eq!(
        as_size(call(seq, b"remaining", &[size(1).unwrap()])),
        Some(1)
    );
    assert_ne!(returned, system);
    assert_eq!(vt(returned), values_vt);
    assert_eq!(
        call(h.id.root, b"identityOf:", &[returned]),
        call(seq, b"lastCreatedIdentity", &[])
    );
    hooks.put(sequence, 7, size(0).unwrap());
    for (i, &byte) in target.as_os_str().as_bytes().iter().enumerate() {
        call(
            path_bytes,
            b"at:put:",
            &[size(i).unwrap(), size(byte as usize).unwrap()],
        );
    }
    let system_vt = vt(system);
    let alias_vt = call(named(b"ObjectVt"), b"delegated", &[]);
    bind(
        alias_vt,
        b"loadModule:",
        original(system_vt, b"loadModule:"),
    );
    let receiver = h.send(alias_vt, b"allocate", &[size(0).unwrap()]).unwrap();
    assert_eq!(call(receiver, b"loadModule:", &[path]), receiver);
    hooks.put(sequence, 3, size(1).unwrap());
    let returned = call(system, b"loadModule:arguments:", &[path, seq]);
    assert_eq!(vt(returned), values_vt);
    assert_eq!(
        call(h.id.root, b"identityOf:", &[returned]),
        call(seq, b"lastCreatedIdentity", &[])
    );
    hooks.put(sequence, 3, size(3).unwrap());
    assert_eq!(
        as_size(call(system, b"loadModule:arguments:", &[path, seq])),
        Some(17)
    );
    hooks.put(sequence, 3, size(4).unwrap());
    error(
        call(system, b"loadModule:arguments:", &[path, seq]),
        b"moduleError",
        b"native module initialization failed",
    );
    call(gc, b"collectPrecisely", &[]);
    assert_eq!(
        as_size(call(seq, b"remaining", &[size(4).unwrap()])),
        Some(0)
    );
    let fault = hooks.fault(973);
    hooks.put(sequence, 5, fault);
    hooks.put(sequence, 3, size(1).unwrap());
    assert_eq!(
        unsafe {
            h.id.send(system, key(b"loadModule:arguments:"), &[path, seq])
        }
        .unwrap_err(),
        fault
    );
    hooks.put(sequence, 5, ptr::null_mut());
    for i in 0..guards.len() {
        let failure = hooks.fault(974);
        hooks.put(guards[i].4, 3, failure);
        let args = if i == 3 || i == 4 {
            vec![missing]
        } else {
            vec![path, seq]
        };
        let name = if args.len() == 1 {
            b"loadModule:".as_slice()
        } else {
            b"loadModule:arguments:"
        };
        let result = unsafe { h.id.send(system, key(name), &args) };
        if i == 5 {
            let value = q.keep(result.unwrap()).unwrap();
            error(
                value,
                b"moduleError",
                b"native module initialization failed",
            );
        } else {
            assert_eq!(result.unwrap_err(), failure);
        }
        hooks.put(guards[i].4, 3, ptr::null_mut());
    }
    let error_factory = named(b"Error");
    let sentinel = call(named(b"Object"), b"clone", &[]);
    let factory = call(named(b"Object"), b"clone", &[]);
    let factory_vt = call(h.id.root, b"ensurePrivateVtableFor:", &[factory]);
    bind(
        factory_vt,
        b"kind:message:",
        hooks.method(b"fixed", sentinel),
    );
    hooks.put(guards[3].4, 7, factory);
    assert_eq!(call(system, b"loadModule:", &[missing]), sentinel);
    hooks.put(guards[3].4, 7, ptr::null_mut());
    call(h.namespace, b"at:put:", &[key(b"Error"), error_factory]);
    for (v, name, alias, saved, state) in guards {
        assert!(hooks.count(state, 2) > 0);
        bind(v, name, saved);
        call(v, b"removeMethod:", &[key(alias)]);
    }
    let source = string(format!("[System loadModule: {:?}]", target.to_str().unwrap()).as_bytes());
    assert_eq!(call(named(b"Codegen"), b"eval:", &[source]), system);
    let saved = original(system_vt, b"loadModule:");
    bind(system_vt, b"loadModule:", hooks.method(b"fixed", sentinel));
    call(
        named(b"Codegen"),
        b"eval:",
        &[string(b"[Module load: #nativeLoader]")],
    );
    assert_eq!(call(system, b"loadModule:", &[path]), sentinel);
    bind(system_vt, b"loadModule:", saved);
    std::fs::remove_dir_all(dir).unwrap();
}
