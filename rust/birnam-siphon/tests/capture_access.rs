//! Current capture readers preserve aliases and results across reentry and GC.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use birnam_artifact_codec::portable_program::{NodeSpec, ProgramSpec, StatementSpec, encode};

#[test]
#[ignore = "requires production Oop modules and synchronous compiler"]
fn capture_reads_observe_current_methods_and_preserve_live_values() {
    let host = oop::bootstrap();
    let q = host.scope();
    let hooks = oop::hooks::Hooks::new(&host);
    let named = |n: &[u8]| host.named(n).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let gc = named(b"GC");
    let nil = named(b"nil");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let path = std::env::temp_dir().join(format!(
        "birnam-capture-access-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/capture-access.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = host.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let method = |name: &[u8], context| {
        call(
            call(fixture, b"methodAt:", &[key(name)]),
            b"withContext:",
            &[context],
        )
    };
    let behavior = |object| call(host.id.root, b"behaviorOf:", &[object]);
    let bind = |vt, selector: &[u8], method| {
        call(vt, b"addMethod", &[key(selector), method]);
    };
    let observer = call(named(b"Object"), b"clone", &[]);
    let ovt = call(host.id.root, b"ensurePrivateVtableFor:", &[observer]);
    for selector in [b"present:".as_slice(), b"allocation:bytes:identity:"] {
        bind(ovt, selector, method(selector, nil));
    }
    let portable = encode(ProgramSpec {
        parameters: &[],
        captures: &[b"a", b"b"],
        lets: &[],
        statements: &[StatementSpec::Expression { root_node: 0 }],
        nodes: &[NodeSpec::LexicalRead(0)],
    })
    .unwrap();
    let admit = |bytes: &[u8]| {
        let bytes = call(
            named(b"Bytes"),
            b"fromBytes:",
            &[q.byte_values(bytes).unwrap()],
        );
        call(named(b"CodeObject"), b"fromCanonicalBytes:", &[bytes])
    };
    let code = admit(&portable);
    let vector = |values: &[Oop]| {
        let mut v = call(named(b"Vector"), b"new", &[]);
        for &value in values {
            v = call(v, b"add:", &[value]);
        }
        v
    };
    let a = q.word(7).unwrap();
    let b = q.word(8).unwrap();
    let block = call(
        named(b"Codegen"),
        b"materializeSynchronous:captures:",
        &[code, vector(&[a, b])],
    );
    let repeated = call(
        named(b"Codegen"),
        b"materializeSynchronous:captures:",
        &[code, vector(&[a, a])],
    );
    let vt = behavior(block);
    let factory = named(b"Block");
    let fvt = behavior(factory);
    let reader = call(vt, b"lookup", &[key(b"captureWordAt:")]);
    let factory_reader = call(fvt, b"lookup", &[key(b"captureWord:of:")]);
    for (target, expected) in [(block, [a, b]), (repeated, [a, a])] {
        for (index, value) in expected.into_iter().enumerate() {
            assert_eq!(
                call(target, b"captureWordAt:", &[size(index).unwrap()]),
                value
            );
            assert_eq!(
                call(factory, b"captureWord:of:", &[size(index).unwrap(), target]),
                value
            );
        }
        assert_eq!(call(target, b"captureWordAt:", &[size(2).unwrap()]), nil);
        assert_eq!(call(target, b"value", &[]), a);
        let cloned = call(target, b"clone", &[]);
        assert_ne!(cloned, target);
        call(gc, b"collectPrecisely", &[]);
        for (index, value) in expected.into_iter().enumerate() {
            assert_eq!(call(cloned, b"captureWordAt:", &[size(index).unwrap()]), value);
        }
        assert_eq!(call(cloned, b"value", &[]), a);
    }
    let clone_method = call(vt, b"lookup", &[key(b"clone")]);
    let no_args = call(host.id.root, b"sequence", &[]);
    let foreign = q.string(b"foreign storage").unwrap();
    assert!(host.send(clone_method, b"invokeOn:arguments:", &[foreign, no_args]).is_err(),
            "a capture snapshot must validate Execution ownership before reading storage");
    assert_eq!(call(block, b"captureWordAt:", &[q.word(1).unwrap()]), b);
    assert_eq!(call(block, b"captureWordAt:", &[std::ptr::null_mut()]), nil);
    let body = call(block, b"methodFor:", &[key(b"value")]);
    assert_eq!(call(body, b"invokeOn:", &[nil]), nil);
    for selector in [
        b"tryBorrowCaptureOperands:".as_slice(),
        b"captureReadMode",
        b"captureReadModeOf:",
    ] {
        for receiver in [block, factory, nil] {
            assert!(
                call(host.id.root, b"bind:to:", &[key(selector), receiver]).is_null(),
                "retired raw capture protocols must remain absent"
            );
        }
    }
    let failure = unsafe {
        host.id.send(
            named(b"Execution"),
            key(b"failNative:detail:"),
            &[size(812).unwrap(), size(19).unwrap()],
        )
    }
    .unwrap_err();
    q.keep(failure).unwrap();
    let denied = hooks.context(&[failure, size(0).unwrap()]);
    let deny = hooks.method(b"deny", denied);
    let gcvt = behavior(gc);
    let prepare = call(gcvt, b"lookup", &[key(b"prepareAllocation")]);
    let direct_key = key(b"captureWordAt:");
    let factory_key = key(b"captureWord:of:");
    let value_key = key(b"value");
    let add_key = key(b"addMethod");
    let prepare_key = key(b"prepareAllocation");
    let raw = |r, s, args: &[Oop]| unsafe { host.id.send(r, s, args) };
    bind(gcvt, b"prepareAllocation", deny);
    assert_eq!(raw(block, direct_key, &[size(0).unwrap()]).unwrap(), a);
    assert_eq!(
        raw(factory, factory_key, &[size(1).unwrap(), block]).unwrap(),
        b
    );
    raw(gcvt, add_key, &[prepare_key, prepare]).unwrap();
    assert_eq!(
        hooks.count(denied, 1),
        0,
        "default Word-indexed reads require no allocation admission"
    );

    let state = hooks.context(&[
        gc,
        named(b"Word"),
        size(0).unwrap(),
        size(0).unwrap(),
        std::ptr::null_mut(),
        vt,
        direct_key,
        observer,
        host.roots,
    ]);
    let fresh = method(b"fresh:", state);
    for (owner, selector, replacement, original) in [
        (vt, b"captureWordAt:".as_slice(), fresh, reader),
        (
            fvt,
            b"captureWord:of:",
            method(b"factory:of:", state),
            factory_reader,
        ),
    ] {
        hooks.put(state, 2, size(0).unwrap());
        bind(owner, selector, replacement);
        let result = call(block, b"value", &[]);
        assert_eq!(q.word_bits(result).unwrap(), 101);
        assert_eq!(
            hooks.count(state, 2),
            2,
            "every capture uses the current reader"
        );
        bind(owner, selector, original);
        bind(owner, selector, deny);
        assert_eq!(raw(block, value_key, &[]).unwrap_err(), failure);
        bind(owner, selector, original);
    }
    // The first projection replaces the second projection's reader. No cached
    // Method or old borrowing proof may bypass the replacement.
    hooks.put(state, 4, deny);
    hooks.put(state, 2, size(0).unwrap());
    bind(vt, b"captureWordAt:", fresh);
    assert_eq!(raw(block, value_key, &[]).unwrap_err(), failure);
    assert_eq!(hooks.count(state, 2), 1);
    bind(vt, b"captureWordAt:", reader);
    hooks.put(state, 4, std::ptr::null_mut());

    let callback_code = admit(
        &encode(ProgramSpec {
            parameters: &[],
            captures: &[b"a", b"b"],
            lets: &[],
            statements: &[
                StatementSpec::Expression { root_node: 1 },
                StatementSpec::Expression { root_node: 2 },
            ],
            nodes: &[
                NodeSpec::LexicalRead(0),
                NodeSpec::Send {
                    selector: b"nested",
                    operands: &[0],
                },
                NodeSpec::LexicalRead(1),
            ],
        })
        .unwrap(),
    );
    let only = host.scope();
    let (callback, owned, ids, nested_state) = {
        let temporary = host.scope();
        let tc = |r, n: &[u8], a: &[Oop]| temporary.call(r, n, a).unwrap();
        let first = tc(named(b"Object"), b"clone", &[]);
        let second = tc(named(b"Object"), b"clone", &[]);
        let ids = [
            tc(host.id.root, b"identityOf:", &[first]),
            tc(host.id.root, b"identityOf:", &[second]),
        ];
        let nested_state = tc(
            host.id.root,
            b"sequence",
            &[gc, block, ids[1], size(0).unwrap(), observer],
        );
        let nested_method = tc(
            tc(fixture, b"methodAt:", &[key(b"nested")]),
            b"withContext:",
            &[nested_state],
        );
        let private = tc(host.id.root, b"ensurePrivateVtableFor:", &[first]);
        tc(private, b"addMethod", &[key(b"nested"), nested_method]);
        let captures = tc(
            tc(tc(named(b"Vector"), b"new", &[]), b"add:", &[first]),
            b"add:",
            &[second],
        );
        let callback = tc(
            named(b"Codegen"),
            b"materializeSynchronous:captures:",
            &[callback_code, captures],
        );
        let selected = call(block, b"methodFor:", &[value_key]);
        let owned = tc(
            factory,
            b"fromMethod:captureOperands:",
            &[selected, first, second],
        );
        only.keep(callback).unwrap();
        only.keep(owned).unwrap();
        (callback, owned, ids, nested_state)
    };
    call(gc, b"collectPrecisely", &[]);
    for identity in ids {
        assert_eq!(as_size(call(observer, b"present:", &[identity])), Some(1));
    }
    // Raw sends avoid accidentally retaining captures in the outer test scope.
    let first = raw(owned, value_key, &[]).unwrap();
    assert_eq!(
        host.send(host.id.root, b"identityOf:", &[first]).unwrap(),
        ids[0]
    );
    let second = raw(callback, value_key, &[]).unwrap();
    assert_eq!(
        host.send(host.id.root, b"identityOf:", &[second]).unwrap(),
        ids[1]
    );
    assert_eq!(
        as_size(
            host.send(nested_state, b"at:", &[size(3).unwrap()])
                .unwrap()
        ),
        Some(1)
    );
    drop(only);
    call(gc, b"collectPrecisely", &[]);
    for identity in ids {
        assert_eq!(
            as_size(call(observer, b"present:", &[identity])),
            Some(0),
            "test scopes must not hide capture ownership"
        );
    }
}
