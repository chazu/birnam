//! Explicit incoming Oop adapters preserve native failures and defer control safely.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{as_size, size};
use std::os::unix::ffi::OsStrExt;

#[test]
#[ignore = "requires production Oop native adapters"]
fn incoming_adapters_preserve_failures_without_holding_a_native_branch_boundary() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let named = |name: &[u8]| h.named(name).unwrap();
    let key = |name: &[u8]| hooks.symbol(name);
    let call = |r, n: &[u8], a: &[_]| q.call(r, n, a).unwrap();
    let eval = |text: &[u8]| {
        let source = q.string(text).unwrap();
        let value = call(named(b"Codegen"), b"eval:", &[source]);
        q.check_result(value).unwrap();
        value
    };
    let gc = named(b"GC");
    let yes = named(b"true");
    let nil = named(b"nil");
    let execution = named(b"Execution");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let body = eval(b"[:self :argument | argument]");
    let signature = call(
        named(b"Bytes"),
        b"fromBytes:",
        &[q.byte_values(b"oO").unwrap()],
    );
    let adapted = call(execution, b"block:withNativeSignature:", &[body, signature]);
    q.check_result(adapted).unwrap();
    call(
        signature,
        b"appendByteValues",
        &[size(b'x' as usize).unwrap()],
    );
    let description = call(execution, b"nativeSignatureOf:", &[adapted]);
    assert_eq!(q.read_bytes(description).unwrap(), b"oO");
    call(
        description,
        b"appendByteValues",
        &[size(b'x' as usize).unwrap()],
    );
    assert_eq!(
        q.read_bytes(call(execution, b"nativeSignatureOf:", &[adapted]))
            .unwrap(),
        b"oO"
    );
    let target = call(named(b"Object"), b"clone", &[]);
    call(
        named(b"Reflect"),
        b"defineMethod:on:as:",
        &[key(b"echo:"), target, adapted],
    );
    assert_eq!(call(target, b"echo:", &[yes]), yes);
    assert!(h.send(target, b"echo:", &[]).is_err());
    assert!(h.send(target, b"echo:", &[std::ptr::null_mut()]).is_err());
    let block_vt = named(b"BlockVt");
    let value_selector = key(b"value:value:");
    let original_value = call(block_vt, b"lookup", &[value_selector]);
    let intercepted = hooks.context(&[named(b"false"), size(0).unwrap()]);
    call(
        block_vt,
        b"addMethod",
        &[value_selector, hooks.method(b"fixedCount", intercepted)],
    );
    assert_eq!(call(target, b"echo:", &[yes]), named(b"false"));
    assert_eq!(
        hooks.count(intercepted, 1),
        1,
        "explicit adapters still use the Block's current value method"
    );
    call(block_vt, b"addMethod", &[value_selector, original_value]);
    let cloned = call(adapted, b"clone", &[]);
    assert_eq!(
        q.read_bytes(call(execution, b"nativeSignatureOf:", &[cloned]))
            .unwrap(),
        b"oO"
    );
    let entered = call(h.id.root, b"bind:to:", &[key(b"echo:"), target]);
    let captured = call(entered, b"captured", &[]);
    let ordinary = call(entered, b"withImplementation:", &[body]);
    let target_vt = call(h.id.root, b"behaviorOf:", &[target]);
    call(target_vt, b"addMethod", &[key(b"echo:"), ordinary]);
    assert_eq!(
        call(target, b"echo:", &[]),
        nil,
        "an ordinary replacement must not inherit native arity policy"
    );
    assert!(
        h.send(captured, b"invokeOn:", &[target]).is_err(),
        "a captured native Method retains its contract"
    );

    // Construction must not probe an unrelated implementation's metadata.
    let foreign = call(named(b"Object"), b"clone", &[]);
    let vt = call(h.id.root, b"ensurePrivateVtableFor:", &[foreign]);
    let poison = hooks.context(&[hooks.fault(91), size(0).unwrap()]);
    call(
        vt,
        b"addMethod",
        &[key(b"codeObject"), hooks.method(b"deny", poison)],
    );
    let fixed = hooks.context(&[yes, size(0).unwrap()]);
    call(
        vt,
        b"addMethod",
        &[key(b"value:"), hooks.method(b"fixedCount", fixed)],
    );
    call(
        named(b"Reflect"),
        b"defineMethod:on:as:",
        &[key(b"foreign"), target, foreign],
    );
    assert_eq!(hooks.count(poison, 1), 0);
    assert_eq!(hooks.count(fixed, 1), 0);
    assert_eq!(call(target, b"foreign", &[]), yes);
    assert_eq!(hooks.count(fixed, 1), 1);
    assert_eq!(hooks.count(poison, 1), 0);

    let failure = hooks.fault(719);
    let callback = call(named(b"Object"), b"clone", &[]);
    let cb_vt = call(h.id.root, b"ensurePrivateVtableFor:", &[callback]);
    let failed = hooks.context(&[failure, size(0).unwrap()]);
    call(
        cb_vt,
        b"addMethod",
        &[key(b"value"), hooks.method(b"deny", failed)],
    );
    let values = eval(
        br#"let seen := [Cell with: #[]]; let target := [Object clone];
      [Reflect defineMethod: #nativeFailed:detail: on: target as: [:self :status :detail |
        [System gc]; [seen set: #[status detail]]; nil]];
      let body := [Execution block: [:self :branch | branch] withNativeSignature: "!vO"];
      [Reflect defineMethod: #run: on: target as: body]; #[target seen]"#,
    );
    let observed = q.vector_at(values, 0).unwrap();
    let seen = q.vector_at(values, 1).unwrap();
    call(
        observed,
        b"nativeFailed:detail:",
        &[size(6).unwrap(), size(719).unwrap()],
    );
    call(seen, b"set:", &[nil]);
    assert_eq!(
        unsafe { h.id.send(observed, key(b"run:"), &[callback]) }.unwrap_err(),
        failure
    );
    let event = call(seen, b"value", &[]);
    assert_eq!(as_size(q.vector_at(event, 0).unwrap()), Some(6));
    assert_eq!(as_size(q.vector_at(event, 1).unwrap()), Some(719));
    call(seen, b"set:", &[nil]);
    assert_eq!(
        unsafe { h.id.send(observed, key(b"run:"), &[]) }.unwrap_err(),
        birnam_abi::object_reference(h.id.invalid)
    );
    assert_eq!(
        q.vector_size(call(seen, b"value", &[])).unwrap(),
        2,
        "incoming arity failure must notify its observer"
    );
    let error = call(named(b"Error"), b"kind:", &[key(b"ordinaryErrorData")]);
    for data in [error, failure] {
        call(
            cb_vt,
            b"addMethod",
            &[key(b"value"), hooks.method(b"fixed", data)],
        );
        call(seen, b"set:", &[nil]);
        assert_eq!(call(observed, b"run:", &[callback]), data);
        assert_eq!(
            call(seen, b"value", &[]),
            nil,
            "unmarked Error/Fault data must not trigger the failure observer"
        );
    }

    // A deferred branch can park. Its selector's failure policy must not leak
    // into another Machine while that branch is suspended.
    call(
        cb_vt,
        b"addMethod",
        &[key(b"value"), hooks.method(b"deny", failed)],
    );
    let globals = named(b"Globals");
    call(globals, b"at:put:", &[key(b"BoundaryObserved"), observed]);
    for source in [
        b"[| [true ifTrue: [| [BoundaryMachine park]]]]".as_slice(),
        b"[| [BoundaryObserved run: [| [BoundaryMachine park]]]]",
    ] {
        let machine = call(named(b"Machine"), b"new", &[]);
        call(globals, b"at:put:", &[key(b"BoundaryMachine"), machine]);
        call(globals, b"at:put:", &[key(b"BoundaryFailure"), callback]);
        let branch = eval(source);
        assert_eq!(call(machine, b"runBlock:", &[branch]), machine);
        assert_eq!(call(machine, b"executionState", &[]), key(b"suspended"));
        let other = call(named(b"Machine"), b"new", &[]);
        let body = eval(b"[| [BoundaryFailure value]]");
        let result = call(other, b"runBlock:", &[body]);
        assert_eq!(call(result, b"kind", &[]), key(b"nativeFailure"));
        call(gc, b"collectPrecisely", &[]);
        assert_eq!(call(machine, b"resumeWith:", &[yes]), yes);
    }
    for name in [
        b"BoundaryMachine".as_slice(),
        b"BoundaryFailure",
        b"BoundaryObserved",
    ] {
        call(globals, b"remove:", &[key(name)]);
    }

    // Signatures remain CodeObject metadata and recover through the existing
    // Image path; no new image family or native pointer carrier is needed.
    call(
        globals,
        b"at:put:",
        &[key(b"NativeBoundaryObserved"), observed],
    );
    let path = std::env::temp_dir().join(format!(
        "birnam-native-boundary-{}.image",
        std::process::id()
    ));
    let filename = q.string(path.as_os_str().as_bytes()).unwrap();
    assert_eq!(call(named(b"System"), b"saveImage:", &[filename]), yes);
    for rebuild in [false, true] {
        let restored = call(
            named(b"System"),
            b"loadImage:rebuild:",
            &[filename, named(if rebuild { b"true" } else { b"false" })],
        );
        q.check_result(restored).unwrap();
        assert_eq!(
            restored,
            named(if rebuild { b"false" } else { b"true" }),
            "load result reports whether code was mapped"
        );
        let recovered = h.global(&q, b"NativeBoundaryObserved").unwrap();
        assert_eq!(
            unsafe { h.id.send(recovered, key(b"run:"), &[callback]) }.unwrap_err(),
            failure
        );
        assert!(h.send(recovered, b"run:", &[]).is_err());
    }
    std::fs::remove_file(path).unwrap();
}
