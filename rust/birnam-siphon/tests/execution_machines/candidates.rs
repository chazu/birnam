//! Explicit candidate selection and retirement through the production owner.
use super::*;
fn install(h: &H<'_>, method: Oop, steps: &[Oop]) {
    assert_eq!(h.call(method, b"optimizeWith:", &[h.vector(steps)]), method);
}
fn candidate(h: &H<'_>, owner: Oop) -> Oop {
    h.native(b"candidate", h.context(&[owner, h.named(b"Integer")]))
}
pub(super) fn check(h: &H<'_>) {
    let nil = h.named(b"nil");
    let baseline_owner = h.object();
    let optimized_owner = h.object();
    let baseline = h.method("minimal-return", 1, baseline_owner);
    let step = candidate(h, optimized_owner);
    // Captured aliases install on their shared logical registration.
    let captured = h.call(baseline, b"captured", &[]);
    install(h, captured, &[step]);
    for _ in 0..63 {
        assert_eq!(h.invoke(baseline, nil), baseline_owner);
    }
    assert_eq!(h.invoke(baseline, nil), optimized_owner);
    h.collect();
    assert_eq!(h.invoke(captured, nil), optimized_owner);
    for bad in [
        h.vector(&[step, step]),
        h.vector(&[baseline]),
        h.vector(&[nil]),
    ] {
        assert!(h.host.send(baseline, b"optimizeWith:", &[bad]).is_err());
        assert_eq!(
            h.invoke(baseline, nil),
            optimized_owner,
            "failed admission preserves candidate and visits"
        );
    }
    let machine = h.machine();
    let vt = h.vt(machine);
    // Each guarded operation can withdraw a candidate; restoring it does not
    // reactivate the old installation.
    for name in [
        b"oopAt:".as_slice(),
        b"oopAt:put:",
        b"callSite:",
        b"tailCallSite:",
        b"continueAt:",
        b"currentFrameView",
    ] {
        let original = h.call(vt, b"lookup", &[h.key(name)]);
        h.bind(vt, name, h.native(b"self", ptr::null_mut()));
        assert_eq!(h.invoke(baseline, nil), baseline_owner);
        h.bind(vt, name, original);
        assert_eq!(h.invoke(baseline, nil), baseline_owner);
        install(h, baseline, &[step]);
        for _ in 0..63 {
            assert_eq!(h.invoke(baseline, nil), baseline_owner);
        }
        assert_eq!(h.invoke(baseline, nil), optimized_owner);
    }
    install(h, baseline, &[]);
    assert_eq!(h.invoke(baseline, nil), baseline_owner);

    // Every admitted program counter gets its own declared executable.
    let branch = h.method("branch-join", 5, baseline_owner);
    install(h, branch, &[step; 4]);
    for _ in 0..21 {
        assert_eq!(h.invoke(branch, nil), baseline_owner);
    }
    assert_eq!(h.invoke(branch, nil), optimized_owner);
    // Reentrant installation during a successful binding proof must not enter
    // the superseded candidate or withdraw the new one.
    let next_owner = h.object();
    let next_step = candidate(h, next_owner);
    install(h, baseline, &[step]);
    for _ in 0..64 {
        h.invoke(baseline, nil);
    }
    let root_vt = h.vt(h.host.id.root);
    let selector = h.key(b"nativeBindingsMatch:");
    let original = h.call(root_vt, b"lookup", &[selector]);
    let alias = h.key(b"candidateSavedProof");
    h.bind(root_vt, b"candidateSavedProof", original);
    let state = h.context(&[
        baseline,
        h.vector(&[next_step]),
        root_vt,
        selector,
        original,
        alias,
        size(0).unwrap(),
    ]);
    h.bind(root_vt, b"nativeBindingsMatch:", h.native(b"proof", state));
    assert_eq!(h.invoke(baseline, nil), baseline_owner);
    assert_eq!(h.count(state, 6), 1);
    assert_eq!(h.call(root_vt, b"lookup", &[selector]), original);
    h.remove(root_vt, b"candidateSavedProof");
    for _ in 0..63 {
        assert_eq!(h.invoke(baseline, nil), baseline_owner);
    }
    assert_eq!(h.invoke(baseline, nil), next_owner);
    h.clear();
    h.collect();
    failed_installation_and_entry(h);
    active_lifetime(h);
    retirement(h);
}
fn failed_installation_and_entry(h: &H<'_>) {
    let hooks = oop::hooks::Hooks::new(h.host);
    let fault = hooks.fault(917);
    let failed = hooks.context(&[fault, size(0).unwrap()]);
    let deny = hooks.method(b"deny", failed);
    let owner = h.object();
    let baseline = h.method("minimal-return", 1, owner);
    let optimized = h.object();
    let step = candidate(h, optimized);
    install(h, baseline, &[step]);
    let input = h.object();
    let vt = h.private_vt(input);
    h.bind(vt, b"size", hooks.method(b"fixed", h.integer(1)));
    h.bind(vt, b"at:", deny);
    assert_eq!(
        unsafe { h.host.id.send(baseline, h.key(b"optimizeWith:"), &[input]) }.unwrap_err(),
        fault
    );
    assert_eq!(hooks.count(failed, 1), 1);
    for _ in 0..63 {
        assert_eq!(h.invoke(baseline, h.named(b"nil")), owner);
    }
    assert_eq!(h.invoke(baseline, h.named(b"nil")), optimized);
    install(h, baseline, &[deny]);
    for _ in 0..63 {
        assert_eq!(h.invoke(baseline, h.named(b"nil")), owner);
    }
    assert_eq!(
        unsafe {
            h.host
                .id
                .send(baseline, h.key(b"invokeOn:"), &[h.named(b"nil")])
        }
        .unwrap_err(),
        fault
    );
    assert_eq!(
        hooks.count(failed, 1),
        2,
        "an entered candidate failure is not replayed"
    );
    h.clear();
    h.collect();
}
fn active_lifetime(h: &H<'_>) {
    let baseline_owner = h.object();
    let baseline = h.method("minimal-return", 1, baseline_owner);
    let owner = h.object();
    let step = candidate(h, owner);
    let owner_id = h.identity(owner);
    let step_id = h.identity(step);
    install(h, baseline, &[step]);
    for _ in 0..64 {
        h.invoke(baseline, h.named(b"nil"));
    }
    let empty = h.vector(&[]);
    let pin = h.host.scope();
    pin.keep(baseline).unwrap();
    pin.keep(empty).unwrap();
    h.clear();
    h.collect();
    assert!(h.present(owner_id));
    let integer = h.named(b"Integer");
    let vt = h.vt(integer);
    let original = h.call(vt, b"lookup", &[h.key(b"fromWord:")]);
    let alias = h.key(b"candidateSavedFromWord:");
    h.bind(vt, b"candidateSavedFromWord:", original);
    let state = h.context(&[
        baseline,
        empty,
        h.gc,
        h.monitor,
        owner_id,
        alias,
        size(0).unwrap(),
    ]);
    h.bind(vt, b"fromWord:", h.native(b"withdraw", state));
    assert_eq!(h.invoke(baseline, h.named(b"nil")), owner);
    assert_eq!(h.count(state, 6), 1);
    h.bind(vt, b"fromWord:", original);
    h.remove(vt, b"candidateSavedFromWord:");
    h.clear();
    h.collect();
    assert!(
        !h.present(owner_id) && !h.present(step_id),
        "withdrawn candidate and context must release after return"
    );
    assert_eq!(h.invoke(baseline, h.named(b"nil")), baseline_owner);
    drop(pin);
    h.clear();
    h.collect();
}
fn retirement(h: &H<'_>) {
    for _ in 0..96 {
        let owner = h.object();
        let method = h.method("minimal-return", 1, owner);
        let owner_id = h.identity(owner);
        let method_id = h.identity(method);
        assert_eq!(h.invoke(method, h.named(b"nil")), owner);
        h.clear();
        h.collect();
        assert!(
            !h.present(owner_id) && !h.present(method_id),
            "retired registrations must not retain or collide with a later Method"
        );
    }
}
