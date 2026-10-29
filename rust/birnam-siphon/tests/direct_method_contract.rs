//! Direct definitions preserve ordinary arity and DNU prefix semantics.
#[path = "support/minimal.rs"]
#[allow(dead_code)]
mod minimal;
use birnam_abi::{as_size, size, Oop};
#[test]
#[ignore = "requires native owners and build-only Lang seed"]
fn direct_methods_preserve_arity_and_dnu_without_definition_forwarders() {
    let h = minimal::native_floor();
    let q = h.scope();
    for n in ["peg", "lang_seed"] {
        h.load_native(
            &q,
            &h.resources.lib_dir.join(format!("libbirnam_{n}.dylib")),
            &[h.namespace],
        )
        .unwrap();
    }
    let named = |n: &[u8]| h.named(n).unwrap();
    let key = |n: &[u8]| q.keep(h.intern(n).unwrap()).unwrap();
    let call = |r, n: &[u8], a: &[Oop]| {
        q.call(r, n, a)
            .unwrap_or_else(|e| panic!("{}: {e:?}", String::from_utf8_lossy(n)))
    };
    let epoch = || as_size(call(h.id.root, b"bindingEpoch", &[])).unwrap();
    let nil = named(b"nil");
    let execution = named(b"Execution");
    for compiler in [b"compileSynchronous:".as_slice(), b"compileResumable:"] {
        let evaluate = |source: &[u8]| {
            let code = call(named(b"Codegen"), compiler, &[q.string(source).unwrap()]);
            let root = call(named(b"Block"), b"fromMethod:captureOperands:", &[code]);
            call(root, b"value", &[])
        };
        let body = evaluate(b"[:self :argument | 42]");
        let arity = call(named(b"Integer"), b"fromWord:", &[size(1).unwrap()]);
        let before = epoch();
        let method = call(
            named(b"Reflect"),
            b"methodForArity:implementation:",
            &[arity, body],
        );
        assert_eq!(epoch(), before);
        for (owner, selector) in [(named(b"Codegen"), b"methodTemplateForArity:".as_slice()), (method, b"withImplementation:")] {
            assert!(call(h.id.root, b"bind:to:", &[key(selector), owner]).is_null());
        }
        assert_eq!(call(execution, b"bodyOfMethod:", &[method]), body);
        assert_eq!(
            call(
                named(b"Codegen"),
                b"implementationOf:",
                &[call(method, b"captured", &[])]
            ),
            body
        );
        let target = call(named(b"Object"), b"clone", &[]);
        let vt = call(h.id.root, b"ensurePrivateVtableFor:", &[target]);
        let before = epoch();
        call(
            named(b"Reflect"),
            b"defineMethod:on:as:",
            &[key(b"answer:"), target, body],
        );
        assert_eq!(
            epoch(),
            before + 1,
            "definition changes only the requested binding"
        );
        let replacement = evaluate(b"[:block :receiver :argument | 99]");
        let replacement = call(execution, b"methodFromBlock:", &[replacement]);
        let body_vt = call(h.id.root, b"ensurePrivateVtableFor:", &[body]);
        call(body_vt, b"addMethod", &[key(b"value:value:"), replacement]);
        assert_eq!(
            q.word_bits(call(call(target, b"answer:", &[nil]), b"word", &[]))
                .unwrap(),
            42
        );
        assert_eq!(call(target, b"answer:", &[]), nil);
        assert_eq!(call(target, b"answer:", &[nil, nil]), nil);
        assert_eq!(
            call(method, b"clone", &[]),
            method,
            "cloning must not convert the calling contract"
        );
        let body = evaluate(b"[:self :selector | selector]");
        let ordinary = call(execution, b"methodFromBlock:", &[body]);
        let before = epoch();
        let handler = call(ordinary, b"forDoesNotUnderstand", &[]);
        assert_eq!(epoch(), before);
        let missing = key(b"notInstalled:with:");
        call(vt, b"addMethod", &[key(b"doesNotUnderstand:"), handler]);
        for arguments in [vec![], vec![nil], vec![target, nil], vec![nil; 16]] {
            assert_eq!(
                unsafe { h.id.send(target, missing, &arguments) }.unwrap(),
                missing
            );
        }
        // Conversion cannot change the originally captured entry's argument rule.
        let args = call(h.id.root, b"sequence", &[missing, nil]);
        assert_eq!(call(ordinary, b"invokeOn:arguments:", &[target, args]), nil);
        let before = epoch();
        let captured = call(handler, b"captured", &[]);
        assert_eq!(epoch(), before);
        call(named(b"GC"), b"collectPrecisely", &[]);
        assert_eq!(
            call(captured, b"invokeOn:arguments:", &[target, args]),
            missing
        );
        // The existing installation message performs the conversion too.
        let other = call(named(b"Object"), b"clone", &[]);
        call(
            named(b"Reflect"),
            b"defineMethod:on:as:",
            &[key(b"doesNotUnderstand:"), other, ordinary],
        );
        assert_eq!(
            unsafe { h.id.send(other, missing, &[nil, nil]) }.unwrap(),
            missing
        );
        if compiler == b"compileResumable:" {
            let machine = call(named(b"Machine"), b"new", &[]);
            let body = evaluate(b"[:self :selector | [self park]; selector]");
            let method = call(execution, b"methodFromBlock:", &[body]);
            call(
                named(b"Reflect"),
                b"defineMethod:on:as:",
                &[key(b"doesNotUnderstand:"), machine, method],
            );
            let maker = evaluate(b"[:machine | [| [machine absent: 1]]]");
            let root = call(maker, b"value:", &[machine]);
            assert_eq!(call(machine, b"runBlock:", &[root]), machine);
            assert_eq!(call(machine, b"executionState", &[]), key(b"suspended"));
            call(named(b"GC"), b"collectPrecisely", &[]);
            assert_eq!(call(machine, b"resumeWith:", &[nil]), key(b"absent:"));
        }
        eprintln!(
            "direct arity and DNU: {} passed",
            String::from_utf8_lossy(compiler)
        );
    }
}
