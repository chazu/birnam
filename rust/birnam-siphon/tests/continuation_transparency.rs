//! Necessity probe: transparent forwarding must not require private alias registration.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::Oop;
#[test]
#[ignore = "requires production Oop Execution"]
fn forwarding_an_answer_preserves_suspension_and_effects() {
    let mode = match std::env::var("BIRNAM_ANSWER_FORWARDING_CASE") {
        Ok(mode) => mode,
        Err(_) => {
            let mut failed = Vec::new();
            for mode in ["direct", "wrapped", "twice"] {
                let result = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--ignored",
                        "--exact",
                        "forwarding_an_answer_preserves_suspension_and_effects",
                        "--nocapture",
                    ])
                    .env("BIRNAM_ANSWER_FORWARDING_CASE", mode)
                    .status()
                    .unwrap();
                if !result.success() {
                    failed.push(mode);
                }
            }
            assert!(failed.is_empty(), "answer forwarding failures: {failed:?}");
            return;
        }
    };
    let h = oop::bootstrap();
    let q = h.scope();
    let key = |n: &[u8]| q.keep(h.intern(n).unwrap()).unwrap();
    let named = |n: &[u8]| h.named(n).unwrap();
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let path = std::env::temp_dir().join(format!(
        "birnam-answer-forwarding-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-continuation/transparent-answer.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let module = h.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let object = call(named(b"Object"), b"clone", &[]);
    let vt = call(h.id.root, b"ensurePrivateVtableFor:", &[object]);
    for name in [
        b"direct:".as_slice(),
        b"wrapped:",
        b"twice:",
        b"forwarded",
        b"native:",
    ] {
        let method = call(
            call(module, b"methodAt:", &[key(name)]),
            b"withContext:",
            &[module],
        );
        call(vt, b"addMethod", &[key(name), method]);
    }
    let globals = named(b"Globals");
    call(globals, b"at:put:", &[key(b"AnswerForwarder"), object]);
    let wrappers = match mode.as_str() {
        "direct" => 0,
        "wrapped" => 1,
        "twice" => 2,
        _ => unreachable!(),
    };
    let forwarded = || birnam_abi::as_size(call(object, b"forwarded", &[])).unwrap();
    for mode in [mode.as_str()] {
        let source = format!("[| [AnswerForwarder {mode}: [| 42]]]");
        let block = call(
            named(b"Codegen"),
            b"eval:",
            &[q.string(source.as_bytes()).unwrap()],
        );
        q.check_result(block).unwrap();
        let machine = call(named(b"Machine"), b"new", &[]);
        let result = call(machine, b"runBlock:", &[block]);
        assert_eq!(q.word_bits(call(result, b"word", &[])).unwrap(), 42);
        assert_eq!(forwarded(), wrappers);
        // Budget stops and collection must preserve the opaque answer chain.
        let machine = call(named(b"Machine"), b"new", &[]);
        let one = call(
            named(b"Integer"),
            b"fromWord:",
            &[birnam_abi::size(1).unwrap()],
        );
        let mut result = call(machine, b"runBlock:budget:", &[block, one]);
        let mut turns = 0;
        while result == machine {
            assert!(turns < 100);
            call(named(b"GC"), b"collectPrecisely", &[]);
            result = call(machine, b"continueWithBudget:", &[one]);
            turns += 1;
        }
        assert_eq!(q.word_bits(call(result, b"word", &[])).unwrap(), 42);
        assert_eq!(forwarded(), 2 * wrappers);
        eprintln!("answer forwarding normal and budgeted result: {mode} passed");
    }
    for mode in [mode.as_str()] {
        let machine = call(named(b"Machine"), b"new", &[]);
        call(globals, b"at:put:", &[key(b"AnswerMachine"), machine]);
        let source = format!(
            "let effects := [Cell with: 0]; let body := [| [effects inc]; [AnswerMachine park]; [effects inc]; 42]; #[effects [| [AnswerForwarder {mode}: body]]]"
        );
        let values = call(
            named(b"Codegen"),
            b"eval:",
            &[q.string(source.as_bytes()).unwrap()],
        );
        q.check_result(values).unwrap();
        let effects = q.vector_at(values, 0).unwrap();
        let root = q.vector_at(values, 1).unwrap();
        q.check_result(effects).unwrap();
        q.check_result(root).unwrap();
        let result = unsafe { h.id.send(machine, key(b"runBlock:"), &[root]) };
        if let Err(error) = result {
            q.keep(error).unwrap();
            let kind = call(error, b"faultKind", &[]);
            eprintln!(
                "answer forwarding mode={mode} failure={}",
                String::from_utf8_lossy(&q.read_bytes(kind).unwrap())
            );
        }
        let state = call(machine, b"executionState", &[]);
        let observed = q
            .word_bits(call(call(effects, b"value", &[]), b"word", &[]))
            .unwrap();
        eprintln!(
            "answer forwarding mode={mode} suspended={} effects={observed}",
            state == key(b"suspended")
        );
        assert_eq!(
            result,
            Ok(machine),
            "{mode} must park the same requested computation"
        );
        assert_eq!(state, key(b"suspended"), "{mode} must preserve suspension");
        assert_eq!(observed, 1);
        assert_eq!(
            forwarded(),
            2 * wrappers,
            "suspension must not answer the call"
        );
        call(named(b"GC"), b"collectPrecisely", &[]);
        let result = call(machine, b"resumeWith:", &[named(b"nil")]);
        assert_eq!(q.word_bits(call(result, b"word", &[])).unwrap(), 42);
        assert_eq!(
            q.word_bits(call(call(effects, b"value", &[]), b"word", &[]))
                .unwrap(),
            2
        );
        assert_eq!(
            forwarded(),
            3 * wrappers,
            "every wrapper must execute exactly once"
        );
        eprintln!("answer forwarding mode={mode} resumed=42 effects=2");

        // Exiting a protected body abandons its pending answer wrappers.
        for outside in [false, true] {
            let before = forwarded();
            let machine = call(named(b"Machine"), b"new", &[]);
            call(globals, b"at:put:", &[key(b"AnswerMachine"), machine]);
            let body = if outside {
                format!(
                    "[AnswerForwarder {mode}: [| [AnswerMachine protectedBlock: [| [AnswerMachine leaveBoundary: 41]]]]]"
                )
            } else {
                format!(
                    "[AnswerMachine protectedBlock: [| [AnswerForwarder {mode}: [| [AnswerMachine leaveBoundary: 41]]]]]"
                )
            };
            let source = format!("[| [{body} + 1]]");
            let block = call(
                named(b"Codegen"),
                b"eval:",
                &[q.string(source.as_bytes()).unwrap()],
            );
            q.check_result(block).unwrap();
            let result = call(machine, b"runBlock:", &[block]);
            assert_eq!(q.word_bits(call(result, b"word", &[])).unwrap(), 42);
            assert_eq!(forwarded(), before + if outside { wrappers } else { 0 });
        }

        // Cancellation ingress can abandon a budget-stopped wrapped body.
        {
            let before = forwarded();
            let machine = call(named(b"Machine"), b"new", &[]);
            call(globals, b"at:put:", &[key(b"AnswerMachine"), machine]);
            let source = format!(
                "let effects := [Cell with: 0]; #[effects [| [AnswerMachine protectedBlock: [| [AnswerForwarder {mode}: [| [effects inc]; [effects inc]; 99]]]]]]"
            );
            let values = call(
                named(b"Codegen"),
                b"eval:",
                &[q.string(source.as_bytes()).unwrap()],
            );
            q.check_result(values).unwrap();
            let effects = q.vector_at(values, 0).unwrap();
            let block = q.vector_at(values, 1).unwrap();
            let one = call(
                named(b"Integer"),
                b"fromWord:",
                &[birnam_abi::size(1).unwrap()],
            );
            let mut result = call(machine, b"runBlock:budget:", &[block, one]);
            let count = || {
                q.word_bits(call(call(effects, b"value", &[]), b"word", &[]))
                    .unwrap()
            };
            let mut turns = 0;
            while count() == 0 {
                assert_eq!(result, machine);
                assert!(turns < 100);
                result = call(machine, b"continueWithBudget:", &[one]);
                turns += 1;
            }
            assert_eq!(count(), 1);
            assert_eq!(result, machine);
            let answer = call(named(b"Object"), b"clone", &[]);
            assert_eq!(call(machine, b"exitRunnableBoundary:", &[answer]), machine);
            while result == machine {
                assert!(turns < 200);
                call(named(b"GC"), b"collectPrecisely", &[]);
                result = call(machine, b"continueWithBudget:", &[one]);
                turns += 1;
            }
            assert_eq!(result, answer);
            assert_eq!(count(), 1);
            assert_eq!(forwarded(), before);
        }

        // A detached continuation must retain opaque answers and pending calls,
        // even though restoring it creates new arena handle generations.
        {
            use birnam_artifact_codec::stackless_selection::{
                self, Disposition, ImplementationKind, RootKind, SelectionSpec,
            };
            let descriptor = std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/stackless-s2/frame-descriptor-call.bin"),
            )
            .unwrap();
            let directory = 64 + 8 * 32;
            let offset = u64::from_le_bytes(
                descriptor[directory + 16..directory + 24]
                    .try_into()
                    .unwrap(),
            ) as usize;
            let revision = descriptor[offset..offset + 32].try_into().unwrap();
            let selected = stackless_selection::encode_artifact_selection(
                &[revision],
                &[SelectionSpec {
                    source_revision: revision,
                    root_kind: RootKind::MethodOperation,
                    root_index: 0,
                    path: &[],
                    implementation_kind: ImplementationKind::NamedAdapter,
                    disposition: Disposition::ResumableRequired,
                }],
            )
            .unwrap();
            let step = call(
                call(module, b"methodAt:", &[key(b"captureStep")]),
                b"withContext:",
                &[named(b"Integer")],
            );
            let method = call(
                named(b"Execution"),
                b"fromStep:descriptor:selection:owner:",
                &[
                    step,
                    q.string(&descriptor).unwrap(),
                    q.string(&selected).unwrap(),
                    module,
                ],
            );
            let empty = call(named(b"Vector"), b"new", &[]);
            let block = call(named(b"Block"), b"fromMethod:captures:", &[method, empty]);
            call(globals, b"at:put:", &[key(b"AnswerCapture"), block]);
            let source = format!("[| [AnswerForwarder {mode}: AnswerCapture]]");
            let root = call(
                named(b"Codegen"),
                b"eval:",
                &[q.string(source.as_bytes()).unwrap()],
            );
            q.check_result(root).unwrap();
            let before = forwarded();
            let machine = call(named(b"Machine"), b"new", &[]);
            let continuation = call(machine, b"runBlock:", &[root]);
            assert_eq!(call(machine, b"executionState", &[]), key(b"suspended"));
            assert_eq!(forwarded(), before);
            call(named(b"GC"), b"collectPrecisely", &[]);
            let answer = call(named(b"Object"), b"clone", &[]);
            assert_eq!(
                call(
                    machine,
                    b"restoreContinuation:with:",
                    &[continuation, answer]
                ),
                answer
            );
            assert_eq!(forwarded(), before + wrappers);
            assert!(
                h.send(
                    machine,
                    b"restoreContinuation:with:",
                    &[continuation, answer]
                )
                .is_err()
            );
        }

        // Forwarding cannot make a real native callback suspendable.
        let machine = call(named(b"Machine"), b"new", &[]);
        call(globals, b"at:put:", &[key(b"AnswerMachine"), machine]);
        let source = format!(
            "[| [AnswerForwarder {mode}: [| [AnswerForwarder native: [| [AnswerMachine park]]]]]]"
        );
        let block = call(
            named(b"Codegen"),
            b"eval:",
            &[q.string(source.as_bytes()).unwrap()],
        );
        q.check_result(block).unwrap();
        assert!(unsafe { h.id.send(machine, key(b"runBlock:"), &[block]) }.is_err());
        assert_eq!(call(machine, b"executionState", &[]), key(b"failed"));
    }
}
