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

struct FailedAdoption {
    original: native::NativeEntry,
    id: Protocol,
    gc: Oop,
    error: Option<Oop>,
    calls: usize,
}
unsafe extern "C" fn failed_adoption(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    unsafe {
        let state = &mut *context.cast::<FailedAdoption>();
        let result =
            (state.original.method.unwrap())(state.original.context, receiver, args, count);
        if result.status != native::LOADED {
            return result;
        }
        state.calls += 1;
        state
            .id
            .send_named(state.gc, b"collectPrecisely", &[])
            .unwrap();
        state.error.map_or_else(
            || Outcome::failed(native::UNBOUND, 54322),
            |error| Outcome::loaded(Operand::object(error)),
        )
    }
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

#[test]
#[ignore = "requires production native modules; scripts/host-cli-test.sh"]
fn image_recovers_composed_generation_before_entry() {
    let lib = PathBuf::from(env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
    let image = env::var_os("BIRNAM_GENERATION_IMAGE").map(PathBuf::from);
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
            skip_stdlib: image.is_some(),
            precise_gc: true,
        },
    )
    .unwrap();
    let q = host.scope();
    let cg = host.named(b"Codegen").unwrap();
    let eval = |source: &[u8]| {
        let source = q.string(source).unwrap();
        let result = q
            .call(cg, b"evalPersisting:", &[Operand::object(source)])
            .unwrap();
        q.check_result(result).unwrap();
        result
    };
    let expect = |source: &[u8]| {
        assert_eq!(
            eval(source),
            host.named(b"true").unwrap(),
            "{}",
            String::from_utf8_lossy(source)
        );
    };
    let put_path = |name: &[u8], path: &Path| {
        let name = unsafe { host.id.intern_name(name) }.unwrap();
        let value = q.string(path.as_os_str().as_bytes()).unwrap();
        q.call(
            host.named(b"Globals").unwrap(),
            b"at:put:",
            &[Operand::object(name), Operand::object(value)],
        )
        .unwrap();
        value
    };
    let child = |path: &Path, mode: &str| {
        let status = std::process::Command::new(env::current_exe().unwrap())
            .args([
                "image_recovers_composed_generation_before_entry",
                "--ignored",
                "--nocapture",
            ])
            .env("BIRNAM_GENERATION_IMAGE", path)
            .env("BIRNAM_GENERATION_IMAGE_MODE", mode)
            .status()
            .unwrap();
        assert!(status.success(), "{mode} generation Image child failed");
    };
    if let Some(path) = image {
        let mode = env::var("BIRNAM_GENERATION_IMAGE_MODE").unwrap();
        eprintln!("generation Image: {mode} load");
        host.load_image_mode(&path, mode == "rebuild").unwrap();
        expect(b"[[2 imageGenerationValue] = 20]");
        expect(b"[[[DispatchGeneration current] predecessor] isNil]");
        expect(b"[[[DispatchGeneration current] deltaUnit] isNil]");
        expect(b"[[[DispatchGeneration current] compositionIdentity] = [System imageCompositionIdentity]]");
        expect(b"[[[[DispatchGeneration current] unitFor: #imageGeneration] kind] = #composed]");
        expect(b"[[[[DispatchGeneration current] unitFor: #imageGeneration] state] = #active]");
        expect(b"[[db currentValue: [[DispatchGeneration current] unitFor: #imageGeneration] attr: #rollbackAvailable] = false]");
        expect(b"[[[Codegen adoptComposedGeneration: [DispatchGeneration current]] kind] = #imageRecoveryRequired]");
        expect(b"[[[CompiledUnit reconcileImageComposition: [System imageCompositionIdentity]] kind] = #imageRecoveryRequired]");
        expect(b"[[db attrCount: savedOriginGeneration] = 0]");
        expect(b"[[db attrCount: savedOriginPredecessor] = 0]");
        expect(b"[[Codegen reconcileCandidate: [DispatchGeneration current]] == [DispatchGeneration current]]");
        if mode != "resaved" {
            let resaved = path.with_extension(format!("{mode}.resaved.image"));
            let filename = q.string(resaved.as_os_str().as_bytes()).unwrap();
            let saved = q
                .call(
                    host.named(b"System").unwrap(),
                    b"saveImage:",
                    &[Operand::object(filename)],
                )
                .unwrap();
            q.check_result(saved).unwrap();
            assert!(resaved.is_file());
            child(&resaved, "resaved");
            fs::remove_file(resaved).unwrap();
        }
        eprintln!("generation Image: {mode} successor and rollback");
        let artifact = path.with_extension(format!("{mode}.successor.o"));
        put_path(b"imageGenerationPath", &artifact);
        eval(br#"[CompiledUnit compile: "[Module named: #imageGeneration requires: #[]]; [Reflect defineMethod: #imageGenerationValue on: Integer as: [:self | 30]]" named: #imageGeneration output: imageGenerationPath]"#);
        eval(
            b"let imageSuccessor := [CompiledUnit open: imageGenerationPath]; [imageSuccessor map]",
        );
        q.evaluate(
            b"[imageSuccessor activate]",
            Path::new("<image-successor>"),
            None,
        )
        .unwrap();
        expect(b"[[2 imageGenerationValue] = 30]");
        q.evaluate(
            b"[imageSuccessor rollback]",
            Path::new("<image-rollback>"),
            None,
        )
        .unwrap();
        expect(b"[[2 imageGenerationValue] = 20]");
        // Loading into a process with a live inverse ledger must replace that
        // capability set, not retain its predecessor pointers as the new base.
        host.load_image(&path).unwrap();
        expect(b"[[Codegen reconcileCandidate: [DispatchGeneration current]] == [DispatchGeneration current]]");
        expect(b"[[[DispatchGeneration current] predecessor] isNil]");
        host.send(host.named(b"GC").unwrap(), b"collectPrecisely", &[])
            .unwrap();
        expect(b"[[2 imageGenerationValue] = 20]");
        fs::remove_file(artifact).unwrap();
        return;
    }
    q.evaluate(b"[Module named: #imageGeneration requires: #[]]; [Reflect defineMethod: #imageGenerationValue on: Integer as: [:self | 10]]", Path::new("<image-generation>"), Some(b"imageGeneration")).unwrap();
    eval(b"[DispatchGeneration initializeCurrent]");
    let dir = env::temp_dir().join(format!("birnam-image-generation-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    put_path(b"imageGenerationPath", &dir.join("candidate.o"));
    eval(br#"[CompiledUnit compile: "[Module named: #imageGeneration requires: #[]]; [Reflect defineMethod: #imageGenerationValue on: Integer as: [:self | 20]]" named: #imageGeneration output: imageGenerationPath]"#);
    eval(b"let imageCandidate := [CompiledUnit open: imageGenerationPath]; [imageCandidate map]");
    q.evaluate(
        b"[imageCandidate activate]",
        Path::new("<image-activation>"),
        None,
    )
    .unwrap();
    expect(b"[[2 imageGenerationValue] = 20]");
    eval(b"let savedOriginGeneration := [DispatchGeneration current]; let savedOriginPredecessor := [savedOriginGeneration predecessor]");
    let path = dir.join("active.image");
    let filename = q.string(path.as_os_str().as_bytes()).unwrap();
    let saved = q
        .call(
            host.named(b"System").unwrap(),
            b"saveImage:",
            &[Operand::object(filename)],
        )
        .unwrap();
    q.check_result(saved).unwrap();
    assert!(path.is_file(), "managed Image save failed");
    eprintln!("generation Image: returned Error and native failure compensation");
    let before = eval(b"[DispatchGeneration current]");
    let vt = q
        .call(host.id.receiver, b"behaviorOf:", &[Operand::object(cg)])
        .unwrap();
    let selector = unsafe { host.id.intern_name(b"adoptComposedGeneration:") }.unwrap();
    let original = host
        .send(
            host.id.receiver,
            b"bindingOf:in:",
            &[Operand::object(selector), Operand::object(vt)],
        )
        .unwrap()
        .as_entry()
        .unwrap();
    let kind = unsafe { host.id.intern_name(b"injectedAdoptionFailure") }.unwrap();
    let error = q
        .call(
            host.named(b"Error").unwrap(),
            b"kind:",
            &[Operand::object(kind)],
        )
        .unwrap();
    for returned_error in [Some(error), None] {
        let mut fault = FailedAdoption {
            original,
            id: host.id,
            gc: host.named(b"GC").unwrap(),
            error: returned_error,
            calls: 0,
        };
        host.send(
            vt,
            b"addMethod",
            &[
                Operand::object(selector),
                Operand::entry(native::NativeEntry {
                    method: Some(failed_adoption),
                    context: (&mut fault as *mut FailedAdoption).cast(),
                }),
            ],
        )
        .unwrap();
        let failure = host.load_image(&path).unwrap_err();
        host.send(
            vt,
            b"addMethod",
            &[Operand::object(selector), Operand::entry(original)],
        )
        .unwrap();
        assert_eq!(fault.calls, 1, "{failure}");
        assert_eq!(eval(b"[DispatchGeneration current]"), before);
        expect(b"[[Codegen reconcileCandidate: [DispatchGeneration current]] == [DispatchGeneration current]]");
        expect(b"[[2 imageGenerationValue] = 20]");
        expect(b"[[imageCandidate state] = #active]");
    }
    q.evaluate(
        b"[imageCandidate rollback]",
        Path::new("<after-failed-image>"),
        None,
    )
    .unwrap();
    expect(b"[[2 imageGenerationValue] = 10]");
    child(&path, "mapped");
    child(&path, "rebuild");
    fs::remove_dir_all(dir).unwrap();
}
