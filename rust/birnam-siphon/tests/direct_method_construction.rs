//! Direct compiled Method construction: receiver, environment and code have distinct roles.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};

#[test]
#[ignore = "requires production Oop Execution"]
fn direct_methods_bind_receivers_and_environments_without_forwarding_or_binding_edits() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let key = |n: &[u8]| hooks.symbol(n);
    let named = |n: &[u8]| h.named(n).unwrap();
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let eval = |text: &[u8]| {
        let result = call(named(b"Codegen"), b"eval:", &[q.string(text).unwrap()]);
        q.check_result(result).unwrap();
        result
    };
    let execution = named(b"Execution");
    let gc = named(b"GC");
    let values = eval(
        b"let captured := [Cell with: 5]; #[captured [:self :arg | #[self [captured value] arg]]]",
    );
    let cell = q.vector_at(values, 0).unwrap();
    let body = q.vector_at(values, 1).unwrap();
    let body_code = call(body, b"methodFor:", &[key(b"value:value:")]);
    let body_description = call(body, b"codeObject", &[]);
    let shell_code = call(body, b"methodFor:", &[key(b"value:value:")]);
    let epoch = || as_size(call(h.id.root, b"bindingEpoch", &[])).unwrap();
    let before = epoch();
    let method = call(execution, b"methodFromBlock:", &[body]);
    let sibling = call(execution, b"methodFromBlock:", &[body]);
    assert_eq!(
        epoch(),
        before,
        "binding an environment must not edit dispatch dictionaries"
    );
    assert_ne!(method, sibling);
    assert_eq!(
        call(h.id.root, b"behaviorOf:", &[method]),
        call(h.id.root, b"methodBehavior", &[])
    );

    let object = call(named(b"Object"), b"clone", &[]);
    let behavior = call(h.id.root, b"ensurePrivateVtableFor:", &[object]);
    let reflect = named(b"Reflect");
    let reflect_vt = call(h.id.root, b"behaviorOf:", &[reflect]);
    let constructor = key(b"methodForArity:implementation:");
    let original_constructor = call(reflect_vt, b"lookup", &[constructor]);
    let forbidden = hooks.context(&[hooks.fault(918), size(0).unwrap()]);
    call(
        reflect_vt,
        b"addMethod",
        &[constructor, hooks.method(b"deny", forbidden)],
    );
    let method_behavior = call(h.id.root, b"behaviorOf:", &[method]);
    let private = call(h.id.root, b"ensurePrivateVtableFor:", &[method]);
    call(private, b"addMethod", &[key(b"isError"), hooks.method(b"deny", forbidden)]);
    let before = epoch();
    assert_eq!(
        call(
            reflect,
            b"defineMethod:on:as:",
            &[key(b"answer:"), object, method]
        ),
        object
    );
    assert_eq!(
        epoch(),
        before + 1,
        "installation adds only the requested binding"
    );
    assert_eq!(call(behavior, b"lookup", &[key(b"answer:")]), method);
    assert_eq!(
        hooks.count(forbidden, 1),
        0,
        "an executable must not pass through body construction"
    );
    call(
        reflect_vt,
        b"addMethod",
        &[constructor, original_constructor],
    );
    call(h.id.root, b"reassociateBehaviorOf:with:", &[method, method_behavior]);
    let argument = call(named(b"Object"), b"clone", &[]);
    // Poison the source body's dispatch. The new Method must enter its code,
    // not send value: or query captures on this former Block receiver.
    let body_behavior = call(h.id.root, b"ensurePrivateVtableFor:", &[body]);
    let failed = hooks.context(&[hooks.fault(915), size(0).unwrap()]);
    for name in [b"value:value:".as_slice(), b"captureWordAt:"] {
        call(
            body_behavior,
            b"addMethod",
            &[key(name), hooks.method(b"deny", failed)],
        );
    }
    let result = call(object, b"answer:", &[argument]);
    assert_eq!(q.vector_at(result, 0).unwrap(), object);
    assert_eq!(
        q.word_bits(call(q.vector_at(result, 1).unwrap(), b"word", &[]))
            .unwrap(),
        5
    );
    assert_eq!(q.vector_at(result, 2).unwrap(), argument);
    assert_eq!(hooks.count(failed, 1), 0);
    let next = eval(b"19");
    call(cell, b"set:", &[next]);
    call(gc, b"collectPrecisely", &[]);
    let result = call(object, b"answer:", &[argument]);
    assert_eq!(
        q.vector_at(result, 1).unwrap(),
        next,
        "captured Cells remain live"
    );
    assert_eq!(hooks.count(failed, 1), 0);

    // Future sends see replacement; the selected Method retains its meaning.
    let captured = call(object, b"methodFor:", &[key(b"answer:")]);
    let replacement_body = eval(b"[:self :arg | arg]");
    let replacement = call(execution, b"methodFromBlock:", &[replacement_body]);
    call(behavior, b"addMethod", &[key(b"answer:"), replacement]);
    assert_eq!(call(object, b"answer:", &[argument]), argument);
    let arguments = call(named(b"Vector"), b"new", &[]);
    let arguments = call(arguments, b"add:", &[argument]);
    let old = call(captured, b"invokeOn:arguments:", &[object, arguments]);
    assert_eq!(q.vector_at(old, 0).unwrap(), object);
    assert_eq!(q.vector_at(old, 1).unwrap(), next);

    // Basic construction does not query optional source/CodeObject metadata.
    let metadata_free = call(
        named(b"Block"),
        b"fromMethod:captureOperands:",
        &[body_code, cell],
    );
    let code_behavior = call(h.id.root, b"ensurePrivateVtableFor:", &[body_code]);
    call(
        code_behavior,
        b"addMethod",
        &[key(b"codeObject"), hooks.method(b"deny", failed)],
    );
    let before = epoch();
    let plain = call(execution, b"methodFromBlock:", &[metadata_free]);
    assert_eq!(epoch(), before);
    call(behavior, b"addMethod", &[key(b"plain:"), plain]);
    let answer = call(object, b"plain:", &[argument]);
    assert_eq!(q.vector_at(answer, 0).unwrap(), object);
    assert_eq!(q.vector_at(answer, 1).unwrap(), next);
    assert_eq!(hooks.count(failed, 1), 0);

    // The retained inline candidate path can enter or decline a bound Method.
    let globals = named(b"Globals");
    call(globals, b"at:put:", &[key(b"DirectObject"), object]);
    call(globals, b"at:put:", &[key(b"DirectArgument"), argument]);
    let root = eval(b"[| let answer := [DirectObject optimized: DirectArgument]; #[answer]]");
    for candidate_answer in [answer, std::ptr::null_mut()] {
        let count = hooks.context(&[candidate_answer, size(0).unwrap()]);
        let candidate = hooks.method(b"fixedCount", count);
        let slot = call(named(b"Integer"), b"fromWord:", &[size(1).unwrap()]);
        let optimized = call(plain, b"withInline:at:owner:", &[candidate, slot, object]);
        call(behavior, b"addMethod", &[key(b"optimized:"), optimized]);
        let machine = call(named(b"Machine"), b"new", &[]);
        let result = call(machine, b"runBlock:", &[root]);
        let tuple = q.vector_at(result, 0).unwrap();
        assert_eq!(q.vector_at(tuple, 0).unwrap(), object);
        assert_eq!(q.vector_at(tuple, 1).unwrap(), next);
        assert_eq!(q.vector_at(tuple, 2).unwrap(), argument);
        assert_eq!(hooks.count(count, 1), 1);
    }

    // Construction checks actual executable ownership and capture shape.
    let no_receiver = eval(b"[| 42]");
    let native = hooks.method(b"fixed", argument);
    let native_block = call(named(b"Block"), b"fromMethod:captureOperands:", &[native]);
    let shell = call(
        named(b"Block"),
        b"imageShellFromMethod:codeObject:",
        &[shell_code, body_description],
    );
    for rejected in [object, no_receiver, native_block, shell] {
        let before = epoch();
        assert!(h.send(execution, b"methodFromBlock:", &[rejected]).is_err());
        assert_eq!(epoch(), before);
    }
    let source = eval(b"let x := 3; [:self | x]");
    let source_method = call(source, b"methodFor:", &[key(b"value:")]);
    let missing = call(
        named(b"Block"),
        b"fromMethod:captureOperands:",
        &[source_method],
    );
    let extra = call(
        named(b"Block"),
        b"fromMethod:captureOperands:",
        &[source_method, argument, argument],
    );
    let null_capture = call(
        named(b"Block"),
        b"fromMethod:captureOperands:",
        &[source_method, std::ptr::null_mut()],
    );
    for rejected in [missing, extra, null_capture] {
        let before = epoch();
        assert!(h.send(execution, b"methodFromBlock:", &[rejected]).is_err());
        assert_eq!(epoch(), before);
    }

    // Two executions share code but retain separate receivers and pending calls.
    let body = eval(b"[:self :arg | [self park]; arg]");
    let method = call(execution, b"methodFromBlock:", &[body]);
    let globals = named(b"Globals");
    let mut running = Vec::new();
    for name in [b"DirectMachineOne".as_slice(), b"DirectMachineTwo"] {
        let machine = call(named(b"Machine"), b"new", &[]);
        let vt = call(h.id.root, b"ensurePrivateVtableFor:", &[machine]);
        call(vt, b"addMethod", &[key(b"answer:"), method]);
        call(globals, b"at:put:", &[key(name), machine]);
        let source = format!(
            "[| [{} answer: {}]]",
            std::str::from_utf8(name).unwrap(),
            running.len() + 41
        );
        let root = eval(source.as_bytes());
        assert_eq!(call(machine, b"runBlock:", &[root]), machine);
        assert_eq!(call(machine, b"executionState", &[]), key(b"suspended"));
        running.push(machine);
    }
    call(gc, b"collectPrecisely", &[]);
    for (i, machine) in running.into_iter().enumerate() {
        let result = call(machine, b"resumeWith:", &[named(b"nil")]);
        assert_eq!(q.word_bits(call(result, b"word", &[])).unwrap(), i + 41);
    }
    // After source roots disappear, the installed Method alone retains its
    // environment and code. Removing it releases the mapping again.
    call(gc, b"collectPrecisely", &[]);
    call(named(b"TCC"), b"checkpoint", &[]);
    let units = || as_size(call(named(b"TCC"), b"liveNativeUnits", &[])).unwrap();
    let baseline = units();
    {
        let local = h.scope();
        let text = local
            .string(b"let captured := [Cell with: 37]; [:self | [captured value]]")
            .unwrap();
        let body = local.call(named(b"Codegen"), b"eval:", &[text]).unwrap();
        local.check_result(body).unwrap();
        let method = local.call(execution, b"methodFromBlock:", &[body]).unwrap();
        local
            .call(behavior, b"addMethod", &[key(b"soleOwner"), method])
            .unwrap();
    }
    call(gc, b"collectPrecisely", &[]);
    assert_eq!(units(), baseline + 1);
    let result = call(object, b"soleOwner", &[]);
    assert_eq!(q.word_bits(call(result, b"word", &[])).unwrap(), 37);
    unsafe {
        h.id.send(behavior, key(b"removeMethod:"), &[key(b"soleOwner")])
    }
    .unwrap();
    call(gc, b"collectPrecisely", &[]);
    call(named(b"TCC"), b"checkpoint", &[]);
    assert_eq!(units(), baseline);
}
