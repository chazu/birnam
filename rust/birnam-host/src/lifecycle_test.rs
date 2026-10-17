//! Authored lifecycle policy entered from the real owner-thread Host boundary.
use super::*;
use birnam_abi::id_protocol::{self as native};
use std::{fs,ffi::c_void, path::PathBuf};

unsafe extern "C" fn failed_fulfill(
    _: *mut c_void,
    _: Oop,
    _: *const Operand,
    _: usize,
) -> Outcome {
    Outcome::failed(native::UNBOUND, 54321)
}

struct ReturnedFailure {
    error: Oop,
    calls: usize,
}
unsafe extern "C" fn returned_failure(
    context: *mut c_void,
    _: Oop,
    _: *const Operand,
    _: usize,
) -> Outcome {
    let fault = unsafe { &mut *context.cast::<ReturnedFailure>() };
    fault.calls += 1;
    Outcome::loaded(Operand::object(fault.error))
}

struct CounterReentry {
    id: Protocol,
    original: native::NativeEntry,
    unit: Oop,
    other: Oop,
    armed: bool,
}
unsafe extern "C" fn counter_reentry(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &mut *c.cast::<CounterReentry>();
        if p.armed {
            p.armed = false;
            if let Err(error) = p.id.send_named(p.unit, b"nextGenerationFor:", &[Operand::object(p.other)]) { return error; }
        }
        p.original.method.unwrap()(p.original.context, r, a, n)
    }
}

