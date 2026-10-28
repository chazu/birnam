//! Explicit Word plans preserve current permissions and generated arithmetic meanings.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
#[test]
#[ignore = "requires production Oop Word owner and compiler"]
fn word_expressions_observe_current_permissions_and_primitives() {
    let host = oop::bootstrap();
    let q = host.scope();
    let hooks = oop::hooks::Hooks::new(&host);
    let key = |n: &[u8]| hooks.symbol(n);
    let named = |n: &[u8]| host.named(n).unwrap();
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let word = named(b"Word");
    let vt = named(b"WordVt");
    let gc = named(b"GC");
    let model = named(b"ObjectModel");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let a = call(word, b"fromBits:", &[size(10).unwrap()]);
    let b = call(word, b"fromBits:", &[size(6).unwrap()]);
    let changed = hooks.context(&[a, size(0).unwrap()]);
    let replacement = hooks.method(b"fixedCount", changed);
    let state = hooks.context(&[
        vt,
        gc,
        size(1).unwrap(),
        size(0).unwrap(),
        replacement,
        size(0).unwrap(),
        size(0).unwrap(),
    ]);
    let path = std::env::temp_dir().join(format!(
        "birnam-word-expression-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/word-expression.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let module = host.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let method = |n: &[u8]| {
        call(
            call(module, b"methodAt:", &[key(n)]),
            b"withContext:",
            &[state],
        )
    };
    let observer = call(named(b"Object"), b"clone", &[]);
    let ovt = call(host.id.root, b"ensurePrivateVtableFor:", &[observer]);
    for n in [b"allocation:bytes:identity:".as_slice(), b"words"] {
        call(ovt, b"addMethod", &[key(n), method(n)]);
    }
    let count = || as_size(call(observer, b"words", &[])).unwrap();
    let bytes = |v: &[u8]| call(named(b"Bytes"), b"fromBytes:", &[q.byte_values(v).unwrap()]);
    let mvt = call(host.id.root, b"behaviorOf:", &[model]);
    let xor = call(vt, b"lookup", &[key(b"bitXor:")]);
    let allocate = call(mvt, b"lookup", &[key(b"allocate:bytes:")]);
    let permission = call(mvt, b"lookup", &[key(b"canOmitAllocationFor:")]);
    call(
        mvt,
        b"addMethod",
        &[key(b"expressionOriginalAllocation:bytes:"), allocate],
    );
    call(
        mvt,
        b"addMethod",
        &[key(b"expressionOriginalPermission:"), permission],
    );
    let allocation_observer = method(b"allocation");
    let permission_observer = method(b"permit");
    let set = |v, n: &[u8], m| {
        call(v, b"addMethod", &[key(n), m]);
    };
    let expression = key(b"tryPrimitiveExpression:");
    let epoch_expression = key(b"tryPrimitiveExpressionAtEpoch:");
    let epoch = key(b"behaviorEpoch");
    let raw = |r, s, a: &[Oop]| unsafe { host.id.send(r, s, a) };
    for shared in [false, true] {
        let source = if shared {
            br#"let Probe := [Object clone]; [Reflect defineMethod: #finish: on: Probe as: [Execution block: [:self :value | value] withNativeSignature: "oO"]]; [Reflect defineMethod: #compute:with: on: Probe as: [Execution block: [:self :a :b | let xor := [a bitXor: b]; let sum := [xor wrappingAdd: a]; let masked := [sum bitAnd: xor]; [self finish: [masked wrappingAdd: a]]] withNativeSignature: "oOO"]]; Probe"#.as_slice()
        } else {
            br#"let Probe := [Object clone]; [Reflect defineMethod: #compute:with: on: Probe as: [Execution block: [:self :a :b | let xor := [a bitXor: b]; let sum := [xor wrappingAdd: a]; let masked := [sum bitAnd: b]; [masked wrappingAdd: a]] withNativeSignature: "oOO"]]; Probe"#.as_slice()
        };
        let probe = call(named(b"Codegen"), b"eval:", &[q.string(source).unwrap()]);
        q.check_result(probe).unwrap();
        let run = || call(probe, b"compute:with:", &[a, b]);
        let expected = if shared { 14 } else { 16 };
        let replaced = if shared { 10 } else { 14 };
        let program = bytes(&[
            1,
            2,
            4,
            5,
            13,
            0,
            1,
            6,
            2,
            0,
            11,
            3,
            if shared { 2 } else { 1 },
            6,
            4,
            0,
        ]);
        let args = [program, a, b];
        call(gc, b"collectPrecisely", &[]);
        let before = count();
        let result = run();
        assert_eq!(q.word_bits(result).unwrap(), expected);
        eprintln!(
            "generated Word allocations: shared={shared} words={}",
            count() - before
        );
        let another = run();
        assert_ne!(result, another);
        assert_eq!(q.word_bits(another).unwrap(), expected);
        let before = count();
        let result = q.keep(raw(word, expression, &args).unwrap()).unwrap();
        assert!(!result.is_null());
        assert_eq!(q.word_bits(result).unwrap(), expected);
        assert_eq!(
            count() - before,
            1,
            "explicit expression publishes only its escaping Word"
        );
        let another = q.keep(raw(word, expression, &args).unwrap()).unwrap();
        assert_ne!(result, another);
        let stamp = raw(host.id.root, epoch, &[]).unwrap();
        assert!(
            !raw(word, epoch_expression, &[stamp, program, a, b])
                .unwrap()
                .is_null()
        );
        assert!(
            raw(
                word,
                epoch_expression,
                &[size(as_size(stamp).unwrap() + 1).unwrap(), program, a, b]
            )
            .unwrap()
            .is_null()
        );
        set(vt, b"bitXor:", replacement);
        hooks.put(changed, 1, size(0).unwrap());
        assert!(raw(word, expression, &args).unwrap().is_null());
        assert_eq!(q.word_bits(run()).unwrap(), replaced);
        assert_eq!(hooks.count(changed, 1), 1);
        set(vt, b"bitXor:", xor);
        set(mvt, b"allocate:bytes:", allocation_observer);
        hooks.put(state, 6, size(0).unwrap());
        assert!(raw(word, expression, &args).unwrap().is_null());
        assert_eq!(q.word_bits(run()).unwrap(), expected);
        assert_eq!(
            hooks.count(state, 6),
            4,
            "current allocator observes every authored Word construction"
        );
        set(mvt, b"allocate:bytes:", allocate);
        call(gc, b"collectPrecisely", &[]);
        set(mvt, b"canOmitAllocationFor:", permission_observer);
        let stamp = raw(host.id.root, epoch, &[]).unwrap();
        hooks.put(state, 2, size(0).unwrap());
        assert!(raw(word, expression, &args).unwrap().is_null());
        assert_eq!(raw(host.id.root, epoch, &[]).unwrap(), stamp);
        hooks.put(state, 2, size(1).unwrap());
        let answer = q.keep(raw(word, expression, &args).unwrap()).unwrap();
        assert_eq!(q.word_bits(answer).unwrap(), expected);
        assert_eq!(raw(host.id.root, epoch, &[]).unwrap(), stamp);
        hooks.put(state, 3, size(1).unwrap());
        hooks.put(changed, 1, size(0).unwrap());
        assert!(
            raw(word, expression, &args).unwrap().is_null(),
            "mutation during permission cannot authorize an expression using old primitive meanings"
        );
        assert_eq!(
            hooks.count(changed, 1),
            0,
            "a refused query does not run the changed primitive"
        );
        assert_eq!(q.word_bits(run()).unwrap(), replaced);
        assert_eq!(hooks.count(changed, 1), 1);
        hooks.put(state, 3, size(0).unwrap());
        set(mvt, b"canOmitAllocationFor:", permission);
        set(vt, b"bitXor:", xor);
        assert_eq!(
            q.word_bits(q.keep(raw(word, expression, &args).unwrap()).unwrap())
                .unwrap(),
            expected
        );
    }
    // Exercise the complete bounded program, with 32 inputs and 32 operations.
    let mut program = vec![1, 32, 32, 63];
    let mut args = vec![std::ptr::null_mut()];
    for i in 0..32 {
        args.push(call(word, b"fromBits:", &[size(i).unwrap()]));
        program.extend_from_slice(&[6, if i == 0 { 0 } else { (31 + i) as u8 }, i as u8]);
    }
    args[0] = bytes(&program);
    assert_eq!(
        q.word_bits(q.keep(raw(word, expression, &args).unwrap()).unwrap())
            .unwrap(),
        496
    );
    let sum = bytes(&[1, 2, 1, 2, 6, 0, 1]);
    assert_eq!(
        q.word_bits(
            q.keep(
                raw(
                    word,
                    expression,
                    &[sum, size(10).unwrap(), size(6).unwrap()]
                )
                .unwrap()
            )
            .unwrap()
        )
        .unwrap(),
        16
    );
    for bad in [
        vec![],
        vec![1, 2, 0, 1],
        vec![1, 2, 33, 2],
        vec![1, 2, 1, 2, 99, 0, 1],
        vec![1, 2, 1, 2, 6, 2, 1],
        vec![1, 2, 1, 1, 6, 0, 1],
    ] {
        assert!(raw(word, expression, &[bytes(&bad), a, b]).is_err());
    }
    assert!(raw(word, expression, &[sum, a, std::ptr::null_mut()]).is_err());
    let mut too_many = vec![sum];
    too_many.extend(std::iter::repeat_n(a, 33));
    assert!(raw(word, expression, &too_many).is_err());
}
