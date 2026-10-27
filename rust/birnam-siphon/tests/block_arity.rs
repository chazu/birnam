//! Default Block calls check code arity while value methods remain replaceable.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::size;

#[test]
#[ignore = "requires production Oop modules"]
fn blocks_reject_wrong_arity_and_preserve_current_replacement() {
    let h = oop::bootstrap();
    let q = h.scope();
    q.call(
        h.named(b"GC").unwrap(),
        b"precisePayloads:",
        &[size(1).unwrap()],
    )
    .unwrap();
    let cg = h.named(b"Codegen").unwrap();
    for source in [b"[| 42]".as_slice(), b"[:a | a]", b"[:a :b | b]"] {
        let source = q.string(source).unwrap();
        let block = q.call(cg, b"eval:", &[source]).unwrap();
        q.check_result(block).unwrap();
        let code = q.call(block, b"codeObject", &[]).unwrap();
        let arity = q
            .call(
                h.named(b"CodeObject").unwrap(),
                b"parameterCountOf:",
                &[code],
            )
            .unwrap();
        let arity = q.word_bits(arity).unwrap();
        for count in 0..=2 {
            let name = if count == 0 {
                b"value".to_vec()
            } else {
                b"value:".repeat(count)
            };
            let selector = h.intern(&name).unwrap();
            let arguments = vec![h.named(b"true").unwrap(); count];
            let answer = unsafe { h.id.send(block, selector, &arguments) };
            assert_eq!(
                answer.is_err(),
                count != arity,
                "Block with {arity} parameters accepted {}",
                String::from_utf8_lossy(&name)
            );
        }
    }
    let source = q
        .string(
            br#"let block := [:a | a];
        let copy := [block clone];
        let answer := [Object clone];
        [System gc];
        let before := [[copy value: answer] == answer];
        [Reflect defineMethod: #value on: block as: [:self | answer]];
        [before & [[block value] == answer]]"#,
        )
        .unwrap();
    let answer = q.call(cg, b"eval:", &[source]).unwrap();
    q.check_result(answer).unwrap();
    assert_eq!(answer, h.named(b"true").unwrap());

    // A native caller of synchronous Block code receives the exact failure.
    // A language Machine handles that failure at its own pending call instead.
    let hooks = oop::hooks::Hooks::new(&h);
    let failure = hooks.fault(83);
    let failed = hooks.context(&[failure, size(0).unwrap()]);
    let callback = q.call(h.named(b"Object").unwrap(), b"clone", &[]).unwrap();
    let callback_vt = q
        .call(h.id.root, b"ensurePrivateVtableFor:", &[callback])
        .unwrap();
    q.call(
        callback_vt,
        b"addMethod",
        &[hooks.symbol(b"fail"), hooks.method(b"deny", failed)],
    )
    .unwrap();
    let globals = h.named(b"Globals").unwrap();
    let name = hooks.symbol(b"BlockFailureFixture");
    q.call(globals, b"at:put:", &[name, callback]).unwrap();
    let source = q.string(b"[BlockFailureFixture fail]").unwrap();
    let method = q.call(cg, b"compileSynchronous:", &[source]).unwrap();
    q.check_result(method).unwrap();
    let block = q
        .call(
            h.named(b"Block").unwrap(),
            b"fromMethod:captureOperands:",
            &[method],
        )
        .unwrap();
    assert_eq!(
        unsafe { h.id.send(block, hooks.symbol(b"value"), &[]) }.unwrap_err(),
        failure
    );
    let error = q.call(cg, b"eval:", &[source]).unwrap();
    assert_eq!(
        q.call(error, b"kind", &[]).unwrap(),
        hooks.symbol(b"nativeFailure")
    );
    assert_eq!(hooks.count(failed, 1), 2);
    q.call(globals, b"remove:", &[name]).unwrap();

    let source = q.string(b"42").unwrap();
    let method = q.call(cg, b"compileSynchronous:", &[source]).unwrap();
    q.check_result(method).unwrap();
    let captured = q.call(method, b"captured", &[]).unwrap();
    let block = q
        .call(
            h.named(b"Block").unwrap(),
            b"fromMethod:captureOperands:",
            &[captured],
        )
        .unwrap();
    assert!(h.send(block, b"value:", &[answer]).is_err());

    // Entry uses the shape admitted earlier; it must not require another
    // metadata query on the captured Method before entering its body.
    let context = hooks.context(&[hooks.fault(27), size(0).unwrap()]);
    let behavior = q.call(h.id.root, b"behaviorOf:", &[captured]).unwrap();
    q.call(
        behavior,
        b"addMethod",
        &[hooks.symbol(b"codeObject"), hooks.method(b"deny", context)],
    )
    .unwrap();
    let result = q.call(block, b"value", &[]).unwrap();
    assert_eq!(
        birnam_abi::as_size(q.call(result, b"word", &[]).unwrap()),
        Some(42)
    );
    assert_eq!(hooks.count(context, 1), 0);
}
