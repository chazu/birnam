//! Preserve the established admission contract while migrating closure construction.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, size};
use birnam_artifact_codec::portable_program::{NodeSpec, ProgramSpec, StatementSpec, encode};

#[test]
#[ignore = "requires production Oop compiler and Execution"]
fn closure_admission_preserves_checked_counts_and_code_identity() {
    let case = match std::env::var("BIRNAM_CLOSURE_ADMISSION_CASE") {
        Ok(case) => case,
        Err(_) => {
            let mut failures = Vec::new();
            for case in [
                "failed-admission",
                "idempotent-admission",
                "different-code",
                "direct-synchronous",
                "direct-resumable",
                "null-capture",
                "external-pairing",
            ] {
                let status = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--ignored",
                        "--exact",
                        "closure_admission_preserves_checked_counts_and_code_identity",
                        "--nocapture",
                    ])
                    .env("BIRNAM_CLOSURE_ADMISSION_CASE", case)
                    .status()
                    .unwrap();
                if !status.success() {
                    failures.push(case);
                }
            }
            assert!(
                failures.is_empty(),
                "closure admission regressions: {failures:?}"
            );
            return;
        }
    };
    eprintln!("closure admission case: {case}");
    let host = oop::bootstrap();
    let q = host.scope();
    let hooks = oop::hooks::Hooks::new(&host);
    let named = |n: &[u8]| host.named(n).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let raw = |r, n: &[u8], a: &[Oop]| unsafe { host.id.send(r, key(n), a) };
    let code = |captures: &[&[u8]]| {
        let portable = encode(ProgramSpec {
            parameters: &[],
            captures,
            lets: &[],
            statements: &[StatementSpec::Expression { root_node: 0 }],
            nodes: &[NodeSpec::Integer(42)],
        })
        .unwrap();
        let bytes = call(
            named(b"Bytes"),
            b"fromBytes:",
            &[q.byte_values(&portable).unwrap()],
        );
        call(named(b"CodeObject"), b"fromCanonicalBytes:", &[bytes])
    };
    let factory = named(b"CodeObject");
    let owner = named(b"Block");
    let nil = named(b"nil");
    let vt = call(host.id.root, b"behaviorOf:", &[factory]);
    let vector = |values: &[Oop]| {
        let mut v = call(named(b"Vector"), b"new", &[]);
        for &value in values {
            v = call(v, b"add:", &[value]);
        }
        v
    };
    // A native Method may assert a portable association; admission does not
    // claim to verify its executable body against the supplied program bytes.
    let method = hooks.method(b"fixed", hooks.context(&[nil]));
    let admitted = code(&[]);
    let failure = hooks.fault(881);
    let denial = hooks.context(&[failure, size(0).unwrap()]);
    let deny = hooks.method(b"deny", denial);
    let replace_count = |selector: &[u8]| {
        call(vt, b"addMethod", &[key(selector), deny]);
    };
    match case.as_str() {
        "failed-admission" => {
            // Failure of either current count reader must precede publication.
            let selector = key(b"captureCountOf:");
            let original = call(vt, b"lookup", &[selector]);
            replace_count(b"captureCountOf:");
            let result = raw(method, b"admitCodeObject:", &[admitted]);
            assert_eq!(
                result,
                Err(failure),
                "capture-count failure must abort admission"
            );
            assert_eq!(
                call(method, b"codeObject", &[]),
                nil,
                "failed admission must not publish code"
            );
            assert!(raw(owner, b"fromMethod:captureOperands:", &[method]).is_err());
            call(vt, b"addMethod", &[selector, original]);
            assert_eq!(call(method, b"admitCodeObject:", &[admitted]), method);
        }
        "idempotent-admission" => {
            call(method, b"admitCodeObject:", &[admitted]);
            replace_count(b"parameterCountOf:");
            replace_count(b"captureCountOf:");
            assert_eq!(
                raw(method, b"admitCodeObject:", &[admitted]),
                Ok(method),
                "same-code admission must reuse its checked facts"
            );
            assert_eq!(
                hooks.count(denial, 1),
                0,
                "idempotent admission must not query shape again"
            );
        }
        "different-code" => {
            let other = code(&[]);
            assert_ne!(admitted, other);
            call(method, b"admitCodeObject:", &[admitted]);
            assert!(
                raw(method, b"admitCodeObject:", &[other]).is_err(),
                "equal program bytes do not authorize rebinding a Method to another CodeObject"
            );
            assert_eq!(call(method, b"codeObject", &[]), admitted);
        }
        "direct-synchronous" | "direct-resumable" | "null-capture" => {
            let code = code(&[b"a", b"b"]);
            let materialize = if case == "direct-resumable" {
                b"materializeResumable:captures:".as_slice()
            } else {
                b"materializeSynchronous:captures:"
            };
            let first = call(named(b"Object"), b"clone", &[]);
            let original = call(
                named(b"Codegen"),
                materialize,
                &[code, vector(&[first, first])],
            );
            q.check_result(original).unwrap();
            let body = call(original, b"methodFor:", &[key(b"value")]);
            // Supported immediates are Oops; reject missing references, not an
            // old Operand carrier kind. Replacements cannot rewrite admitted counts.
            replace_count(b"parameterCountOf:");
            replace_count(b"captureCountOf:");
            let valid = call(
                owner,
                b"fromMethod:captureOperands:",
                &[body, first, size(7).unwrap()],
            );
            assert_eq!(
                call(valid, b"captureWordAt:", &[size(1).unwrap()]),
                size(7).unwrap()
            );
            assert_eq!(hooks.count(denial, 1), 0);
            if case == "null-capture" {
                assert!(
                    raw(
                        owner,
                        b"fromMethod:captureOperands:",
                        &[body, first, std::ptr::null_mut()]
                    )
                    .is_err(),
                    "a raw absent reference is not a language capture"
                );
            } else {
                let mut accepted = Vec::new();
                for count in [0, 1, 3, 4097] {
                    let mut args = vec![body];
                    args.extend(std::iter::repeat_n(first, count));
                    if raw(owner, b"fromMethod:captureOperands:", &args).is_ok() {
                        accepted.push(count);
                    }
                }
                assert!(
                    accepted.is_empty(),
                    "Method with two admitted captures accepted counts {accepted:?}"
                );
            }
        }
        "external-pairing" => {
            let other = code(&[]);
            call(method, b"admitCodeObject:", &[admitted]);
            let paired = call(
                owner,
                b"fromMethod:codeObject:captures:",
                &[method, other, vector(&[])],
            );
            assert_eq!(
                call(paired, b"codeObject", &[]),
                other,
                "explicit pairing preserves supplied code identity"
            );
            assert_eq!(
                call(method, b"codeObject", &[]),
                admitted,
                "external pairing must leave original Method admission intact"
            );
            let wrong = code(&[b"unexpected"]);
            assert!(
                raw(
                    owner,
                    b"fromMethod:codeObject:captures:",
                    &[method, wrong, vector(&[nil])]
                )
                .is_err(),
                "external pairing must reject a capture-count mismatch"
            );
        }
        _ => panic!("unknown closure admission case {case}"),
    }
}
