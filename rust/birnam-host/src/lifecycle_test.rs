//! Authored lifecycle policy entered from the real owner-thread Host boundary.
use super::*;
use birnam_abi::id_protocol::{self as native};
use std::{ffi::c_void, path::PathBuf};

unsafe extern "C" fn failed_fulfill(
    _: *mut c_void,
    _: Oop,
    _: *const Operand,
    _: usize,
) -> Outcome {
    Outcome::failed(native::UNBOUND, 54321)
}

#[test]
#[ignore = "requires production native modules; scripts/host-cli-test.sh"]
fn host_fulfills_authored_generation_intents_at_idle_boundary() {
    let lib = PathBuf::from(env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
    let resources = crate::resolve_from_exe_dir(
        Path::new("/unused/bin"),
        crate::Overrides {
            lib_dir: Some(&lib),
            ..Default::default()
        },
    )
    .unwrap();
    let host = HostRuntime::start(
        &resources,
        StartupPolicy {
            skip_stdlib: false,
            precise_gc: true,
        },
    )
    .unwrap();
    let q = host.scope();
    let cg = host.named(b"Codegen").unwrap();
    let eval = |source: &[u8]| {
        let source = q.string(source).unwrap();
        q.call(cg, b"evalPersisting:", &[Operand::object(source)])
            .unwrap()
    };
    let expect = |source: &[u8]| {
        let result = eval(source);
        q.check_result(result).unwrap();
        assert_eq!(
            result,
            host.named(b"true").unwrap(),
            "{}",
            String::from_utf8_lossy(source)
        );
    };
    let baseline = b"[Module named: #hostLifecycle requires: #[]]; [Reflect defineMethod: #hostLifecycleValue on: Integer as: [:self | 10]]";
    q.evaluate(
        baseline,
        Path::new("<lifecycle-baseline>"),
        Some(b"hostLifecycle"),
    )
    .unwrap();
    q.check(cg, eval(b"[DispatchGeneration initializeCurrent]"))
        .unwrap();
    let dir = env::temp_dir().join(format!("birnam-host-lifecycle-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = q
        .string(dir.join("candidate.o").as_os_str().as_bytes())
        .unwrap();
    let globals = host.named(b"Globals").unwrap();
    let name = unsafe { host.id.intern_name(b"hostLifecyclePath") }.unwrap();
    q.call(
        globals,
        b"at:put:",
        &[Operand::object(name), Operand::object(path)],
    )
    .unwrap();
    q.check(cg, eval(br#"[CompiledUnit compile: "[Module named: #hostLifecycle requires: #[]]; [Reflect defineMethod: #hostLifecycleValue on: Integer as: [:self | 20]]" named: #hostLifecycle output: hostLifecyclePath]"#)).unwrap();
    q.check(cg, eval(b"let hostLifecycleUnit := [CompiledUnit open: hostLifecyclePath]; [hostLifecycleUnit map]")).unwrap();
    expect(b"[[2 hostLifecycleValue] = 10]");
    expect(b"[[[[hostLifecycleUnit activate] fulfill] kind] = #generationNotQuiescent]");
    expect(b"[[[Codegen fulfillEvolutionIntent: [hostLifecycleUnit activate]] kind] = #generationNotQuiescent]");
    expect(b"[[hostLifecycleUnit state] = #candidate]");
    // A native failure during fulfillment must close the quiescence extent.
    let intent = eval(b"[hostLifecycleUnit activate]");
    let vt = q
        .call(host.id.receiver, b"behaviorOf:", &[Operand::object(intent)])
        .unwrap();
    let selector = unsafe { host.id.intern_name(b"fulfill") }.unwrap();
    let original = host
        .send(
            host.id.receiver,
            b"bindingOf:in:",
            &[Operand::object(selector), Operand::object(vt)],
        )
        .unwrap();
    host.send(
        vt,
        b"addMethod",
        &[
            Operand::object(selector),
            Operand::entry(native::NativeEntry {
                method: Some(failed_fulfill),
                context: std::ptr::null_mut(),
            }),
        ],
    )
    .unwrap();
    let failure = q.fulfill_intent(intent).unwrap_err();
    assert!(failure.0.contains("54321"), "{failure}");
    host.send(vt, b"addMethod", &[Operand::object(selector), original])
        .unwrap();
    q.evaluate(
        b"[hostLifecycleUnit activate]",
        Path::new("<activation>"),
        None,
    )
    .unwrap();
    expect(b"[[2 hostLifecycleValue] = 20]");
    expect(b"[[hostLifecycleUnit state] = #active]");
    host.send(host.named(b"GC").unwrap(), b"collectPrecisely", &[])
        .unwrap();
    q.evaluate(
        b"[hostLifecycleUnit rollback]",
        Path::new("<rollback>"),
        None,
    )
    .unwrap();
    expect(b"[[2 hostLifecycleValue] = 10]");
    expect(b"[[hostLifecycleUnit state] = #rolledBack]");
    q.evaluate(
        b"[hostLifecycleUnit rollback]",
        Path::new("<repeat-rollback>"),
        None,
    )
    .unwrap();
    fs::remove_dir_all(dir).unwrap();
}
