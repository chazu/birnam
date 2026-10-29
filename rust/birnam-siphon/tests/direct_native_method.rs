//! Explicit native conversion constructs a Method without a definition forwarder.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{as_size, size, Oop};

#[test]
#[ignore = "requires production Oop Execution"]
fn direct_native_methods_preserve_conversion_failure_and_deferred_control() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let named = |n: &[u8]| h.named(n).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let eval = |source: &[u8]| {
        let value = call(named(b"Codegen"), b"eval:", &[q.string(source).unwrap()]);
        q.check_result(value).unwrap();
        value
    };
    let execution = named(b"Execution");
    let gc = named(b"GC");
    let target = call(named(b"Object"), b"clone", &[]);
    let vt = call(h.id.root, b"ensurePrivateVtableFor:", &[target]);
    let epoch = || as_size(call(h.id.root, b"bindingEpoch", &[])).unwrap();
    let construct = |body, signature: &[u8]| {
        let signature = q.string(signature).unwrap();
        let before = epoch();
        let method = call(
            execution,
            b"method:withNativeSignature:",
            &[body, signature],
        );
        assert_eq!(
            epoch(),
            before,
            "native Method construction must not edit bindings"
        );
        assert_eq!(
            call(h.id.root, b"behaviorOf:", &[method]),
            call(h.id.root, b"methodBehavior", &[])
        );
        let reflected = call(execution, b"bodyOfMethod:", &[method]);
        assert_ne!(reflected, named(b"nil"));
        assert_eq!(
            call(reflected, b"codeObject", &[]),
            call(method, b"codeObject", &[])
        );
        assert_eq!(
            q.read_bytes(call(execution, b"nativeSignatureOf:", &[reflected]))
                .unwrap(),
            q.read_bytes(signature).unwrap()
        );
        assert_eq!(
            epoch(),
            before,
            "provider reflection must not edit bindings"
        );
        method
    };
    let values = eval(
        b"let capture := [Cell with: 7]; #[capture [:self :arg | #[self [capture value] arg]]]",
    );
    let cell = q.vector_at(values, 0).unwrap();
    let body = q.vector_at(values, 1).unwrap();
    let method = construct(body, b"oO");
    let description = call(execution, b"nativeSignatureOf:", &[method]);
    assert_eq!(q.read_bytes(description).unwrap(), b"oO");
    call(
        description,
        b"appendByteValues",
        &[size(b'x' as usize).unwrap()],
    );
    assert_eq!(
        q.read_bytes(call(execution, b"nativeSignatureOf:", &[method]))
            .unwrap(),
        b"oO"
    );
    assert_eq!(
        call(
            named(b"Reflect"),
            b"defineMethod:on:as:",
            &[key(b"native:"), target, method]
        ),
        target
    );
    assert_eq!(call(vt, b"lookup", &[key(b"native:")]), method);
    let body_vt = call(h.id.root, b"ensurePrivateVtableFor:", &[body]);
    let poisoned = hooks.context(&[hooks.fault(91), size(0).unwrap()]);
    call(
        body_vt,
        b"addMethod",
        &[key(b"value:value:"), hooks.method(b"deny", poisoned)],
    );
    let argument = call(named(b"Object"), b"clone", &[]);
    let result = call(target, b"native:", &[argument]);
    assert_eq!(q.vector_at(result, 0).unwrap(), target);
    assert_eq!(q.vector_at(result, 2).unwrap(), argument);
    assert_eq!(hooks.count(poisoned, 1), 0);
    let next = eval(b"18");
    call(cell, b"set:", &[next]);
    call(gc, b"collectPrecisely", &[]);
    let result = call(target, b"native:", &[argument]);
    assert_eq!(q.vector_at(result, 1).unwrap(), next);
    let captured = call(target, b"methodFor:", &[key(b"native:")]);
    assert_eq!(
        q.read_bytes(call(execution, b"nativeSignatureOf:", &[captured]))
            .unwrap(),
        b"oO"
    );
    assert!(h.send(target, b"native:", &[]).is_err());
    assert!(h.send(target, b"native:", &[std::ptr::null_mut()]).is_err());

    let echo = eval(b"[:self :arg | arg]");
    for (name, signature, input) in [
        (b"word:".as_slice(), b"ww".as_slice(), size(29).unwrap()),
        (
            b"bytes:".as_slice(),
            b"bb".as_slice(),
            q.string(b"a\0z").unwrap(),
        ),
    ] {
        let method = construct(echo, signature);
        call(vt, b"addMethod", &[key(name), method]);
        let result = call(target, name, &[input]);
        if name == b"word:" {
            assert_eq!(result, input);
        } else {
            assert_eq!(q.read_bytes(result).unwrap(), b"a\0z");
        }
    }

    let observations = eval(b"let seen := [Cell with: nil]; #[seen [:self :status :detail | [System gc]; [seen set: #[self status detail]]; nil]]");
    let seen = q.vector_at(observations, 0).unwrap();
    let observer = call(
        execution,
        b"methodFromBlock:",
        &[q.vector_at(observations, 1).unwrap()],
    );
    call(vt, b"addMethod", &[key(b"nativeFailed:detail:"), observer]);
    let failure = hooks.fault(731);
    let failed = hooks.context(&[failure, size(0).unwrap()]);
    let raw = hooks.method(b"deny", failed);
    let native_body = call(named(b"Block"), b"fromMethod:captureOperands:", &[raw]);
    let failing = construct(native_body, b"!oO");
    call(vt, b"addMethod", &[key(b"fail:"), failing]);
    assert_eq!(
        unsafe { h.id.send(target, key(b"fail:"), &[argument]) }.unwrap_err(),
        failure
    );
    let event = call(seen, b"value", &[]);
    assert_eq!(q.vector_at(event, 0).unwrap(), target);
    assert_eq!(as_size(q.vector_at(event, 2).unwrap()), Some(731));
    assert_eq!(hooks.count(failed, 1), 1);
    call(seen, b"set:", &[named(b"nil")]);
    assert!(h.send(target, b"fail:", &[std::ptr::null_mut()]).is_err());
    assert_eq!(
        hooks.count(failed, 1),
        1,
        "invalid arguments must not enter native code"
    );
    assert_eq!(q.vector_at(call(seen, b"value", &[]), 0).unwrap(), target);

    // The deferred branch executes outside the native conversion/body boundary.
    let choose = construct(echo, b"!vO");
    call(
        named(b"Reflect"),
        b"defineMethod:on:as:",
        &[key(b"choose:"), target, choose],
    );
    let machine = call(named(b"Machine"), b"new", &[]);
    let globals = named(b"Globals");
    call(globals, b"at:put:", &[key(b"DirectNativeTarget"), target]);
    call(globals, b"at:put:", &[key(b"DirectNativeMachine"), machine]);
    let root = eval(b"[| [DirectNativeTarget choose: [| [DirectNativeMachine park]; 42]]]");
    call(seen, b"set:", &[named(b"nil")]);
    assert_eq!(call(machine, b"runBlock:", &[root]), machine);
    assert_eq!(call(machine, b"executionState", &[]), key(b"suspended"));
    call(gc, b"collectPrecisely", &[]);
    let result = call(machine, b"resumeWith:", &[named(b"nil")]);
    assert_eq!(q.word_bits(call(result, b"word", &[])).unwrap(), 42);
    assert_eq!(call(seen, b"value", &[]), named(b"nil"));
    let callback = call(named(b"Object"), b"clone", &[]);
    let cb_vt = call(h.id.root, b"ensurePrivateVtableFor:", &[callback]);
    call(cb_vt, b"addMethod", &[key(b"value"), raw]);
    assert_eq!(
        unsafe { h.id.send(target, key(b"choose:"), &[callback]) }.unwrap_err(),
        failure
    );
    assert_eq!(q.vector_at(call(seen, b"value", &[]), 0).unwrap(), target);

    // Actual synchronous generated bodies retain their native call contract.
    let source = q.string(b"[:self :arg | arg]").unwrap();
    let code = call(named(b"Codegen"), b"compileSynchronous:", &[source]);
    q.check_result(code).unwrap();
    let outer = call(named(b"Block"), b"fromMethod:captureOperands:", &[code]);
    let synchronous = call(outer, b"value", &[]);
    let method = construct(synchronous, b"oO");
    call(vt, b"addMethod", &[key(b"sync:"), method]);
    assert_eq!(call(target, b"sync:", &[argument]), argument);

    // An unfinished image shell cannot be made executable by native conversion.
    let source_method = call(echo, b"methodFor:", &[key(b"value:value:")]);
    let code = call(echo, b"codeObject", &[]);
    let shell = call(
        named(b"Block"),
        b"imageShellFromMethod:codeObject:",
        &[source_method, code],
    );
    let signature = q.string(b"oO").unwrap();
    for constructor in [
        b"method:withNativeSignature:".as_slice(),
        b"block:withNativeSignature:",
    ] {
        let before = epoch();
        assert!(h.send(execution, constructor, &[shell, signature]).is_err());
        assert_eq!(epoch(), before);
    }
    // The context keeps code and captures alive without a permanent Method index.
    call(gc, b"collectPrecisely", &[]);
    call(named(b"TCC"), b"checkpoint", &[]);
    let units = || as_size(call(named(b"TCC"), b"liveNativeUnits", &[])).unwrap();
    let baseline = units();
    {
        let local = h.scope();
        let text = local
            .string(b"let capture := [Cell with: 53]; [:self | [capture value]]")
            .unwrap();
        let body = local.call(named(b"Codegen"), b"eval:", &[text]).unwrap();
        local.check_result(body).unwrap();
        let sig = local.string(b"o").unwrap();
        let method = local
            .call(execution, b"method:withNativeSignature:", &[body, sig])
            .unwrap();
        local
            .call(vt, b"addMethod", &[key(b"retained"), method])
            .unwrap();
    }
    call(gc, b"collectPrecisely", &[]);
    assert_eq!(units(), baseline + 1);
    let result = call(target, b"retained", &[]);
    assert_eq!(q.word_bits(call(result, b"word", &[])).unwrap(), 53);
    unsafe { h.id.send(vt, key(b"removeMethod:"), &[key(b"retained")]) }.unwrap();
    call(gc, b"collectPrecisely", &[]);
    call(named(b"TCC"), b"checkpoint", &[]);
    assert_eq!(units(), baseline);
}
