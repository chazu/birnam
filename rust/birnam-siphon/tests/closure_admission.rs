//! Portable source association belongs to the compiled code's constructor.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{as_size, size, Oop};
use birnam_artifact_codec::portable_program::{encode, NodeSpec, ProgramSpec, StatementSpec};

#[test]
#[ignore = "requires production Oop compiler and Execution"]
fn closure_admission_preserves_checked_counts_and_code_identity() {
    let host = oop::bootstrap();
    let q = host.scope();
    let hooks = oop::hooks::Hooks::new(&host);
    let named = |n: &[u8]| host.named(n).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let raw = |r, n: &[u8], a: &[Oop]| unsafe { host.id.send(r, key(n), a) };
    let code = |captures: &[&[u8]], value| {
        let portable = encode(ProgramSpec {
            parameters: &[],
            captures,
            lets: &[],
            statements: &[StatementSpec::Expression { root_node: 0 }],
            nodes: &[NodeSpec::Integer(value)],
        })
        .unwrap();
        let bytes = call(
            named(b"Bytes"),
            b"fromBytes:",
            &[q.byte_values(&portable).unwrap()],
        );
        call(named(b"CodeObject"), b"fromCanonicalBytes:", &[bytes])
    };
    let vector = |values: &[Oop]| {
        let mut v = call(named(b"Vector"), b"new", &[]);
        for &value in values {
            v = call(v, b"add:", &[value]);
        }
        v
    };
    let epoch = || as_size(call(host.id.root, b"bindingEpoch", &[])).unwrap();
    let owner = named(b"Block");
    let nil = named(b"nil");
    let failure = hooks.fault(881);
    let denial = hooks.context(&[failure, size(0).unwrap()]);
    let deny = hooks.method(b"deny", denial);
    let native = hooks.method(b"fixed", nil);
    let description = code(&[], 42);
    for selector in [
        b"admitCodeObject:".as_slice(),
        b"admitCodeObject:synchronous:",
    ] {
        assert!(call(host.id.root, b"bind:to:", &[key(selector), native]).is_null());
    }
    // Native code requires no portable source. A replaced reflection getter or
    // a reinstated admission selector cannot authorize an unrelated program.
    let plain = call(owner, b"fromMethod:captureOperands:", &[native]);
    assert_eq!(call(plain, b"value", &[]), nil);
    assert_eq!(call(plain, b"codeObject", &[]), nil);
    let behavior = call(host.id.root, b"ensurePrivateVtableFor:", &[native]);
    call(
        behavior,
        b"addMethod",
        &[key(b"codeObject"), hooks.method(b"fixed", description)],
    );
    call(behavior, b"addMethod", &[key(b"admitCodeObject:"), deny]);
    let empty = vector(&[]);
    let before = epoch();
    assert!(raw(
        owner,
        b"fromMethod:codeObject:captures:",
        &[native, description, empty]
    )
    .is_err());
    assert_eq!(hooks.count(denial, 1), 0);
    assert_eq!(epoch(), before);
    assert_eq!(call(plain, b"value", &[]), nil);

    for materialize in [
        b"materializeSynchronous:captures:".as_slice(),
        b"materializeResumable:captures:",
    ] {
        let source = code(&[b"a", b"b"], 42);
        let original = call(
            named(b"Codegen"),
            materialize,
            &[source, vector(&[nil, nil])],
        );
        q.check_result(original).unwrap();
        let method = call(original, b"methodFor:", &[key(b"value")]);
        assert_eq!(call(method, b"codeObject", &[]), source);
        let factory_vt = call(host.id.root, b"behaviorOf:", &[named(b"CodeObject")]);
        let mut saved = Vec::new();
        for selector in [b"parameterCountOf:".as_slice(), b"captureCountOf:"] {
            let selector = key(selector);
            saved.push((selector, call(factory_vt, b"lookup", &[selector])));
            call(factory_vt, b"addMethod", &[selector, deny]);
        }
        let before = epoch();
        for _ in 0..2 {
            let block = call(
                owner,
                b"fromMethod:captureOperands:",
                &[method, nil, size(7).unwrap()],
            );
            assert_eq!(
                call(block, b"captureWordAt:", &[size(1).unwrap()]),
                size(7).unwrap()
            );
            assert_eq!(call(block, b"codeObject", &[]), source);
        }
        for count in [0, 1, 3, 4097] {
            let mut fields = vec![method];
            fields.extend(std::iter::repeat_n(nil, count));
            assert!(raw(owner, b"fromMethod:captureOperands:", &fields).is_err());
        }
        assert!(raw(
            owner,
            b"fromMethod:captureOperands:",
            &[method, nil, std::ptr::null_mut()]
        )
        .is_err());
        assert_eq!(epoch(), before);
        assert_eq!(
            hooks.count(denial, 1),
            0,
            "checked code must not query its shape again"
        );
        for (selector, original) in saved {
            call(factory_vt, b"addMethod", &[selector, original]);
        }

        let equal = code(&[b"a", b"b"], 42);
        let wrong_program = code(&[b"a", b"b"], 43);
        let wrong_shape = code(&[b"a"], 42);
        let captures = vector(&[nil, nil]);
        let before = epoch();
        let paired = call(
            owner,
            b"fromMethod:codeObject:captures:",
            &[method, equal, captures],
        );
        assert_eq!(call(paired, b"codeObject", &[]), equal);
        assert_eq!(call(method, b"codeObject", &[]), source);
        assert!(raw(
            owner,
            b"fromMethod:codeObject:captures:",
            &[method, wrong_program, captures]
        )
        .is_err());
        assert!(raw(
            owner,
            b"fromMethod:codeObject:captures:",
            &[method, wrong_shape, vector(&[nil])]
        )
        .is_err());
        assert_eq!(epoch(), before);
        call(named(b"GC"), b"collectPrecisely", &[]);
        assert_eq!(
            q.word_bits(call(call(paired, b"value", &[]), b"word", &[]))
                .unwrap(),
            42
        );
    }
}
