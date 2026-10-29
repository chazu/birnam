//! Valid Oop closure construction, capture lifetime, image shells and task captures.
//! Code-owner rejection and checked-count cases are covered by closure_admission.rs.
#[path = "support/generation.rs"]
#[allow(dead_code)]
mod support;
use birnam_abi::{size, Oop};
use birnam_artifact_codec::portable_program::{encode, NodeSpec, ProgramSpec, StatementSpec};
use support::H;
fn literal() -> Vec<u8> {
    encode(ProgramSpec {
        parameters: &[],
        captures: &[],
        lets: &[],
        statements: &[StatementSpec::Expression { root_node: 0 }],
        nodes: &[NodeSpec::Integer(42)],
    })
    .unwrap()
}
fn admit(h: &H<'_>, program: &[u8]) -> Oop {
    h.call(
        h.named(b"CodeObject"),
        b"fromCanonicalBytes:",
        &[h.call(
            h.named(b"Bytes"),
            b"fromBytes:",
            &[h.q.byte_values(program).unwrap()],
        )],
    )
}
fn materialize(h: &H<'_>, code: Oop, captures: &[Oop], resumable: bool) -> Oop {
    h.call(
        h.named(b"Codegen"),
        if resumable {
            b"materializeResumable:captures:"
        } else {
            b"materializeSynchronous:captures:"
        },
        &[code, h.vector(captures)],
    )
}
#[test]
#[ignore = "requires production Oop modules"]
fn construction_preserves_captures_lifetime_and_pending_image_state() {
    let case = match std::env::var("BIRNAM_CLOSURE_CASE") {
        Ok(case) => case,
        Err(_) => {
            let mut failed = Vec::new();
            for case in ["construction", "image-shell", "nested", "tasks"] {
                let result = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--ignored",
                        "--exact",
                        "construction_preserves_captures_lifetime_and_pending_image_state",
                        "--nocapture",
                    ])
                    .env("BIRNAM_CLOSURE_CASE", case)
                    .status()
                    .unwrap();
                if !result.success() {
                    failed.push(case);
                }
            }
            assert!(
                failed.is_empty(),
                "closure construction cases failed: {failed:?}"
            );
            return;
        }
    };
    eprintln!("closure construction case: {case}");
    let host = support::bootstrap();
    let h = H::with_fixture(&host, "closure-construction");
    let owner = h.named(b"Block");
    let gc = h.named(b"GC");
    let nil = h.named(b"nil");
    let raw = |r, n: &[u8], a: &[Oop]| unsafe { host.id.send(r, h.key(n), a) };
    let code = admit(
        &h,
        &encode(ProgramSpec {
            parameters: &[b"arg"],
            captures: &[b"a", b"b"],
            lets: &[],
            statements: &[StatementSpec::Expression { root_node: 0 }],
            nodes: &[NodeSpec::LexicalRead(1)],
        })
        .unwrap(),
    );
    let first = h.integer(31);
    let second = h.integer(47);
    match case.as_str() {
        "construction" => {
            let probe = h.call(h.named(b"Object"), b"clone", &[]);
            let pv = h.call(host.id.root, b"ensurePrivateVtableFor:", &[probe]);
            for n in [b"present:".as_slice(), b"allocation:bytes:identity:"] {
                h.bind(pv, n, h.method(n, nil));
            }
            let present = |identity| h.call(probe, b"present:", &[identity]) == size(1).unwrap();
            let (admitted, token, identity) = {
                let q = host.scope();
                let bytes = q
                    .call(
                        h.named(b"Bytes"),
                        b"fromBytes:",
                        &[q.byte_values(&literal()).unwrap()],
                    )
                    .unwrap();
                let code = q
                    .call(h.named(b"CodeObject"), b"fromCanonicalBytes:", &[bytes])
                    .unwrap();
                let token = host.send(host.roots, b"retain:", &[code]).unwrap();
                let identity = host.send(host.id.root, b"identityOf:", &[code]).unwrap();
                (code, token, identity)
            };
            let factory = h.named(b"CodeObject");
            let fv = h.vt(factory);
            let count = h.binding(fv, b"parameterCountOf:");
            h.bind(fv, b"closureOriginalParameterCount:", count);
            let state = h.context(&[gc, host.roots, token, identity, factory, size(0).unwrap()]);
            h.bind(fv, b"parameterCountOf:", h.method(b"parameter", state));
            let direct = materialize(&h, admitted, &[], false);
            h.q.check_result(direct).unwrap();
            let method = h.call(direct, b"methodFor:", &[h.key(b"value")]);
            h.bind(fv, b"parameterCountOf:", count);
            assert_eq!(h.count(state, 5), 1);
            h.call(gc, b"collectPrecisely", &[]);
            assert!(
                present(identity),
                "compiled Method retains source after the caller lease is released during construction"
            );
            let direct = h.call(owner, b"fromMethod:captureOperands:", &[method]);
            assert_eq!(h.number(h.call(direct, b"value", &[])), 42);
            assert_eq!(h.call(direct, b"codeObject", &[]), admitted);
            let captured = h.call(direct, b"methodFor:", &[h.key(b"value")]);
            let alias = h.call(owner, b"fromMethod:captureOperands:", &[captured]);
            assert_eq!(h.number(h.call(alias, b"value", &[])), 42);
            assert_eq!(h.call(alias, b"codeObject", &[]), admitted);
            for resumable in [false, true] {
                let original = materialize(&h, code, &[first, second], resumable);
                let method = h.call(original, b"methodFor:", &[h.key(b"value:")]);
                let direct = h.call(
                    owner,
                    b"fromMethod:captureOperands:",
                    &[method, second, first],
                );
                assert_eq!(h.call(direct, b"value:", &[first]), second);
                assert_eq!(
                    h.call(owner, b"captureWord:of:", &[size(1).unwrap(), direct]),
                    first
                );
                let fresh = host.send(h.named(b"Object"), b"clone", &[]).unwrap();
                let token = host.send(host.roots, b"retain:", &[fresh]).unwrap();
                let identity = host.send(host.id.root, b"identityOf:", &[fresh]).unwrap();
                let bv = h.named(b"BlockVt");
                let av = h.vt(bv);
                let allocate = h.binding(av, b"allocate");
                h.bind(av, b"closureOriginalAllocate", allocate);
                let state = h.context(&[gc, host.roots, token, identity, bv, size(0).unwrap()]);
                let callback = h.method(b"allocation", state);
                h.bind(av, b"allocate", callback);
                let result = raw(
                    owner,
                    b"fromMethod:captureOperands:",
                    &[method, fresh, fresh],
                );
                h.bind(av, b"allocate", allocate);
                let block = h.keep(result.unwrap());
                assert_eq!(h.count(state, 5), 1);
                assert!(h.call(state, b"at:", &[size(2).unwrap()]).is_null());
                h.call(gc, b"collectPrecisely", &[]);
                assert!(present(identity));
                assert_eq!(h.call(block, b"value:", &[first]), fresh);
                let clone = h.call(block, b"clone", &[]);
                assert_eq!(h.call(clone, b"value:", &[first]), fresh);
            }
        }
        "image-shell" => {
            for resumable in [false, true] {
                let original = materialize(&h, code, &[first, second], resumable);
                let method = h.call(original, b"methodFor:", &[h.key(b"value:")]);
                let shell = h.call(owner, b"imageShellFromMethod:codeObject:", &[method, code]);
                assert!(
                    raw(shell, b"value:", &[first]).is_err(),
                    "pending image shell must not execute before capture restoration; resumable={resumable}"
                );
                let clone = h.call(shell, b"clone", &[]);
                assert!(
                    raw(clone, b"value:", &[first]).is_err(),
                    "cloning cannot make a pending image shell executable"
                );
                assert!(raw(
                    owner,
                    b"restoreImageCaptures:of:",
                    &[h.vector(&[first]), shell]
                )
                .is_err());
                assert!(raw(shell, b"value:", &[first]).is_err());
                for block in [shell, clone] {
                    h.call(
                        owner,
                        b"restoreImageCaptures:of:",
                        &[h.vector(&[second, first]), block],
                    );
                    h.call(gc, b"collectPrecisely", &[]);
                    assert_eq!(h.call(block, b"value:", &[first]), second);
                    assert!(
                        raw(
                            owner,
                            b"restoreImageCaptures:of:",
                            &[h.vector(&[first, second]), block]
                        )
                        .is_err(),
                        "restoration is one-time"
                    );
                }
                let zero_code = admit(
                    &h,
                    &encode(ProgramSpec {
                        parameters: &[],
                        captures: &[b"captured"],
                        lets: &[],
                        statements: &[StatementSpec::Expression { root_node: 0 }],
                        nodes: &[NodeSpec::LexicalRead(0)],
                    })
                    .unwrap(),
                );
                let complete = materialize(&h, zero_code, &[first], resumable);
                let method = h.call(complete, b"methodFor:", &[h.key(b"value")]);
                let pending = h.call(
                    owner,
                    b"imageShellFromMethod:codeObject:",
                    &[method, zero_code],
                );
                let machine = h.call(h.named(b"Machine"), b"new", &[]);
                let error = h.call(machine, b"runBlock:", &[pending]);
                assert_eq!(h.call(error, b"kind", &[]), h.key(b"nativeInvalidRequest"));
                h.call(
                    owner,
                    b"restoreImageCaptures:of:",
                    &[h.vector(&[first]), pending],
                );
                let machine = h.call(h.named(b"Machine"), b"new", &[]);
                assert_eq!(h.call(machine, b"runBlock:", &[pending]), first);
            }
        }
        "nested" => {
            let names = (0..12).map(|i| format!("c{i}")).collect::<Vec<_>>();
            let refs = names.iter().map(|s| s.as_bytes()).collect::<Vec<_>>();
            let inner = encode(ProgramSpec {
                parameters: &[],
                captures: &refs,
                lets: &[],
                statements: &[StatementSpec::Expression { root_node: 0 }],
                nodes: &[NodeSpec::LexicalRead(11)],
            })
            .unwrap();
            let indices = (0..12).collect::<Vec<_>>();
            let outer = admit(
                &h,
                &encode(ProgramSpec {
                    parameters: &[],
                    captures: &refs,
                    lets: &[],
                    statements: &[StatementSpec::Expression { root_node: 0 }],
                    nodes: &[NodeSpec::BlockLiteral {
                        program: &inner,
                        captures: &indices,
                    }],
                })
                .unwrap(),
            );
            let values = (0..12).map(|i| h.integer(i + 100)).collect::<Vec<_>>();
            for resumable in [false, true] {
                let outer = materialize(&h, outer, &values, resumable);
                let child = h.call(outer, b"value", &[]);
                assert_eq!(h.call(child, b"value", &[]), values[11]);
                for (i, v) in values.iter().enumerate() {
                    assert_eq!(
                        h.call(owner, b"captureWord:of:", &[size(i).unwrap(), child]),
                        *v
                    );
                }
                let empty = materialize(&h, admit(&h, &literal()), &[], resumable);
                assert_eq!(h.number(h.call(empty, b"value", &[])), 42);
            }
        }
        "tasks" => {
            h.modules(&["stdlib", "stacklessControl"]);
            for quantum in [128, 1] {
                let source = format!(
                    r#"
              [[TaskExecutor field: #quantum] set: {quantum}];
              let shared := [41 asCell]; let observed := [nil asCell]; let cleaned := [0 asCell];
              let task := [TaskControl start: [:task |
                [task ensure: [| [task wait]; [observed set: [shared value]]; [shared value]]
                  cleanup: [| [cleaned inc]]]]];
              let token := [task waitToken];
              [GC collectPrecisely]; [shared set: 42];
              let woke := [task resolve: token with: nil];
              let good := [[woke & [[observed value] = 42]] & [[cleaned value] = 1]];
              let outcome := [task pollOutcome];
              let cancelCleaned := [0 asCell];
              let cancelled := [TaskControl start: [:task |
                [task ensure: [| [task wait]; [observed set: 99]] cleanup: [| [cancelCleaned inc]]]]];
              [GC collectPrecisely]; [cancelled cancel: #stop];
              let cancelledOutcome := [cancelled pollOutcome];
              let success := [[good & [outcome = #[#success 42]]] & [[observed value] = 42]];
              [[success & [[cancelCleaned value] = 1]] & [cancelledOutcome = #[#cancelled #stop]]]
            "#
                );
                assert_eq!(
                    h.eval(source.as_bytes()),
                    h.named(b"true"),
                    "capture task lifecycle at quantum {quantum}"
                );
            }
        }
        _ => panic!("unknown closure case {case}"),
    }
}
