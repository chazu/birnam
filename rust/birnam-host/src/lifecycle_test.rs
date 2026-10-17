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

struct RetirementFailure {
    adopt: native::NativeEntry,
    retract: native::NativeEntry,
    error: Oop,
    armed: bool,
    calls: usize,
}
unsafe extern "C" fn arm_retirement_failure(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let fault = unsafe { &mut *context.cast::<RetirementFailure>() };
    let result =
        unsafe { (fault.adopt.method.unwrap())(fault.adopt.context, receiver, args, count) };
    let generation = unsafe { birnam_abi::borrowed_slice(args, count) }
        .and_then(|values| values.first())
        .and_then(|value| value.as_object());
    fault.armed = result.status == native::LOADED
        && generation.is_some()
        && result.value.as_object() == generation;
    result
}
unsafe extern "C" fn fail_retirement_retract(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let fault = unsafe { &mut *context.cast::<RetirementFailure>() };
    if fault.armed {
        fault.armed = false;
        fault.calls += 1;
        Outcome::loaded(Operand::object(fault.error))
    } else {
        unsafe { (fault.retract.method.unwrap())(fault.retract.context, receiver, args, count) }
    }
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
    let baseline = b"[Module named: #hostLifecycle requires: #[]]; [Reflect defineMethod: #hostLifecycleValue on: Integer as: [:self | 10]]";
    q.evaluate(
        baseline,
        Path::new("<lifecycle-baseline>"),
        Some(b"hostLifecycle"),
    )
    .unwrap();
    q.check(cg, eval(b"[DispatchGeneration initializeCurrent]"))
        .unwrap();
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
    q.check(cg, eval(br#"[CompiledUnit compile: "[Module named: #hostLifecycle requires: #[]]; [Reflect defineMethod: #hostLifecycleValue on: Integer as: [:self | 20]]" named: #hostLifecycle output: hostLifecyclePath]"#)).unwrap();
    q.check(cg, eval(b"let hostLifecycleUnit := [CompiledUnit open: hostLifecyclePath]; [hostLifecycleUnit map]")).unwrap();
    expect(b"[[2 hostLifecycleValue] = 10]");
    expect(b"[[[[hostLifecycleUnit activate] fulfill] kind] = #generationNotQuiescent]");
    expect(b"[[[Generation fulfillEvolutionIntent: [hostLifecycleUnit activate]] kind] = #generationNotQuiescent]");
    expect(b"[[hostLifecycleUnit state] = #candidate]");
    // A native failure during fulfillment must close the quiescence extent.
    let intent = eval(b"[hostLifecycleUnit activate]");
    let vt = q
        .call(host.context.id.receiver, b"behaviorOf:", &[Operand::object(intent)])
        .unwrap();
    let selector = unsafe { host.context.id.intern_name(b"fulfill") }.unwrap();
    let original = host
        .send(
            host.context.id.receiver,
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
    // The image-main route must also fulfill only after the main frame exits,
    // preserve native failure details, and close the extent before retrying.
    q.check(
        cg,
        eval(b"[Globals at: #main put: [| [hostLifecycleUnit activate]]]"),
    )
    .unwrap();
    let failure = host.run_image_main().unwrap_err();
    assert!(failure.0.contains("54321"), "{failure}");
    host.send(vt, b"addMethod", &[Operand::object(selector), original])
        .unwrap();
    host.run_image_main().unwrap();
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
        host.load_image_mode(&path, mode == "rebuild", false).unwrap();
        expect(b"[[2 imageGenerationValue] = 20]");
        expect(b"[[imageGenerationClosure value] = 21]");
        expect(b"[[[DispatchGeneration current] predecessor] isNil]");
        expect(b"[[[DispatchGeneration current] deltaUnit] isNil]");
        expect(b"[[[DispatchGeneration current] compositionIdentity] = [System imageCompositionIdentity]]");
        expect(b"[[[[DispatchGeneration current] unitFor: #imageGeneration] kind] = #composed]");
        expect(b"[[[[DispatchGeneration current] unitFor: #imageGeneration] state] = #active]");
        expect(b"[[[[DispatchGeneration current] unitFor: #imageGeneration] rollbackAvailable] = false]");
        expect(b"[[[Generation adoptComposedGeneration: [DispatchGeneration current]] kind] = #imageRecoveryRequired]");
        expect(b"[[[CompiledUnit reconcileImageComposition: [System imageCompositionIdentity]] kind] = #imageRecoveryRequired]");
        expect(b"[[db currentValue: savedOriginGeneration attr: #userAnnotation] = 77]");
        expect(b"[[savedOriginGeneration sourceMap] notNil]");
        expect(b"[[db currentValue: savedOriginPredecessor attr: #sourceMap] = 88]");
        expect(b"[[savedOriginPredecessor sourceMap] notNil]");
        expect(b"[[Generation reconcileCandidate: [DispatchGeneration current]] == [DispatchGeneration current]]");
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
        expect(b"[[Generation reconcileCandidate: [DispatchGeneration current]] == [DispatchGeneration current]]");
        expect(b"[[[DispatchGeneration current] predecessor] isNil]");
        host.send(host.named(b"GC").unwrap(), b"collectPrecisely", &[])
            .unwrap();
        expect(b"[[2 imageGenerationValue] = 20]");
        expect(b"[[imageGenerationClosure value] = 21]");
        fs::remove_file(artifact).unwrap();
        return;
    }
    q.evaluate(b"[Module named: #imageGeneration requires: #[]]; let imageGenerationClosure := [| 11]; [Reflect defineMethod: #imageGenerationValue on: Integer as: [:self | 10]]", Path::new("<image-generation>"), Some(b"imageGeneration")).unwrap();
    eval(b"[DispatchGeneration initializeCurrent]");
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
    eprintln!("generation Image: returned Error and native failure compensation");
    let before = eval(b"[DispatchGeneration current]");
    let captured = q.call(host.named(b"Integer").unwrap(), b"methodFor:",
        &[Operand::object(unsafe { host.context.id.intern_name(b"imageGenerationValue") }.unwrap())]).unwrap();
    let receiver = eval(b"2");
    let vt = q
        .call(host.context.id.receiver, b"behaviorOf:", &[Operand::object(host.named(b"Generation").unwrap())])
        .unwrap();
    let selector = unsafe { host.context.id.intern_name(b"adoptComposedGeneration:") }.unwrap();
    let original = host
        .send(
            host.context.id.receiver,
            b"bindingOf:in:",
            &[Operand::object(selector), Operand::object(vt)],
        )
        .unwrap()
        .as_entry()
        .unwrap();
    let kind = unsafe { host.context.id.intern_name(b"injectedAdoptionFailure") }.unwrap();
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
        expect(b"[[[System imageSaveReport] status] = #partialLoad]");
        expect(b"[[Generation validateImageGeneration] isError]");
        let stale = q.call(host.named(b"Generation").unwrap(), b"reconcileCandidate:", &[Operand::object(before)]).unwrap();
        assert!(q.check_result(stale).is_err(), "old adoption must not regain authority");
        let held = q.call(captured, b"invokeOn:", &[Operand::object(receiver)]).unwrap();
        assert_eq!(host.send(host.named(b"Integer").unwrap(), b"wordOf:", &[Operand::object(held)]).unwrap().as_word(), Some(20));
        // Explicitly reload a known image after a partial/conflicted live load.
        host.load_image(&path).unwrap();
        expect(b"[[Generation reconcileCandidate: [DispatchGeneration current]] == [DispatchGeneration current]]");
        expect(b"[[2 imageGenerationValue] = 20]");
        expect(b"[[imageGenerationClosure value] = 21]");
    }
    // Post-adoption failure has already changed multiple owners. Preserve those
    // states and report incomplete recovery; never revive the previous ledger
    // as authority over the partially loaded world.
    let store = host.named(b"Store").unwrap();
    let retract_vt = q
        .call(host.context.id.receiver, b"behaviorOf:", &[Operand::object(store)])
        .unwrap();
    let retract_selector = unsafe { host.context.id.intern_name(b"retract:attr:value:") }.unwrap();
    let retract_original = host
        .send(
            host.context.id.receiver,
            b"bindingOf:in:",
            &[
                Operand::object(retract_selector),
                Operand::object(retract_vt),
            ],
        )
        .unwrap()
        .as_entry()
        .unwrap();
    let mut fault = RetirementFailure {
        adopt: original,
        retract: retract_original,
        error,
        armed: false,
        calls: 0,
    };
    let context = (&mut fault as *mut RetirementFailure).cast();
    host.send(
        vt,
        b"addMethod",
        &[
            Operand::object(selector),
            Operand::entry(native::NativeEntry::new(arm_retirement_failure, context)),
        ],
    )
    .unwrap();
    host.send(
        retract_vt,
        b"addMethod",
        &[
            Operand::object(retract_selector),
            Operand::entry(native::NativeEntry::new(fail_retirement_retract, context)),
        ],
    )
    .unwrap();
    let failure = host.load_image(&path);
    host.send(
        vt,
        b"addMethod",
        &[Operand::object(selector), Operand::entry(original)],
    )
    .unwrap();
    host.send(
        retract_vt,
        b"addMethod",
        &[
            Operand::object(retract_selector),
            Operand::entry(retract_original),
        ],
    )
    .unwrap();
    failure.unwrap();
    assert_eq!(fault.calls, 0, "image adoption must not retract lifecycle or user facts");
    expect(b"[[[Generation validateImageGeneration] isError] not]");
    let held = q.call(captured, b"invokeOn:", &[Operand::object(receiver)]).unwrap();
    assert_eq!(host.send(host.named(b"Integer").unwrap(), b"wordOf:", &[Operand::object(held)]).unwrap().as_word(), Some(20));
    host.load_image(&path).unwrap();
    expect(b"[[Generation reconcileCandidate: [DispatchGeneration current]] == [DispatchGeneration current]]");
    expect(b"[[2 imageGenerationValue] = 20]");
    expect(b"[[imageGenerationClosure value] = 21]");
    child(&path, "mapped");
    child(&path, "rebuild");
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
    expect(b"[[[Reflect slotsOf: ownedUnit] size] = 15]");
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