struct RecoveryFailure {
    original: native::NativeEntry,
    id: Protocol,
    gc: Oop,
    error: Option<Oop>,
    calls: usize,
}
unsafe extern "C" fn failed_recovery(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    unsafe {
        let state = &mut *context.cast::<RecoveryFailure>();
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
fn activation_inside_entered_methods_uses_current_setters() {
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
            source_bootstrap: false,
            tool_bootstrap: false,
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
    let baseline = b"[Module named: #hostLifecycle requires: #[]]; let hostLifecycleTrigger := [false asCell]; let hostLifecycleDrop := [false asCell]; [Reflect defineMethod: #hostLifecycleValue on: Integer as: [:self | [[hostLifecycleTrigger value] ifTrue: [| [hostLifecycleTrigger set: false]; [hostLifecycleUnit activate]; [[hostLifecycleDrop value] ifTrue: [| [hostLifecycleUnit discard]; [Codegen evictMaterializations]]]; [System gc]]]; 10]]";
    q.evaluate(
        baseline,
        Path::new("<lifecycle-baseline>"),
        Some(b"hostLifecycle"),
    )
    .unwrap();
    expect(b"[[DispatchGeneration current] isNil]");
    expect(b"[[Reflect defineMethod: #ordinaryLifecycleValue on: Integer as: [:self | 99]] == Integer]");
    expect(b"[[2 ordinaryLifecycleValue] = 99]");
    expect(b"[[2 hostLifecycleValue] = 10]");
    let dir = env::temp_dir().join(format!("birnam-host-lifecycle-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = q
        .string(dir.join("candidate.o").as_os_str().as_bytes())
        .unwrap();
    let globals = host.named(b"Globals").unwrap();
    let name = unsafe { host.context.id.intern_name(b"hostLifecyclePath") }.unwrap();
    q.call(
        globals,
        b"at:put:",
        &[Operand::object(name), Operand::object(path)],
    )
    .unwrap();
    q.check(cg, eval(br#"[CompiledUnit compile: "[Module named: #hostLifecycle requires: #[]]; let hostLifecycleTrigger := [false asCell]; let hostLifecycleDrop := [false asCell]; [Reflect defineMethod: #hostLifecycleValue on: Integer as: [:self | 20]]" named: #hostLifecycle output: hostLifecyclePath]"#)).unwrap();
    q.check(cg, eval(b"let hostLifecycleUnit := [CompiledUnit open: hostLifecyclePath]; [hostLifecycleUnit map]")).unwrap();
    expect(b"[[2 hostLifecycleValue] = 10]");
    let unit = eval(b"hostLifecycleUnit");
    let selector = unsafe { host.context.id.intern_name(b"state:") }.unwrap();
    q.call(unit, b"methodFor:", &[Operand::object(selector)]).unwrap();
    let vt = q.call(host.context.id.receiver, b"behaviorOf:", &[Operand::object(unit)]).unwrap();
    let original = host.send(host.context.id.receiver, b"bind:to:", &[Operand::object(selector), Operand::object(unit)]).unwrap();
    host.send(vt, b"addMethod", &[Operand::object(selector), Operand::entry(native::NativeEntry::new(failed_fulfill, std::ptr::null_mut()))]).unwrap();
    let failure = q.evaluate(b"[[| [hostLifecycleUnit activate]] value]", Path::new("<entered-native-failure>"), None).unwrap_err();
    assert!(failure.0.contains("nativeFailure"), "{failure}");
    expect(b"[[hostLifecycleUnit state] = #candidate]");
    expect(b"[[2 hostLifecycleValue] = 10]");
    let error = q.call(host.named(b"Error").unwrap(), b"kind:", &[Operand::object(unsafe { host.context.id.intern_name(b"enteredSetterFailure") }.unwrap())]).unwrap();
    let mut fault = ReturnedFailure { error, calls: 0 };
    host.send(vt, b"addMethod", &[Operand::object(selector), Operand::entry(native::NativeEntry::new(returned_failure, (&mut fault as *mut ReturnedFailure).cast()))]).unwrap();
    assert_eq!(eval(b"[hostLifecycleUnit activate]"), error);
    assert_eq!(fault.calls, 1);
    expect(b"[[hostLifecycleUnit state] = #candidate]");
    host.send(vt, b"addMethod", &[Operand::object(selector), original]).unwrap();
    // This call replaces the method whose frame is currently entered. Its old
    // body completes after GC; only subsequent dispatch selects the replacement.
    eval(b"[hostLifecycleTrigger set: true]");
    expect(b"[[2 hostLifecycleValue] = 10]");
    expect(b"[[2 hostLifecycleValue] = 20]");
    expect(b"[[hostLifecycleUnit state] = #active]");
    host.send(host.named(b"GC").unwrap(), b"collectPrecisely", &[])
        .unwrap();
    eval(b"[Globals at: #main put: [| [hostLifecycleUnit rollback]]]");
    host.run_image_main().unwrap();
    expect(b"[[2 hostLifecycleValue] = 10]");
    expect(b"[[hostLifecycleUnit state] = #rolledBack]");
    q.evaluate(
        b"[hostLifecycleUnit rollback]",
        Path::new("<repeat-rollback>"),
        None,
    )
    .unwrap();
    // Release the active handle/inverse from the method being replaced. With
    // materialization caches evicted, entered execution still owns its old code.
    eval(b"let hostLifecycleUnit := [CompiledUnit open: hostLifecyclePath]; [hostLifecycleUnit map]; [hostLifecycleDrop set: true]; [hostLifecycleTrigger set: true]");
    expect(b"[[2 hostLifecycleValue] = 10]");
    expect(b"[[2 hostLifecycleValue] = 20]");
    expect(b"[[hostLifecycleUnit state] = #discarded]");
    expect(b"[[hostLifecycleUnit rollbackAvailable] = false]");
    expect(b"[[[hostLifecycleUnit rollback] kind] = #rollbackUnavailable]");
    assert!(dir.join("candidate.o").is_file(), "discard deleted the artifact");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "requires production native modules; scripts/host-cli-test.sh"]
fn image_restores_descriptions_and_explicit_adoption() {
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
            source_bootstrap: false,
            tool_bootstrap: false,
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
        let name = unsafe { host.context.id.intern_name(name) }.unwrap();
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
                "image_restores_descriptions_and_explicit_adoption",
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
        host.load_image_mode(&path, mode == "rebuild" || mode == "drift-rebuild", false).unwrap();
        expect(b"[[DispatchGeneration current] == savedOriginGeneration]");
        expect(b"[[savedOriginGeneration predecessor] == savedOriginPredecessor]");
        expect(b"[[savedOriginGeneration sourceMap] notNil]");
        expect(b"[[db currentValue: savedOriginGeneration attr: #userAnnotation] = 77]");
        expect(b"[[db currentValue: savedOriginPredecessor attr: #sourceMap] = 88]");
        expect(b"[[[Generation reconcileCandidate: [DispatchGeneration current]] kind] = #generationNotAdopted]");
        expect(b"[[[imageCandidate rollback] kind] = #rollbackUnavailable]");
        if mode.starts_with("drift-") {
            expect(b"[[2 imageGenerationValue] = 30]");
            expect(b"[[2 imageOutsideValue] = 9]");
            expect(b"[[imageGenerationClosure value] = 31]");
            expect(b"[[[DispatchGeneration adoptCurrent] kind] = #sourceBindingMismatch]");
            expect(b"[[DispatchGeneration current] == savedOriginGeneration]");
            host.send(host.named(b"GC").unwrap(), b"collectPrecisely", &[]).unwrap();
            expect(b"[[2 imageOutsideValue] = 9]");
            let filename = q.string(path.with_extension(format!("{mode}.resaved.image")).as_os_str().as_bytes()).unwrap();
            let saved = q.call(host.named(b"System").unwrap(), b"saveImage:", &[Operand::object(filename)]).unwrap();
            q.check_result(saved).unwrap();
            return;
        }
        expect(b"[[2 imageOutsideValue] = 1]");
        expect(b"[[2 imageGenerationValue] = 20]");
        expect(b"[[imageGenerationClosure value] = 21]");
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
        // Portability rejection must not depend on an adopted ledger.
        let selector = unsafe { host.context.id.intern_name(b"imageOutsideValue") }.unwrap();
        let integer_vt = host.named(b"IntegerVt").unwrap();
        q.call(host.named(b"Integer").unwrap(), b"methodFor:", &[Operand::object(selector)]).unwrap();
        let original = host.send(host.context.id.receiver, b"bindingOf:in:", &[Operand::object(selector), Operand::object(integer_vt)]).unwrap().as_entry().unwrap();
        host.send(integer_vt, b"addMethod", &[Operand::object(selector), Operand::entry(native::NativeEntry::new(failed_fulfill, core::ptr::null_mut()))]).unwrap();
        let opaque_path = path.with_extension(format!("{mode}.opaque.image"));
        fs::write(&opaque_path, b"previous image").unwrap();
        let filename = q.string(opaque_path.as_os_str().as_bytes()).unwrap();
        let rejected = q.call(host.named(b"System").unwrap(), b"saveImage:", &[Operand::object(filename)]).unwrap();
        host.send(integer_vt, b"addMethod", &[Operand::object(selector), Operand::entry(original)]).unwrap();
        let kind = q.call(rejected, b"kind", &[]).unwrap();
        assert_eq!(kind, unsafe { host.context.id.intern_name(b"nonPortableGeneration") }.unwrap());
        assert_eq!(fs::read(&opaque_path).unwrap(), b"previous image");
        fs::remove_file(opaque_path).unwrap();
        eval(b"let adoptedBaseline := [DispatchGeneration adoptCurrent]");
        expect(b"[[adoptedBaseline isError] not]");
        expect(b"[[[DispatchGeneration current] predecessor] isNil]");
        expect(b"[[[DispatchGeneration current] deltaUnit] isNil]");
        expect(b"[[[DispatchGeneration current] unitFor: #imageGeneration] isNil]");
        expect(b"[[savedOriginGeneration predecessor] == savedOriginPredecessor]");
        expect(b"[[Generation reconcileCandidate: adoptedBaseline] == adoptedBaseline]");
        eprintln!("generation Image: {mode} successor and rollback");
        let artifact = path.with_extension(format!("{mode}.successor.o"));
        put_path(b"imageGenerationPath", &artifact);
        eval(br#"[CompiledUnit compile: "[Module named: #imageGeneration requires: #[]]; let imageGenerationClosure := [| 31]; [Reflect defineMethod: #imageGenerationValue on: Integer as: [:self | 30]]" named: #imageGeneration output: imageGenerationPath]"#);
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
        expect(b"[[imageGenerationClosure value] = 31]");
        q.evaluate(
            b"[imageSuccessor rollback]",
            Path::new("<image-rollback>"),
            None,
        )
        .unwrap();
        expect(b"[[2 imageGenerationValue] = 20]");
        expect(b"[[imageGenerationClosure value] = 21]");
        // Loading into a process with a live inverse ledger must replace that
        // capability set, not retain its predecessor pointers as the new base.
        host.load_image(&path).unwrap();
        expect(b"[[DispatchGeneration current] == savedOriginGeneration]");
        expect(b"[[[Generation reconcileCandidate: [DispatchGeneration current]] kind] = #generationNotAdopted]");
        host.send(host.named(b"GC").unwrap(), b"collectPrecisely", &[])
            .unwrap();
        expect(b"[[2 imageGenerationValue] = 20]");
        expect(b"[[imageGenerationClosure value] = 21]");
        fs::remove_file(artifact).unwrap();
        return;
    }
    q.evaluate(b"[Module named: #imageGeneration requires: #[]]; let imageGenerationClosure := [| 11]; [Reflect defineMethod: #imageGenerationValue on: Integer as: [:self | 10]]", Path::new("<image-generation>"), Some(b"imageGeneration")).unwrap();
    q.evaluate(b"[Module named: #imageOutside requires: #[]]; [Reflect defineMethod: #imageOutsideValue on: Integer as: [:self | 1]]", Path::new("<image-outside>"), Some(b"imageOutside")).unwrap();
    eval(b"[DispatchGeneration adoptCurrent]");
    let dir = env::temp_dir().join(format!("birnam-image-generation-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    put_path(b"imageGenerationPath", &dir.join("candidate.o"));
    eval(br#"[CompiledUnit compile: "[Module named: #imageGeneration requires: #[]]; let imageGenerationClosure := [| 21]; [Reflect defineMethod: #imageGenerationValue on: Integer as: [:self | 20]]" named: #imageGeneration output: imageGenerationPath]"#);
    eval(b"let imageCandidate := [CompiledUnit open: imageGenerationPath]; [imageCandidate map]");
    q.evaluate(
        b"[imageCandidate activate]",
        Path::new("<image-activation>"),
        None,
    )
    .unwrap();
    expect(b"[[2 imageGenerationValue] = 20]");
    expect(b"[[imageGenerationClosure value] = 21]");
    eval(b"let savedOriginGeneration := [DispatchGeneration current]; let savedOriginPredecessor := [savedOriginGeneration predecessor]; [db assert: savedOriginGeneration attr: #userAnnotation value: 77]; [db assert: savedOriginPredecessor attr: #sourceMap value: 88]");
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
    eprintln!("generation Image: returned Error and native recovery failure");
    let before = eval(b"[DispatchGeneration current]");
    let captured = q.call(host.named(b"Integer").unwrap(), b"methodFor:",
        &[Operand::object(unsafe { host.context.id.intern_name(b"imageGenerationValue") }.unwrap())]).unwrap();
    let receiver = eval(b"2");
    let vt = q
        .call(host.context.id.receiver, b"behaviorOf:", &[Operand::object(host.named(b"Generation").unwrap())])
        .unwrap();
    let selector = unsafe { host.context.id.intern_name(b"recoverImageGeneration:") }.unwrap();
    let original = host
        .send(
            host.context.id.receiver,
            b"bindingOf:in:",
            &[Operand::object(selector), Operand::object(vt)],
        )
        .unwrap()
        .as_entry()
        .unwrap();
    let kind = unsafe { host.context.id.intern_name(b"injectedRecoveryFailure") }.unwrap();
    let error = q
        .call(
            host.named(b"Error").unwrap(),
            b"kind:",
            &[Operand::object(kind)],
        )
        .unwrap();
    for returned_error in [Some(error), None] {
        let mut fault = RecoveryFailure {
            original,
            id: host.context.id,
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
                    method: Some(failed_recovery),
                    context: (&mut fault as *mut RecoveryFailure).cast(),
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
        expect(b"[[[System imageSaveReport] status] = #partialLoad]");
        expect(b"[[Generation validateImageGeneration] isError]");
        let stale = q.call(host.named(b"Generation").unwrap(), b"reconcileCandidate:", &[Operand::object(before)]).unwrap();
        assert!(q.check_result(stale).is_err(), "old adoption must not regain authority");
        let held = q.call(captured, b"invokeOn:", &[Operand::object(receiver)]).unwrap();
        assert_eq!(host.send(host.named(b"Integer").unwrap(), b"wordOf:", &[Operand::object(held)]).unwrap().as_word(), Some(20));
        // Explicitly reload a known image after a partial/conflicted live load.
        host.load_image(&path).unwrap();
        expect(b"[[[Generation reconcileCandidate: [DispatchGeneration current]] kind] = #generationNotAdopted]");
        expect(b"[[2 imageGenerationValue] = 20]");
        expect(b"[[imageGenerationClosure value] = 21]");
    }
    let held = q.call(captured, b"invokeOn:", &[Operand::object(receiver)]).unwrap();
    assert_eq!(host.send(host.named(b"Integer").unwrap(), b"wordOf:", &[Operand::object(held)]).unwrap().as_word(), Some(20));
    host.load_image(&path).unwrap();
    expect(b"[[DispatchGeneration current] == savedOriginGeneration]");
    expect(b"[[[Generation reconcileCandidate: [DispatchGeneration current]] kind] = #generationNotAdopted]");
    child(&path, "mapped");
    child(&path, "rebuild");
    eval(b"[DispatchGeneration adoptCurrent]");
    // An independent managed edit must survive scoped activation and both
    // restoration modes. Its old source revision must not replace live code.
    eval(b"[Reflect defineMethod: #imageOutsideValue on: Integer as: [:self | 9]]");
    put_path(b"imageGenerationPath", &dir.join("scoped.o"));
    eval(br#"[CompiledUnit compile: "[Module named: #imageGeneration requires: #[]]; let imageGenerationClosure := [| 31]; [Reflect defineMethod: #imageGenerationValue on: Integer as: [:self | 30]]" named: #imageGeneration output: imageGenerationPath]"#);
    eval(b"let scopedCandidate := [CompiledUnit open: imageGenerationPath]; [scopedCandidate map]");
    q.evaluate(b"[scopedCandidate activate]", Path::new("<scoped-activation>"), None).unwrap();
    expect(b"[[2 imageGenerationValue] = 30]");
    expect(b"[[2 imageOutsideValue] = 9]");
    eval(b"let savedOriginGeneration := [DispatchGeneration current]; let savedOriginPredecessor := [savedOriginGeneration predecessor]; let imageCandidate := scopedCandidate; [db assert: savedOriginGeneration attr: #userAnnotation value: 77]; [db assert: savedOriginPredecessor attr: #sourceMap value: 88]");
    let drift_path = dir.join("scoped-drift.image");
    let filename = q.string(drift_path.as_os_str().as_bytes()).unwrap();
    let saved = q.call(host.named(b"System").unwrap(), b"saveImage:", &[Operand::object(filename)]).unwrap();
    q.check_result(saved).unwrap();
    assert!(drift_path.is_file());
    expect(b"[[scopedCandidate state] = #active]");
    expect(b"[scopedCandidate rollbackAvailable]");
    expect(b"[[DispatchGeneration current] == savedOriginGeneration]");
    child(&drift_path, "drift-mapped");
    child(&drift_path, "drift-rebuild");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "requires installed production modules"]
fn lifecycle_fields_preserve_explicit_facts_and_status_reads() {
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
            source_bootstrap: false,
            tool_bootstrap: false,
        },
    )
    .unwrap();
    let q = host.scope();
    let cg = host.named(b"Codegen").unwrap();
    let eval = |source: &[u8]| {
        let result = q.call(cg, b"evalPersisting:", &[Operand::object(q.string(source).unwrap())]).unwrap();
        q.check_result(result).unwrap(); result
    };
    let expect = |source: &[u8]| { assert_eq!(eval(source), host.named(b"true").unwrap(), "{}", String::from_utf8_lossy(source)); };
    eval(b"let ownedUnit := [CompiledUnit clone]; [ownedUnit setState: #candidate]; [db assert: ownedUnit attr: #state value: #annotation]; let unitAlias := ownedUnit; let unitClone := [ownedUnit clone]");
    expect(b"[[ownedUnit state] = #candidate]");
    expect(b"[[[Reflect slotsOf: ownedUnit] size] = 13]");
    eval(b"[Reflect slot: #state in: unitClone put: #cloneState]");
    expect(b"[[unitClone state] = #cloneState]");
    expect(b"[[ownedUnit state] = #candidate]");
    eval(b"[Reflect slot: #state in: unitAlias put: #active]");
    expect(b"[[ownedUnit state] = #active]");
    expect(b"[[db currentValue: ownedUnit attr: #state] = #annotation]");
    eval(b"let oldDescription := [DispatchGeneration fromProjection: #[#generationProjection #[] nil nil] predecessor: ownedUnit delta: ownedUnit]; let freshDescription := [oldDescription fromProjection: #[#generationProjection #[] nil nil] predecessor: nil delta: nil]");
    expect(b"[[freshDescription predecessor] isNil]");
    expect(b"[[freshDescription deltaUnit] isNil]");
    expect(b"[[oldDescription predecessor] == ownedUnit]");
    let generation = host.named(b"Generation").unwrap();
    let vt = q.call(host.context.id.receiver, b"behaviorOf:", &[Operand::object(generation)]).unwrap();
    let selector = unsafe { host.context.id.intern_name(b"reconcileCandidate:") }.unwrap();
    let original = host.send(host.context.id.receiver, b"bindingOf:in:", &[Operand::object(selector), Operand::object(vt)]).unwrap();
    let mut fault = ReturnedFailure { error: host.named(b"nil").unwrap(), calls: 0 };
    host.send(vt, b"addMethod", &[Operand::object(selector), Operand::entry(native::NativeEntry::new(returned_failure, (&mut fault as *mut ReturnedFailure).cast()))]).unwrap();
    expect(b"[[ownedUnit state] = #active]");
    assert_eq!(fault.calls, 0, "status read performed implicit validation");
    host.send(vt, b"addMethod", &[Operand::object(selector), original]).unwrap();
    let unit = eval(b"ownedUnit");
    let alias = eval(b"unitAlias");
    assert_eq!(unit, alias);
    let names = [b"Store".as_slice(), b"db"];
    let values: Vec<_> = names.iter().map(|n| host.named(n).unwrap()).collect();
    for name in names {
        let key = unsafe { host.context.id.intern_name(name) }.unwrap();
        host.send(host.context.namespace, b"remove:", &[Operand::object(key)]).unwrap();
    }
    // Invoke the actual compiled getters/setters without parsing new source.
    let state = unsafe { host.context.id.intern_name(b"withoutStore") }.unwrap();
    q.call(unit, b"setState:", &[Operand::object(state)]).unwrap();
    assert_eq!(q.call(unit, b"state", &[]).unwrap(), state);
    for (name, value) in names.into_iter().zip(values) {
        let key = unsafe { host.context.id.intern_name(name) }.unwrap();
        host.send(host.context.namespace, b"at:put:", &[Operand::object(key), Operand::object(value)]).unwrap();
    }
    expect(b"[[db currentValue: ownedUnit attr: #state] = #annotation]");
    let owner = host.global(&q, b"CompiledUnit").unwrap();
    let a = unsafe { host.context.id.intern_name(b"counterA") }.unwrap();
    let b = unsafe { host.context.id.intern_name(b"counterB") }.unwrap();
    q.call(owner, b"nextGenerationFor:", &[Operand::object(a)]).unwrap();
    let key = unsafe { host.context.id.intern_name(b"+") }.unwrap();
    let vt = host.named(b"IntegerVt").unwrap();
    let original = host.send(host.context.id.receiver, b"bindingOf:in:", &[Operand::object(key), Operand::object(vt)]).unwrap().as_entry().unwrap();
    if let Some(method) = host.send(host.named(b"Execution").unwrap(), b"methodOfEntry:", &[Operand::entry(original)]).unwrap().as_object() { q.keep(Operand::object(method)).unwrap(); }
    let mut reentry = CounterReentry { id: host.context.id, original, unit: owner, other: b, armed: true };
    host.send(vt, b"addMethod", &[Operand::object(key), Operand::entry(native::NativeEntry::new(counter_reentry, (&mut reentry as *mut CounterReentry).cast()))]).unwrap();
    let next = q.call(owner, b"nextGenerationFor:", &[Operand::object(a)]);
    host.send(vt, b"addMethod", &[Operand::object(key), Operand::entry(original)]).unwrap();
    assert!(!reentry.armed, "counter did not use the current arithmetic send");
    assert_eq!(host.send(host.named(b"Integer").unwrap(), b"wordOf:", &[Operand::object(next.unwrap())]).unwrap().as_word(), Some(2));
    let other = q.call(owner, b"nextGenerationFor:", &[Operand::object(b)]).unwrap();
    assert_eq!(host.send(host.named(b"Integer").unwrap(), b"wordOf:", &[Operand::object(other)]).unwrap().as_word(), Some(2), "counter publication overwrote a reentrant edit to another name");
    let numbers = q.call(owner, b"generationNumbers", &[]).unwrap();
    assert_eq!(q.call(numbers, b"at:", &[Operand::object(b)]).unwrap(), other);
    q.call(owner, b"generationNumbers:", &[Operand::object(numbers)]).unwrap();

}

#[test]
#[ignore = "requires production native modules"]
fn independent_candidate_modules_need_no_certificate() {
    let lib = PathBuf::from(env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
    let resources = crate::resolve_from_exe_dir(Path::new("/unused/bin"), crate::Overrides { lib_dir: Some(&lib), ..Default::default() }).unwrap();
    let host = HostRuntime::start(&resources, StartupPolicy { skip_stdlib: false, precise_gc: true, source_bootstrap: false, tool_bootstrap: false }).unwrap();
    let q = host.scope();
    let cg = host.named(b"Codegen").unwrap();
    let eval = |source: &[u8]| {
        let value = q.call(cg, b"evalPersisting:", &[Operand::object(q.string(source).unwrap())]).unwrap();
        q.check_result(value).unwrap(); value
    };
    let expect = |source: &[u8]| assert_eq!(eval(source), host.named(b"true").unwrap(), "{}", String::from_utf8_lossy(source));
    let run = |source: &[u8]| { q.evaluate(source, Path::new("<scoped-module-change>"), None).unwrap(); };
    for (name, source) in [
        (b"independentA".as_slice(), b"[Module named: #independentA requires: #[]]; [Reflect defineMethod: #independentA on: Integer as: [:self | 1]]".as_slice()),
        (b"independentB", b"[Module named: #independentB requires: #[]]; [Reflect defineMethod: #independentB on: Integer as: [:self | 2]]"),
    ] { q.evaluate(source, Path::new("<module-baseline>"), Some(name)).unwrap(); }
    let dir = env::temp_dir().join(format!("birnam-independent-candidates-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for (key, file) in [(b"candidateAPath".as_slice(), "a.o"), (b"candidateBPath", "b.o")] {
        q.call(host.named(b"Globals").unwrap(), b"at:put:", &[Operand::object(unsafe { host.context.id.intern_name(key) }.unwrap()), Operand::object(q.string(dir.join(file).as_os_str().as_bytes()).unwrap())]).unwrap();
    }
    eval(br#"[CompiledUnit compile: "[Module named: #independentA requires: #[]]; [Reflect defineMethod: #independentA on: Integer as: [:self | 11]]" named: #independentA output: candidateAPath]"#);
    eval(br#"[CompiledUnit compile: "[Module named: #independentB requires: #[]]; [Reflect defineMethod: #independentB on: Integer as: [:self | 22]]" named: #independentB output: candidateBPath]"#);
    eval(b"let unitA := [CompiledUnit open: candidateAPath]; [unitA map]; let unitB := [CompiledUnit open: candidateBPath]; [unitB map]");
    run(b"[unitA activate]");
    q.evaluate(b"[Module named: #introducedLater requires: #[]]; [Reflect defineMethod: #introducedLater on: Integer as: [:self | 33]]", Path::new("<later>"), Some(b"introducedLater")).unwrap();
    run(b"[unitB activate]");
    expect(b"[[3 independentA] = 11]"); expect(b"[[3 independentB] = 22]");
    expect(b"[[DispatchGeneration current] isNil]");
    run(b"[unitA rollback]");
    expect(b"[[3 independentA] = 1]"); expect(b"[[3 independentB] = 22]");
    expect(b"[[3 introducedLater] = 33]");
    // Optional certification can be established/refreshed without changing undo.
    eval(b"[DispatchGeneration adoptCurrent]");
    run(b"[unitB rollback]");
    expect(b"[[3 independentB] = 2]");
    eval(b"[DispatchGeneration adoptCurrent]");
    expect(b"[[Generation reconcileCandidate: [DispatchGeneration current]] == [DispatchGeneration current]]");
    // An artifact can be opened afresh and used again after exact module rollback.
    eval(b"let unitAgain := [CompiledUnit open: candidateAPath]; [unitAgain map]");
    run(b"[unitAgain activate]"); expect(b"[[3 independentA] = 11]");
    run(b"[unitAgain rollback]"); expect(b"[[3 independentA] = 1]");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "requires production native modules"]
fn discard_cancels_preparation_and_keeps_bindings_and_files() {
    let lib = PathBuf::from(env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
    let resources = crate::resolve_from_exe_dir(Path::new("/unused/bin"), crate::Overrides { lib_dir: Some(&lib), ..Default::default() }).unwrap();
    let host = HostRuntime::start(&resources, StartupPolicy { skip_stdlib: false, precise_gc: true, source_bootstrap: false, tool_bootstrap: false }).unwrap();
    let q=host.scope(); let cg=host.named(b"Codegen").unwrap();
    let eval=|source:&[u8]| { let v=q.call(cg,b"evalPersisting:",&[Operand::object(q.string(source).unwrap())]).unwrap();q.check_result(v).unwrap();v };
    let expect=|source:&[u8]| assert_eq!(eval(source),host.named(b"true").unwrap(),"{}",String::from_utf8_lossy(source));
    q.evaluate(b"[Module named: #discardScope requires: #[]]; let discardHelper := [| 1]; [Reflect defineMethod: #discardScope on: Integer as: [:self | [discardHelper value]]]",Path::new("<discard-baseline>"),Some(b"discardScope")).unwrap();
    let dir=env::temp_dir().join(format!("birnam-discard-scope-{}",std::process::id()));fs::create_dir_all(&dir).unwrap();
    let path=dir.join("candidate.o");
    q.call(host.named(b"Globals").unwrap(),b"at:put:",&[Operand::object(unsafe{host.context.id.intern_name(b"discardPath")}.unwrap()),Operand::object(q.string(path.as_os_str().as_bytes()).unwrap())]).unwrap();
    eval(br#"[CompiledUnit compile: "[Module named: #discardScope requires: #[]]; let discardHelper := [| 2]; [Reflect defineMethod: #discardScope on: Integer as: [:self | [discardHelper value]]]" named: #discardScope output: discardPath]"#);
    let tcc=host.named(b"TCC").unwrap();
    let tcc_vt=q.call(host.context.id.receiver,b"behaviorOf:",&[Operand::object(tcc)]).unwrap();
    let diagnostic_key=unsafe{host.context.id.intern_name(b"mappingIdentityOf:")}.unwrap();
    q.call(tcc,b"methodFor:",&[Operand::object(diagnostic_key)]).unwrap();
    let diagnostic=host.send(host.context.id.receiver,b"bind:to:",&[Operand::object(diagnostic_key),Operand::object(tcc)]).unwrap();
    host.send(tcc_vt,b"addMethod",&[Operand::object(diagnostic_key),Operand::entry(native::NativeEntry::new(failed_fulfill,std::ptr::null_mut()))]).unwrap();
    eval(b"let preparedUnit := [CompiledUnit open: discardPath]; [preparedUnit map]; let abandonedToken := [Generation prepareActivation: preparedUnit plan: [preparedUnit publicationPlan: #activate]]");
    expect(b"[[Generation watchCandidateRetirement: preparedUnit] isError]");
    let bytes=fs::read(&path).unwrap();
    fs::write(&path,b"new author file").unwrap();
    expect(b"[[preparedUnit discard] == preparedUnit]");
    expect(b"[[[Generation commitPrepared: abandonedToken] kind] = #invalidPreparedToken]");
    expect(b"[[3 discardScope] = 1]");
    assert_eq!(fs::read(&path).unwrap(),b"new author file");
    fs::write(&path,bytes).unwrap();
    host.send(tcc_vt,b"addMethod",&[Operand::object(diagnostic_key),diagnostic]).unwrap();
    eval(b"let activeUnit := [CompiledUnit open: discardPath]; [activeUnit map]");
    let globals=host.named(b"Globals").unwrap();
    let vt=q.call(host.context.id.receiver,b"behaviorOf:",&[Operand::object(globals)]).unwrap();
    let key=unsafe{host.context.id.intern_name(b"releaseBindingCompensationFor:")}.unwrap();
    q.call(globals,b"methodFor:",&[Operand::object(key)]).unwrap();
    let original=host.send(host.context.id.receiver,b"bind:to:",&[Operand::object(key),Operand::object(globals)]).unwrap().as_entry().unwrap();
    let error=q.call(host.named(b"Error").unwrap(),b"kind:",&[Operand::object(unsafe{host.context.id.intern_name(b"releaseDenied")}.unwrap())]).unwrap();
    let mut fault=ReturnedFailure{error,calls:0};
    expect(b"[[activeUnit activate] == activeUnit]");
    host.send(vt,b"addMethod",&[Operand::object(key),Operand::entry(native::NativeEntry::new(returned_failure,(&mut fault as *mut ReturnedFailure).cast()))]).unwrap();
    let rejected=q.call(cg,b"evalPersisting:",&[Operand::object(q.string(b"[activeUnit discard]").unwrap())]).unwrap();
    assert_eq!(rejected,error);
    assert_eq!(fault.calls,1);
    expect(b"[[activeUnit state] = #active]");
    host.send(vt,b"addMethod",&[Operand::object(key),Operand::entry(original)]).unwrap();
    expect(b"[[activeUnit discard] == activeUnit]");
    expect(b"[[activeUnit rollbackAvailable] = false]");
    expect(b"[[3 discardScope] = 2]");
    expect(b"[[[activeUnit rollback] kind] = #rollbackUnavailable]");
    assert!(path.exists());
    fs::remove_dir_all(dir).unwrap();
}
