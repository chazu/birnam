//! Authored call preparation through the current Execution inspection operations.
use super::*;
use std::path::Path;
pub(super) fn check(h: &H<'_>) {
    h.clear();
    h.collect();
    let baseline_units = h.units();
    let baseline_bytes = h.native_bytes();
    let q = h.host.scope();
    let path =
        std::env::temp_dir().join(format!("birnam-prepared-call-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/prepared-call.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = h.host.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let execution = h.named(b"Execution");
    let execution_vt = h.vt(execution);
    let registration = h.key(b"fromStep:descriptor:selection:owner:callPlans:");
    let register = h.call(execution_vt, b"lookup", &[registration]);
    let captured = h.hooks.context(&[register, h.named(b"nil")]);
    let capture = q
        .call(
            q.call(fixture, b"methodAt:", &[h.key(b"capture")]).unwrap(),
            b"withContext:",
            &[captured],
        )
        .unwrap();
    h.call(execution_vt, b"addMethod", &[registration, capture]);
    let admitted = h.compile(b"[Object clone]");
    h.call(execution_vt, b"addMethod", &[registration, register]);
    q.check_result(admitted).unwrap();
    let saved = h.call(captured, b"at:", &[size(1).unwrap()]);
    let mut arguments: Vec<_> = (0..5)
        .map(|i| h.call(saved, b"at:", &[size(i).unwrap()]))
        .collect();
    let plans = q.read_bytes(h.call(arguments[4], b"bytes", &[])).unwrap();
    assert!(plans.len() >= 24);
    let mut wrong_version = plans.clone();
    wrong_version[..4].copy_from_slice(&99u32.to_le_bytes());
    let mut wrong_source = plans.clone();
    wrong_source[20..24].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut trailing = plans.clone();
    trailing.push(0);
    for malformed in [
        vec![],
        plans[..plans.len() - 1].to_vec(),
        wrong_version,
        wrong_source,
        trailing,
    ] {
        arguments[4] = q.string(&malformed).unwrap();
        assert!(
            unsafe { h.host.id.send(execution, registration, &arguments) }.is_err(),
            "malformed plans must fail admission"
        );
    }
    h.hooks.put(captured, 1, h.named(b"nil"));
    let machine = h.named(b"Machine");
    let operation = h.key(b"prepareCall:selector:");
    let original = h.call(h.host.id.root, b"bind:to:", &[operation, machine]);
    assert!(
        !original.is_null(),
        "Execution must expose authored call preparation"
    );
    let body = h.eval(include_bytes!(
        "../../../../tests/fixtures/execution/prepared-call-reference.birnam"
    ));
    h.host.scope().check_result(body).unwrap();
    h.bind(b"PreparedReference", body);
    let holder=h.eval(b"[[| let holder := [Object clone]; [Reflect defineMethod: #prepareCall:selector: on: holder as: PreparedReference]; holder] value]");
    h.host.scope().check_result(holder).unwrap();
    let authored = h.call(h.host.id.root, b"bind:to:", &[operation, holder]);
    let argument = h.eval(b"[Object clone]");
    h.bind(b"PreparedArgument", argument);
    let block=h.eval(b"[| let firstBody := [:x :y :z | y]; let lastBody := [:x :y | y]; let a := PreparedArgument; let b := a; let c := b; let first := [firstBody value: a value: b value: c]; let next := first; let last := next; [lastBody value: next value: last]]");
    let target = h.call(machine, b"new", &[]);
    let behavior = h.vt(target);
    let state = h.hooks.context(&[
        target,
        authored,
        original,
        size(0).unwrap(),
        size(0).unwrap(),
        size(0).unwrap(),
        h.named(b"Integer"),
        size(0).unwrap(),
    ]);
    let replacement = q
        .call(
            q.call(fixture, b"methodAt:", &[h.key(b"replace")]).unwrap(),
            b"withContext:",
            &[state],
        )
        .unwrap();
    h.call(behavior, b"addMethod", &[operation, replacement]);
    let result = unsafe { h.host.id.send(target, h.key(b"runBlock:"), &[block]) };
    h.call(behavior, b"addMethod", &[operation, original]);
    assert_eq!(
        result.unwrap(),
        argument,
        "authored preparation preserves exact identity"
    );
    assert!(h.hooks.count(state, 3) >= 2);
    assert!(h.hooks.count(state, 5) > 0);
    assert!(
        h.hooks.count(state, 7) > 0,
        "authored preparation executes ordered alias copies"
    );
    let failed = h.call(machine, b"new", &[]);
    let fault = h.hooks.fault(71);
    let failure = h.hooks.context(&[fault, size(0).unwrap()]);
    h.call(
        behavior,
        b"addMethod",
        &[operation, h.hooks.method(b"deny", failure)],
    );
    let result = unsafe { h.host.id.send(failed, h.key(b"runBlock:"), &[block]) };
    h.call(behavior, b"addMethod", &[operation, original]);
    assert_eq!(result.unwrap_err(), fault);
    assert_eq!(
        h.hooks.count(failure, 1),
        1,
        "an entered failure is not replayed"
    );
    let fresh = h.call(machine, b"new", &[]);
    assert_eq!(h.call(fresh, b"runBlock:", &[block]), argument);
    h.call(h.vt(holder), b"removeMethod:", &[operation]);
    for (i, v) in [(0, h.named(b"nil")), (1, h.named(b"nil"))] {
        h.hooks.put(state, i, v);
    }
    h.bind(b"PreparedReference", h.named(b"nil"));
    h.bind(b"PreparedArgument", h.named(b"nil"));
    h.call(h.named(b"Codegen"), b"evictMaterializations", &[]);
    drop(q);
    h.clear();
    h.collect();
    assert_eq!(h.units(), baseline_units);
    assert_eq!(h.native_bytes(), baseline_bytes);
}
