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
    for name in [b"direct:".as_slice(), b"wrapped:", b"twice:"] {
        let method = call(
            call(module, b"methodAt:", &[key(name)]),
            b"withContext:",
            &[module],
        );
        call(vt, b"addMethod", &[key(name), method]);
    }
    let globals = named(b"Globals");
    call(globals, b"at:put:", &[key(b"AnswerForwarder"), object]);
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
        eprintln!("answer forwarding normal result: {mode} passed");
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
        call(named(b"GC"), b"collectPrecisely", &[]);
        let result = call(machine, b"resumeWith:", &[named(b"nil")]);
        assert_eq!(q.word_bits(call(result, b"word", &[])).unwrap(), 42);
        assert_eq!(
            q.word_bits(call(call(effects, b"value", &[]), b"word", &[]))
                .unwrap(),
            2
        );
        eprintln!("answer forwarding mode={mode} resumed=42 effects=2");
    }
}
