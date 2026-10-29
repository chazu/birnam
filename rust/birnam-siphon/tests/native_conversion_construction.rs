//! Native conversion metadata is initialized without dispatch mutations.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{as_size, size, Oop};

#[test]
#[ignore = "requires production Oop Execution"]
fn typed_blocks_bind_direct_methods_without_metadata_mutation() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let named = |n: &[u8]| h.named(n).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let epoch = || as_size(call(h.id.root, b"bindingEpoch", &[])).unwrap();
    let execution = named(b"Execution");
    let target = call(named(b"Object"), b"clone", &[]);
    let vt = call(h.id.root, b"ensurePrivateVtableFor:", &[target]);
    for compiler in [b"compileSynchronous:".as_slice(), b"compileResumable:"] {
        let source = q
            .string(b"[| let captured := 37; [:self :arg | #[self captured arg]]]")
            .unwrap();
        let code = call(named(b"Codegen"), compiler, &[source]);
        q.check_result(code).unwrap();
        let outer = call(named(b"Block"), b"fromMethod:captureOperands:", &[code]);
        let factory = call(outer, b"value", &[]);
        let body = call(factory, b"value", &[]);
        let signature = q.string(b"oO").unwrap();
        let before = epoch();
        let typed = call(execution, b"block:withNativeSignature:", &[body, signature]);
        let method = call(execution, b"methodFromBlock:", &[typed]);
        let captured = call(method, b"captured", &[]);
        let description = call(typed, b"codeObject", &[]);
        assert_ne!(description, named(b"nil"));
        for value in [method, captured] {
            assert_eq!(call(value, b"codeObject", &[]), description);
            assert_eq!(
                call(value, b"isSynchronous", &[]),
                call(body, b"isSynchronous", &[])
            );
            assert_eq!(
                call(h.id.root, b"behaviorOf:", &[value]),
                call(h.id.root, b"methodBehavior", &[])
            );
            assert_eq!(
                q.read_bytes(call(execution, b"nativeSignatureOf:", &[value]))
                    .unwrap(),
                b"oO"
            );
            assert_eq!(call(value, b"admitCodeObject:", &[description]), value);
            assert!(h
                .send(
                    value,
                    b"admitCodeObject:",
                    &[call(body, b"codeObject", &[])]
                )
                .is_err());
        }
        assert_eq!(
            epoch(),
            before,
            "construction, capture and reflection must not edit bindings"
        );
        let typed_code = call(typed, b"methodFor:", &[key(b"value:value:")]);
        for fields in [vec![typed_code], vec![typed_code, std::ptr::null_mut()]] {
            if let Ok(malformed) = h.send(named(b"Block"), b"fromMethod:captureOperands:", &fields)
            {
                q.keep(malformed).unwrap();
                assert!(h
                    .send(execution, b"methodFromBlock:", &[malformed])
                    .is_err());
            }
        }
        assert_eq!(epoch(), before);
        // Poisoning either Block's value method must not affect a bound native Method.
        let poison = hooks.context(&[hooks.fault(91), size(0).unwrap()]);
        for block in [body, typed] {
            let behavior = call(h.id.root, b"ensurePrivateVtableFor:", &[block]);
            call(
                behavior,
                b"addMethod",
                &[key(b"value:value:"), hooks.method(b"deny", poison)],
            );
        }
        call(vt, b"addMethod", &[key(b"typed:"), captured]);
        call(named(b"GC"), b"collectPrecisely", &[]);
        let result = call(target, b"typed:", &[target]);
        assert_eq!(q.vector_at(result, 0).unwrap(), target);
        assert_eq!(q.vector_at(result, 2).unwrap(), target);
        let value = q.vector_at(result, 1).unwrap();
        assert_eq!(q.word_bits(call(value, b"word", &[])).unwrap(), 37);
        assert_eq!(hooks.count(poison, 1), 0);
        assert!(h.send(target, b"typed:", &[]).is_err());
        assert!(h.send(target, b"typed:", &[std::ptr::null_mut()]).is_err());
    }
}
