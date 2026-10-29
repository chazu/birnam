//! Direct definitions retain live lookup, captured meaning and stackless control.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{size, Oop};

#[test]
#[ignore = "requires production Oop modules"]
fn direct_definitions_preserve_arguments_lookup_and_entered_calls() {
    let h = oop::bootstrap();
    let q = h.scope();
    let named = |n: &[u8]| h.named(n).unwrap();
    let key = |n: &[u8]| q.keep(h.intern(n).unwrap()).unwrap();
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let eval = |source: &[u8]| {
        let v = call(named(b"Codegen"), b"eval:", &[q.string(source).unwrap()]);
        q.check_result(v).unwrap();
        v
    };
    let define = |target, selector: &[u8], body| {
        assert_eq!(
            call(
                named(b"Reflect"),
                b"defineMethod:on:as:",
                &[key(selector), target, body]
            ),
            target
        );
    };
    let target = call(named(b"Object"), b"clone", &[]);
    let argument = call(named(b"Object"), b"clone", &[]);
    for compiler in [b"compileSynchronous:".as_slice(), b"compileResumable:"] {
        for arity in [0, 1, 2, 16] {
            let names = (0..arity).map(|i| format!("a{i}")).collect::<Vec<_>>();
            let parameters = names.iter().map(|n| format!(" :{n}")).collect::<String>();
            let source = format!(
                "[:self{parameters} | [System gc]; #[self {}]]",
                names.join(" ")
            );
            let code = call(
                named(b"Codegen"),
                compiler,
                &[q.string(source.as_bytes()).unwrap()],
            );
            q.check_result(code).unwrap();
            let root = call(named(b"Block"), b"fromMethod:captureOperands:", &[code]);
            let body = call(root, b"value", &[]);
            let selector = if arity == 0 {
                "probe".to_string()
            } else {
                "part:".repeat(arity)
            };
            let arguments = (0..arity)
                .map(|_| call(named(b"Object"), b"clone", &[]))
                .collect::<Vec<_>>();
            define(target, selector.as_bytes(), body);
            let captured = call(target, b"methodFor:", &[key(selector.as_bytes())]);
            let result = call(target, selector.as_bytes(), &arguments);
            assert_eq!(q.vector_at(result, 0).unwrap(), target);
            for (i, &argument) in arguments.iter().enumerate() {
                assert_eq!(q.vector_at(result, i + 1).unwrap(), argument);
            }
            let replacement = eval(format!("[:self{parameters} | #replacement]").as_bytes());
            define(target, selector.as_bytes(), replacement);
            assert_eq!(
                call(target, selector.as_bytes(), &arguments),
                key(b"replacement")
            );
            let packet = call(h.id.root, b"sequence", &arguments);
            let result = call(captured, b"invokeOn:arguments:", &[target, packet]);
            assert_eq!(q.vector_at(result, 0).unwrap(), target);
            for (i, &argument) in arguments.iter().enumerate() {
                assert_eq!(q.vector_at(result, i + 1).unwrap(), argument);
            }
        }
    }

    define(target, b"live:", eval(b"[:self :argument | #old]"));
    let selected = call(target, b"methodFor:", &[key(b"live:")]);
    define(target, b"live:", eval(b"[:self :argument | #new]"));
    let behavior = call(h.id.root, b"behaviorOf:", &[target]);
    let original_meta = call(h.id.root, b"behaviorOf:", &[behavior]);
    let original_lookup = call(h.id.root, b"bind:to:", &[key(b"lookup"), behavior]);
    let meta = call(original_meta, b"delegated", &[]);
    let count = eval(b"[Cell with: 0]");
    let factory = eval(b"[:saved :wanted :selected :count | [:table :selector | [[selector == wanted] ifTrue: [| [count inc]; [System gc]; selected] ifFalse: [| [saved invokeOn: table arguments: #[selector]]]]]]");
    let body = call(
        factory,
        b"value:value:value:value:",
        &[original_lookup, key(b"live:"), selected, count],
    );
    let lookup = call(named(b"Execution"), b"methodFromBlock:", &[body]);
    call(meta, b"addMethod", &[key(b"lookup"), lookup]);
    call(h.id.root, b"reassociateBehaviorOf:with:", &[behavior, meta]);
    assert_eq!(call(target, b"live:", &[argument]), key(b"old"));
    assert_eq!(
        q.word_bits(call(call(count, b"value", &[]), b"word", &[]))
            .unwrap(),
        1
    );
    call(
        h.id.root,
        b"reassociateBehaviorOf:with:",
        &[behavior, original_meta],
    );
    assert_eq!(call(target, b"live:", &[argument]), key(b"new"));

    let error = call(named(b"Error"), b"kind:", &[key(b"ordinaryErrorData")]);
    let factory = eval(b"[:value | [:self :argument | value]]");
    define(target, b"data:", call(factory, b"value:", &[error]));
    assert_eq!(call(target, b"data:", &[argument]), error);

    let machine = call(named(b"Machine"), b"new", &[]);
    let effects = eval(b"[Cell with: 0]");
    let factory = eval(b"[:machine :effects | [:self :argument | [effects inc]; [machine park]; [effects inc]; #[self argument #entered]]]");
    define(
        target,
        b"wait:",
        call(factory, b"value:value:", &[machine, effects]),
    );
    let caller = eval(b"[:target :argument | [| [target wait: argument]]]");
    let root = call(caller, b"value:value:", &[target, argument]);
    assert_eq!(call(machine, b"runBlock:", &[root]), machine);
    assert_eq!(call(machine, b"executionState", &[]), key(b"suspended"));
    assert_eq!(
        q.word_bits(call(call(effects, b"value", &[]), b"word", &[]))
            .unwrap(),
        1
    );
    define(target, b"wait:", eval(b"[:self :argument | #replacement]"));
    call(named(b"GC"), b"collectPrecisely", &[]);
    let result = call(machine, b"resumeWith:", &[named(b"nil")]);
    assert_eq!(q.vector_at(result, 0).unwrap(), target);
    assert_eq!(q.vector_at(result, 1).unwrap(), argument);
    assert_eq!(q.vector_at(result, 2).unwrap(), key(b"entered"));
    assert_eq!(
        q.word_bits(call(call(effects, b"value", &[]), b"word", &[]))
            .unwrap(),
        2
    );
    assert_eq!(call(target, b"wait:", &[argument]), key(b"replacement"));
    assert!(h
        .send(machine, b"resumeWith:", &[size(7).unwrap()])
        .is_err());
}
