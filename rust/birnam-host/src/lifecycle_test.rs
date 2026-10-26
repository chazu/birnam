//! Authored lifecycle policy entered from the real owner-thread Host boundary.
use super::*;
use crate::test_hooks::Hooks;
use std::{fs, path::PathBuf};
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
    let fixtures = Hooks::new(&host.context);
    let q = host.scope();
    let cg = host.named(b"Codegen").unwrap();
    let eval = |source: &[u8]| {
        let source = q.string(source).unwrap();
        q.call(cg, b"evalPersisting:", &[source]).unwrap()
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
    let name = host.context.intern(b"hostLifecyclePath").unwrap();
    q.call(globals, b"at:put:", &[name, path]).unwrap();
    q.check(cg, eval(br#"[CompiledUnit compile: "[Module named: #hostLifecycle requires: #[]]; let hostLifecycleTrigger := [false asCell]; let hostLifecycleDrop := [false asCell]; [Reflect defineMethod: #hostLifecycleValue on: Integer as: [:self | 20]]" named: #hostLifecycle output: hostLifecyclePath]"#)).unwrap();
    q.check(cg, eval(b"let hostLifecycleUnit := [CompiledUnit open: hostLifecyclePath]; [hostLifecycleUnit map]")).unwrap();
    expect(b"[[2 hostLifecycleValue] = 10]");
    let unit = eval(b"hostLifecycleUnit");
    let selector = host.context.intern(b"state:").unwrap();
    q.call(unit, b"methodFor:", &[selector]).unwrap();
    let vt = q
        .call(host.context.id.root, b"behaviorOf:", &[unit])
        .unwrap();
    let original = host
        .send(host.context.id.root, b"bind:to:", &[selector, unit])
        .unwrap();
    host.send(
        vt,
        b"addMethod",
        &[
            selector,
            fixtures.method(
                b"deny",
                fixtures.context(&[fixtures.fault(54321), birnam_abi::size(0).unwrap()]),
            ),
        ],
    )
    .unwrap();
    let failure = q
        .evaluate(
            b"[[| [hostLifecycleUnit activate]] value]",
            Path::new("<entered-native-failure>"),
            None,
        )
        .unwrap_err();
    assert!(failure.0.contains("nativeFailure"), "{failure}");
    expect(b"[[hostLifecycleUnit state] = #candidate]");
    expect(b"[[2 hostLifecycleValue] = 10]");
    // Failure retains the attempted operation for explicit undo, even when a
    // descriptive setter failed before the first code write.
    expect(b"[hostLifecycleUnit rollbackAvailable]");
    host.send(vt, b"addMethod", &[selector, original]).unwrap();
    q.check_result(eval(b"[hostLifecycleUnit rollback]"))
        .unwrap();
    expect(b"[[hostLifecycleUnit state] = #rolledBack]");
    let error = q
        .call(
            host.named(b"Error").unwrap(),
            b"kind:",
            &[host.context.intern(b"enteredSetterFailure").unwrap()],
        )
        .unwrap();
    let fault = fixtures.context(&[error, birnam_abi::size(0).unwrap()]);
    host.send(
        vt,
        b"addMethod",
        &[selector, fixtures.method(b"fixedCount", fault)],
    )
    .unwrap();
    assert_eq!(eval(b"[hostLifecycleUnit activate]"), error);
    assert_eq!(fixtures.count(fault, 1), 2); // applying, then failed; original Error is retained
    expect(b"[[hostLifecycleUnit state] = #rolledBack]");
    host.send(vt, b"addMethod", &[selector, original]).unwrap();
    q.check_result(eval(b"[hostLifecycleUnit rollback]"))
        .unwrap();
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
    assert!(
        dir.join("candidate.o").is_file(),
        "discard deleted the artifact"
    );
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
    let fixtures = Hooks::new(&host.context);
    let q = host.scope();
    let cg = host.named(b"Codegen").unwrap();
    let eval = |source: &[u8]| {
        let source = q.string(source).unwrap();
        let result = q.call(cg, b"evalPersisting:", &[source]).unwrap();
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
        let name = host.context.intern(name).unwrap();
        let value = q.string(path.as_os_str().as_bytes()).unwrap();
        q.call(host.named(b"Globals").unwrap(), b"at:put:", &[name, value])
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
        host.load_image_mode(&path, mode == "rebuild" || mode == "drift-rebuild", false)
            .unwrap();
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
            host.send(host.named(b"GC").unwrap(), b"collectPrecisely", &[])
                .unwrap();
            expect(b"[[2 imageOutsideValue] = 9]");
            let filename = q
                .string(
                    path.with_extension(format!("{mode}.resaved.image"))
                        .as_os_str()
                        .as_bytes(),
                )
                .unwrap();
            let saved = q
                .call(host.named(b"System").unwrap(), b"saveImage:", &[filename])
                .unwrap();
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
                .call(host.named(b"System").unwrap(), b"saveImage:", &[filename])
                .unwrap();
            q.check_result(saved).unwrap();
            assert!(resaved.is_file());
            child(&resaved, "resaved");
            fs::remove_file(resaved).unwrap();
        }
        // Portability rejection must not depend on an adopted ledger.
        let selector = host.context.intern(b"imageOutsideValue").unwrap();
        let integer_vt = host.named(b"IntegerVt").unwrap();
        q.call(host.named(b"Integer").unwrap(), b"methodFor:", &[selector])
            .unwrap();
        let original = fixtures.capture(integer_vt, selector);
        host.send(
            integer_vt,
            b"addMethod",
            &[
                selector,
                fixtures.method(
                    b"deny",
                    fixtures.context(&[fixtures.fault(54321), birnam_abi::size(0).unwrap()]),
                ),
            ],
        )
        .unwrap();
        let opaque_path = path.with_extension(format!("{mode}.opaque.image"));
        fs::write(&opaque_path, b"previous image").unwrap();
        let filename = q.string(opaque_path.as_os_str().as_bytes()).unwrap();
        let rejected = q
            .call(host.named(b"System").unwrap(), b"saveImage:", &[filename])
            .unwrap();
        host.send(integer_vt, b"addMethod", &[selector, original])
            .unwrap();
        let kind = q.call(rejected, b"kind", &[]).unwrap();
        assert_eq!(kind, host.context.intern(b"nonPortableGeneration").unwrap());
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
    eval(b"[Module load: #store]");
    eval(b"let savedOriginGeneration := [DispatchGeneration current]; let savedOriginPredecessor := [savedOriginGeneration predecessor]; [db assert: savedOriginGeneration attr: #userAnnotation value: 77]; [db assert: savedOriginPredecessor attr: #sourceMap value: 88]");
    let path = dir.join("active.image");
    let filename = q.string(path.as_os_str().as_bytes()).unwrap();
    let saved = q
        .call(host.named(b"System").unwrap(), b"saveImage:", &[filename])
        .unwrap();
    q.check_result(saved).unwrap();
    assert!(path.is_file(), "managed Image save failed");
    eprintln!("generation Image: returned Error and native application failure");
    let before = eval(b"[DispatchGeneration current]");
    let captured = q
        .call(
            host.named(b"Integer").unwrap(),
            b"methodFor:",
            &[host.context.intern(b"imageGenerationValue").unwrap()],
        )
        .unwrap();
    let receiver = eval(b"2");
    let vt = q
        .call(
            host.context.id.root,
            b"behaviorOf:",
            &[host.named(b"Generation").unwrap()],
        )
        .unwrap();
    let selector = host.context.intern(b"beginImageApplication").unwrap();
    let direct = fixtures.capture(vt, selector);
    let original = host
        .send(
            host.context.id.root,
            b"bind:to:",
            &[selector, host.named(b"Generation").unwrap()],
        )
        .unwrap();
    let kind = host
        .context
        .intern(b"injectedImageApplicationFailure")
        .unwrap();
    let error = q
        .call(host.named(b"Error").unwrap(), b"kind:", &[kind])
        .unwrap();
    for returned_error in [Some(error), None] {
        let fault = fixtures.context(&[
            original,
            host.named(b"GC").unwrap(),
            returned_error.unwrap_or_else(|| fixtures.fault(54322)),
            birnam_abi::size(usize::from(returned_error.is_none())).unwrap(),
            birnam_abi::size(0).unwrap(),
            std::ptr::null_mut(),
        ]);
        host.send(
            vt,
            b"addMethod",
            &[selector, fixtures.method(b"imageFailure", fault)],
        )
        .unwrap();
        let failure = host.load_image(&path).unwrap_err();
        if direct == host.named(b"nil").unwrap() {
            host.send(vt, b"removeMethod:", &[selector]).unwrap();
        } else {
            host.send(vt, b"addMethod", &[selector, direct]).unwrap();
        }
        assert_eq!(fixtures.count(fault, 4), 1, "{failure}");
        expect(b"[[[System imageSaveReport] status] = #partialLoad]");
        expect(b"[[Generation validateImageGeneration] isError]");
        let stale = q
            .call(
                host.named(b"Generation").unwrap(),
                b"reconcileCandidate:",
                &[before],
            )
            .unwrap();
        assert!(
            q.check_result(stale).is_err(),
            "old adoption must not regain authority"
        );
        let held = q.call(captured, b"invokeOn:", &[receiver]).unwrap();
        assert_eq!(
            birnam_abi::as_size(
                host.send(host.named(b"Integer").unwrap(), b"wordOf:", &[held])
                    .unwrap()
            ),
            Some(20)
        );
        // Explicitly reload a known image after a partial/conflicted live load.
        host.load_image(&path).unwrap();
        expect(b"[[[Generation reconcileCandidate: [DispatchGeneration current]] kind] = #generationNotAdopted]");
        expect(b"[[2 imageGenerationValue] = 20]");
        expect(b"[[imageGenerationClosure value] = 21]");
    }
    let held = q.call(captured, b"invokeOn:", &[receiver]).unwrap();
    assert_eq!(
        birnam_abi::as_size(
            host.send(host.named(b"Integer").unwrap(), b"wordOf:", &[held])
                .unwrap()
        ),
        Some(20)
    );
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
    q.evaluate(
        b"[scopedCandidate activate]",
        Path::new("<scoped-activation>"),
        None,
    )
    .unwrap();
    expect(b"[[2 imageGenerationValue] = 30]");
    expect(b"[[2 imageOutsideValue] = 9]");
    eval(b"[Module load: #store]");
    eval(b"let savedOriginGeneration := [DispatchGeneration current]; let savedOriginPredecessor := [savedOriginGeneration predecessor]; let imageCandidate := scopedCandidate; [db assert: savedOriginGeneration attr: #userAnnotation value: 77]; [db assert: savedOriginPredecessor attr: #sourceMap value: 88]");
    let drift_path = dir.join("scoped-drift.image");
    let filename = q.string(drift_path.as_os_str().as_bytes()).unwrap();
    let saved = q
        .call(host.named(b"System").unwrap(), b"saveImage:", &[filename])
        .unwrap();
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
    let fixtures = Hooks::new(&host.context);
    let q = host.scope();
    let cg = host.named(b"Codegen").unwrap();
    let eval = |source: &[u8]| {
        let result = q
            .call(cg, b"evalPersisting:", &[q.string(source).unwrap()])
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
    eval(b"[Module load: #store]");
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
    let vt = q
        .call(host.context.id.root, b"behaviorOf:", &[generation])
        .unwrap();
    let selector = host.context.intern(b"reconcileCandidate:").unwrap();
    let original = fixtures.capture(vt, selector);
    let fault = fixtures.context(&[host.named(b"nil").unwrap(), birnam_abi::size(0).unwrap()]);
    host.send(
        vt,
        b"addMethod",
        &[selector, fixtures.method(b"fixedCount", fault)],
    )
    .unwrap();
    expect(b"[[ownedUnit state] = #active]");
    assert_eq!(
        fixtures.count(fault, 1),
        0,
        "status read performed implicit validation"
    );
    // Authored Generation methods specialize its behavior; the native query
    // can be inherited, so restore absence rather than install a UNIT operand.
    if original == host.named(b"nil").unwrap() {
        host.send(vt, b"removeMethod:", &[selector]).unwrap();
    } else {
        host.send(vt, b"addMethod", &[selector, original]).unwrap();
    }
    let unit = eval(b"ownedUnit");
    let alias = eval(b"unitAlias");
    assert_eq!(unit, alias);
    let names = [b"Store".as_slice(), b"db"];
    let values: Vec<_> = names.iter().map(|n| host.named(n).unwrap()).collect();
    for name in names {
        let key = host.context.intern(name).unwrap();
        host.send(host.context.namespace, b"remove:", &[key])
            .unwrap();
    }
    // Invoke the actual compiled getters/setters without parsing new source.
    let state = host.context.intern(b"withoutStore").unwrap();
    q.call(unit, b"setState:", &[state]).unwrap();
    assert_eq!(q.call(unit, b"state", &[]).unwrap(), state);
    for (name, value) in names.into_iter().zip(values) {
        let key = host.context.intern(name).unwrap();
        host.send(host.context.namespace, b"at:put:", &[key, value])
            .unwrap();
    }
    expect(b"[[db currentValue: ownedUnit attr: #state] = #annotation]");
    let owner = host.global(&q, b"CompiledUnit").unwrap();
    let a = host.context.intern(b"counterA").unwrap();
    let b = host.context.intern(b"counterB").unwrap();
    q.call(owner, b"nextGenerationFor:", &[a]).unwrap();
    let key = host.context.intern(b"+").unwrap();
    let vt = host.named(b"IntegerVt").unwrap();
    let original = fixtures.capture(vt, key);

    let reentry = fixtures.context(&[original, owner, b, birnam_abi::size(1).unwrap()]);
    host.send(
        vt,
        b"addMethod",
        &[key, fixtures.method(b"reenter", reentry)],
    )
    .unwrap();
    let next = q.call(owner, b"nextGenerationFor:", &[a]);
    host.send(vt, b"addMethod", &[key, original]).unwrap();
    assert!(
        fixtures.count(reentry, 3) == 0,
        "counter did not use the current arithmetic send"
    );
    assert_eq!(
        birnam_abi::as_size(
            host.send(
                host.named(b"Integer").unwrap(),
                b"wordOf:",
                &[next.unwrap()]
            )
            .unwrap()
        ),
        Some(2)
    );
    let other = q.call(owner, b"nextGenerationFor:", &[b]).unwrap();
    assert_eq!(
        birnam_abi::as_size(
            host.send(host.named(b"Integer").unwrap(), b"wordOf:", &[other])
                .unwrap()
        ),
        Some(2),
        "counter publication overwrote a reentrant edit to another name"
    );
    let numbers = q.call(owner, b"generationNumbers", &[]).unwrap();
    assert_eq!(q.call(numbers, b"at:", &[b]).unwrap(), other);
    q.call(owner, b"generationNumbers:", &[numbers]).unwrap();
}

#[test]
#[ignore = "requires production native modules"]
fn independent_candidate_modules_need_no_certificate() {
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
        let value = q
            .call(cg, b"evalPersisting:", &[q.string(source).unwrap()])
            .unwrap();
        q.check_result(value).unwrap();
        value
    };
    let expect = |source: &[u8]| {
        assert_eq!(
            eval(source),
            host.named(b"true").unwrap(),
            "{}",
            String::from_utf8_lossy(source)
        )
    };
    let run = |source: &[u8]| {
        q.evaluate(source, Path::new("<scoped-module-change>"), None)
            .unwrap();
    };
    for (name, source) in [
        (b"independentA".as_slice(), b"[Module named: #independentA requires: #[]]; [Reflect defineMethod: #independentA on: Integer as: [:self | 1]]".as_slice()),
        (b"independentB", b"[Module named: #independentB requires: #[]]; [Reflect defineMethod: #independentB on: Integer as: [:self | 2]]"),
    ] { q.evaluate(source, Path::new("<module-baseline>"), Some(name)).unwrap(); }
    let dir = env::temp_dir().join(format!(
        "birnam-independent-candidates-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    for (key, file) in [
        (b"candidateAPath".as_slice(), "a.o"),
        (b"candidateBPath", "b.o"),
    ] {
        q.call(
            host.named(b"Globals").unwrap(),
            b"at:put:",
            &[
                host.context.intern(key).unwrap(),
                q.string(dir.join(file).as_os_str().as_bytes()).unwrap(),
            ],
        )
        .unwrap();
    }
    eval(br#"[CompiledUnit compile: "[Module named: #independentA requires: #[]]; [Reflect defineMethod: #independentA on: Integer as: [:self | 11]]" named: #independentA output: candidateAPath]"#);
    eval(br#"[CompiledUnit compile: "[Module named: #independentB requires: #[]]; [Reflect defineMethod: #independentB on: Integer as: [:self | 22]]" named: #independentB output: candidateBPath]"#);
    eval(b"let unitA := [CompiledUnit open: candidateAPath]; [unitA map]; let unitB := [CompiledUnit open: candidateBPath]; [unitB map]");
    run(b"[unitA activate]");
    q.evaluate(b"[Module named: #introducedLater requires: #[]]; [Reflect defineMethod: #introducedLater on: Integer as: [:self | 33]]", Path::new("<later>"), Some(b"introducedLater")).unwrap();
    run(b"[unitB activate]");
    expect(b"[[3 independentA] = 11]");
    expect(b"[[3 independentB] = 22]");
    expect(b"[[DispatchGeneration current] isNil]");
    run(b"[unitA rollback]");
    expect(b"[[3 independentA] = 1]");
    expect(b"[[3 independentB] = 22]");
    expect(b"[[3 introducedLater] = 33]");
    // Optional certification can be established/refreshed without changing undo.
    eval(b"[DispatchGeneration adoptCurrent]");
    run(b"[unitB rollback]");
    expect(b"[[3 independentB] = 2]");
    eval(b"[DispatchGeneration adoptCurrent]");
    expect(b"[[Generation reconcileCandidate: [DispatchGeneration current]] == [DispatchGeneration current]]");
    // An artifact can be opened afresh and used again after exact module rollback.
    eval(b"let unitAgain := [CompiledUnit open: candidateAPath]; [unitAgain map]");
    run(b"[unitAgain activate]");
    expect(b"[[3 independentA] = 11]");
    run(b"[unitAgain rollback]");
    expect(b"[[3 independentA] = 1]");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "requires production native modules"]
fn discard_cancels_preparation_and_keeps_bindings_and_files() {
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
    let fixtures = Hooks::new(&host.context);
    let q = host.scope();
    let cg = host.named(b"Codegen").unwrap();
    let eval = |source: &[u8]| {
        let v = q
            .call(cg, b"evalPersisting:", &[q.string(source).unwrap()])
            .unwrap();
        q.check_result(v).unwrap();
        v
    };
    let expect = |source: &[u8]| {
        assert_eq!(
            eval(source),
            host.named(b"true").unwrap(),
            "{}",
            String::from_utf8_lossy(source)
        )
    };
    q.evaluate(b"[Module named: #discardScope requires: #[]]; let discardHelper := [| 1]; [Reflect defineMethod: #discardScope on: Integer as: [:self | [discardHelper value]]]",Path::new("<discard-baseline>"),Some(b"discardScope")).unwrap();
    let dir = env::temp_dir().join(format!("birnam-discard-scope-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("candidate.o");
    q.call(
        host.named(b"Globals").unwrap(),
        b"at:put:",
        &[
            host.context.intern(b"discardPath").unwrap(),
            q.string(path.as_os_str().as_bytes()).unwrap(),
        ],
    )
    .unwrap();
    eval(br#"[CompiledUnit compile: "[Module named: #discardScope requires: #[]]; let discardHelper := [| 2]; [Reflect defineMethod: #discardScope on: Integer as: [:self | [discardHelper value]]]" named: #discardScope output: discardPath]"#);
    let tcc = host.named(b"TCC").unwrap();
    let tcc_vt = q
        .call(host.context.id.root, b"behaviorOf:", &[tcc])
        .unwrap();
    let diagnostic_key = host.context.intern(b"mappingIdentityOf:").unwrap();
    q.call(tcc, b"methodFor:", &[diagnostic_key]).unwrap();
    let diagnostic = host
        .send(host.context.id.root, b"bind:to:", &[diagnostic_key, tcc])
        .unwrap();
    host.send(
        tcc_vt,
        b"addMethod",
        &[
            diagnostic_key,
            fixtures.method(
                b"deny",
                fixtures.context(&[fixtures.fault(54321), birnam_abi::size(0).unwrap()]),
            ),
        ],
    )
    .unwrap();
    eval(b"let preparedUnit := [CompiledUnit open: discardPath]; [preparedUnit map]; let abandonedToken := [Generation prepareActivation: preparedUnit plan: [preparedUnit publicationPlan: #activate]]");
    expect(b"[[Generation watchCandidateRetirement: preparedUnit] isError]");
    let bytes = fs::read(&path).unwrap();
    fs::write(&path, b"new author file").unwrap();
    expect(b"[[preparedUnit discard] == preparedUnit]");
    expect(b"[[[Generation commitPrepared: abandonedToken] kind] = #invalidPreparedToken]");
    expect(b"[[3 discardScope] = 1]");
    assert_eq!(fs::read(&path).unwrap(), b"new author file");
    fs::write(&path, bytes).unwrap();
    host.send(tcc_vt, b"addMethod", &[diagnostic_key, diagnostic])
        .unwrap();
    eval(b"let activeUnit := [CompiledUnit open: discardPath]; [activeUnit map]");
    let globals = host.named(b"Globals").unwrap();
    let key = host
        .context
        .intern(b"releaseBindingCompensationFor:")
        .unwrap();
    assert!(
        host.send(host.context.id.root, b"bind:to:", &[key, globals])
            .unwrap()
            .is_null()
    );
    expect(b"[[activeUnit activate] == activeUnit]");
    expect(b"[[activeUnit discard] == activeUnit]");
    expect(b"[[activeUnit state] = #discarded]");
    expect(b"[[activeUnit rollbackAvailable] = false]");
    expect(b"[[3 discardScope] = 2]");
    expect(b"[[[activeUnit rollback] kind] = #rollbackUnavailable]");
    assert!(path.exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "requires production native modules"]
fn dependency_changes_use_registered_module_without_certification() {
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
        let v = q
            .call(cg, b"evalPersisting:", &[q.string(source).unwrap()])
            .unwrap();
        q.check_result(v).unwrap();
        v
    };
    let expect = |source: &[u8]| {
        assert_eq!(
            eval(source),
            host.named(b"true").unwrap(),
            "{}",
            String::from_utf8_lossy(source)
        )
    };
    // A real certificate that predates this module must not become its baseline.
    eval(b"let oldCertificate := [DispatchGeneration adoptCurrent]; [DispatchGeneration current: nil]");
    q.evaluate(b"[Module named: #dependencyReview requires: #[#stdlib]]; [Reflect defineMethod: #dependencyReview on: Integer as: [:self | 1]]", Path::new("<dependency-baseline>"), Some(b"dependencyReview")).unwrap();
    let dir = env::temp_dir().join(format!("birnam-dependency-review-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("candidate.o");
    q.call(
        host.named(b"Globals").unwrap(),
        b"at:put:",
        &[
            host.context.intern(b"dependencyPath").unwrap(),
            q.string(path.as_os_str().as_bytes()).unwrap(),
        ],
    )
    .unwrap();
    eval(br#"[CompiledUnit compile: "[Module named: #dependencyReview requires: #[#stdlib]]; [Reflect defineMethod: #dependencyReview on: Integer as: [:self | 2]]" named: #dependencyReview output: dependencyPath]; let dependencyUnit := [CompiledUnit open: dependencyPath]"#);
    let unchanged = || {
        expect(b"[[[[dependencyUnit dependencyChanges] at: #added] size] = 0]");
        expect(b"[[[[dependencyUnit dependencyChanges] at: #removed] size] = 0]");
    };
    unchanged();
    eval(b"[DispatchGeneration current: oldCertificate]");
    unchanged();
    eval(b"[dependencyUnit map]; [dependencyUnit activate]");
    unchanged();
    eval(b"[dependencyUnit rollback]");
    unchanged();
    // The helper remains a malleable descriptive comparison, not constant empty output.
    eval(b"[dependencyUnit requires: #[#modules]]");
    expect(b"[[[dependencyUnit dependencyChanges] at: #added] = #[#modules]]");
    expect(b"[[[dependencyUnit dependencyChanges] at: #removed] = #[#stdlib]]");
    eval(b"[dependencyUnit unitName: #unregisteredReviewModule]");
    expect(b"[[[dependencyUnit dependencyChanges] kind] = #moduleNotRegistered]");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "requires production native modules"]
fn rollback_keeps_released_predecessors_discarded() {
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
        let v = q
            .call(cg, b"evalPersisting:", &[q.string(source).unwrap()])
            .unwrap();
        q.check_result(v).unwrap();
        v
    };
    let expect = |source: &[u8]| {
        assert_eq!(
            eval(source),
            host.named(b"true").unwrap(),
            "{}",
            String::from_utf8_lossy(source)
        )
    };
    let dir = env::temp_dir().join(format!("birnam-discard-chain-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for mode in 0..4 {
        let name = format!("discardChain{mode}");
        let source = |value| {
            format!(
                "[Module named: #{name} requires: #[]]; [Reflect defineMethod: #{name} on: Integer as: [:self | {value}]]"
            )
        };
        q.evaluate(
            source(1).as_bytes(),
            Path::new("<chain-baseline>"),
            Some(name.as_bytes()),
        )
        .unwrap();
        let candidate = |var: &str, value| {
            let path = dir.join(format!("{name}-{var}.o"));
            let text = format!(
                "[CompiledUnit compile: \"{}\" named: #{name} output: \"{}\"]; let {var} := [CompiledUnit open: \"{}\"]; [{var} map]",
                source(value),
                path.display(),
                path.display()
            );
            eval(text.as_bytes());
        };
        candidate("chainA", 2);
        eval(b"[chainA activate]");
        candidate("chainB", 3);
        if mode == 3 {
            eval(b"let forwardToken := [Generation prepareActivation: chainB plan: [chainB publicationPlan: #activate]]; [chainA discard]");
            expect(b"[[Generation commitPrepared: forwardToken] == chainB]");
        } else {
            eval(b"[chainB activate]");
        }
        if mode == 0 {
            eval(b"[chainA discard]");
        }
        if mode == 2 {
            candidate("chainC", 4);
            eval(b"[chainC activate]");
            eval(b"let chainToken := [Generation prepareRollback: chainC plan: [chainC publicationPlan: #rollback]]; [chainA discard]; [chainB discard]");
            expect(b"[[Generation commitPreparedRollback: chainToken] == chainC]");
            expect(b"[[chainB state] = #discarded]");
            eval(b"[chainC discard]");
        } else {
            eval(b"let chainToken := [Generation prepareRollback: chainB plan: [chainB publicationPlan: #rollback]]");
            if mode == 1 {
                eval(b"[chainA discard]");
            }
            expect(b"[[Generation commitPreparedRollback: chainToken] == chainB]");
            eval(b"[chainB discard]");
        }
        expect(b"[[chainA state] = #discarded]");
        expect(b"[[chainA rollbackAvailable] = false]");
        expect(b"[[[chainA rollback] kind] = #rollbackUnavailable]");
        expect(format!("[[CompiledUnit activeFor: #{name}] isNil]").as_bytes());
        eval(b"[Codegen evictMaterializations]; [System gc]");
        expect(format!("[[3 {name}] = {}]", if mode == 2 { 3 } else { 2 }).as_bytes());
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "requires production native modules"]
fn certificate_release_reclaims_observations_and_preserves_candidate_undo() {
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
        let v = q
            .call(cg, b"evalPersisting:", &[q.string(source).unwrap()])
            .unwrap();
        q.check_result(v).unwrap();
        v
    };
    let expect = |source: &[u8]| {
        assert_eq!(
            eval(source),
            host.named(b"true").unwrap(),
            "{}",
            String::from_utf8_lossy(source)
        )
    };
    let checkpoint = || {
        eval(b"[Codegen evictMaterializations]; [System gc]");
        host.send(host.named(b"TCC").unwrap(), b"checkpoint", &[])
            .unwrap();
    };
    let dir = env::temp_dir().join(format!("birnam-certificate-release-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for (name, var, before, after) in [
        ("releaseCertValue", "certUnit", 1, 2),
        ("releaseUndoValue", "undoUnit", 10, 20),
    ] {
        let source = |value| {
            format!(
                "[Module named: #{name} requires: #[]]; [Reflect defineMethod: #{name} on: Integer as: [:self | {value}]]"
            )
        };
        q.evaluate(
            source(before).as_bytes(),
            Path::new("<certificate-baseline>"),
            Some(name.as_bytes()),
        )
        .unwrap();
        let path = dir.join(format!("{name}.o"));
        eval(format!("[CompiledUnit compile: \"{}\" named: #{name} output: \"{}\"]; let {var} := [CompiledUnit open: \"{}\"]; [{var} map]; [{var} activate]", source(after), path.display(), path.display()).as_bytes());
    }
    eval(b"let retainedCertificate := [DispatchGeneration adoptCurrent]; let retainedDescription := [retainedCertificate sourceMap]; [Generation watchCandidateRetirement: certUnit]; [certUnit discard]");
    expect(b"[[3 releaseCertValue] = 2]");
    eval(b"[Reflect defineMethod: #releaseCertValue on: Integer as: [:self | 3]]");
    checkpoint();
    expect(b"[[Generation retainedGenerationCount] = 1]");
    expect(b"[[Generation retainedGenerationBytes] > 0]");
    // Clearing the descriptive pointer does not implicitly release certification.
    eval(b"[DispatchGeneration current: nil]");
    checkpoint();
    expect(b"[[Generation retainedGenerationCount] = 1]");
    expect(b"[[DispatchGeneration releaseCurrent] == DispatchGeneration]");
    expect(b"[[DispatchGeneration current] isNil]");
    expect(
        b"[[[Generation reconcileCandidate: retainedCertificate] kind] = #generationNotAdopted]",
    );
    expect(b"[[retainedCertificate sourceMap] == retainedDescription]");
    expect(b"[[undoUnit rollbackAvailable] = true]");
    checkpoint();
    expect(b"[[Generation retainedGenerationCount] = 0]");
    expect(b"[[Generation retainedGenerationBytes] = 0]");
    expect(b"[[3 releaseCertValue] = 3]");
    expect(b"[[3 releaseUndoValue] = 20]");
    expect(b"[[undoUnit rollback] == undoUnit]");
    expect(b"[[3 releaseUndoValue] = 10]");
    expect(b"[[DispatchGeneration releaseCurrent] == DispatchGeneration]");
    // Explicit fresh adoption remains available after ordinary code matches source.
    eval(b"[Reflect defineMethod: #releaseCertValue on: Integer as: [:self | 2]]; [DispatchGeneration adoptCurrent]");
    expect(b"[[Generation reconcileCandidate: [DispatchGeneration current]] == [DispatchGeneration current]]");
    expect(b"[[DispatchGeneration releaseCurrent] == DispatchGeneration]");
    expect(b"[[DispatchGeneration current] isNil]");
    expect(b"[[3 releaseCertValue] = 2]");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "requires production native modules"]
fn image_application_boundary_invalidates_without_restore() {
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
    let fixtures = Hooks::new(&host.context);
    let q = host.scope();
    let cg = host.named(b"Codegen").unwrap();
    let eval = |source: &[u8]| {
        let v = q
            .call(cg, b"evalPersisting:", &[q.string(source).unwrap()])
            .unwrap();
        q.check_result(v).unwrap();
        v
    };
    let expect = |source: &[u8]| {
        assert_eq!(
            eval(source),
            host.named(b"true").unwrap(),
            "{}",
            String::from_utf8_lossy(source)
        )
    };
    let dir = env::temp_dir().join(format!("birnam-image-application-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let source = |value| {
        format!(
            "[Module named: #imageBoundaryValue requires: #[]]; [Reflect defineMethod: #imageBoundaryValue on: Integer as: [:self | {value}]]"
        )
    };
    q.evaluate(
        source(1).as_bytes(),
        Path::new("<image-boundary>"),
        Some(b"imageBoundaryValue"),
    )
    .unwrap();
    let path = dir.join("candidate.o");
    eval(format!("[CompiledUnit compile: \"{}\" named: #imageBoundaryValue output: \"{}\"]; let boundaryUnit := [CompiledUnit open: \"{}\"]; [boundaryUnit map]; [boundaryUnit activate]; [DispatchGeneration adoptCurrent]", source(2), path.display(), path.display()).as_bytes());
    eval(b"let boundaryToken := [Generation prepareRollback: boundaryUnit plan: [boundaryUnit publicationPlan: #rollback]]");
    let image_path = dir.join("boundary.image");
    let saved = q
        .call(
            host.named(b"System").unwrap(),
            b"saveImage:",
            &[q.string(image_path.as_os_str().as_bytes()).unwrap()],
        )
        .unwrap();
    q.check_result(saved).unwrap();
    assert!(image_path.is_file());

    // A collecting failure after preparation, before application, retains the
    // original candidate, certificate and a usable prepared rollback token.
    let vt = q.call(host.context.id.root, b"behaviorOf:", &[cg]).unwrap();
    let selector = host.context.intern(b"finishPortableImageUnit").unwrap();
    let original = fixtures.capture(vt, selector);
    let fault = fixtures.context(&[
        original,
        host.named(b"GC").unwrap(),
        fixtures.fault(54322),
        birnam_abi::size(1).unwrap(),
        birnam_abi::size(0).unwrap(),
        std::ptr::null_mut(),
    ]);
    host.send(
        vt,
        b"addMethod",
        &[selector, fixtures.method(b"imageFailure", fault)],
    )
    .unwrap();
    let failed = host.load_image(&image_path);
    host.send(vt, b"addMethod", &[selector, original]).unwrap();
    assert!(failed.is_err());
    assert_eq!(fixtures.count(fault, 4), 1);
    expect(b"[[CompiledUnit activeFor: #imageBoundaryValue] == boundaryUnit]");
    expect(b"[[Generation reconcileCandidate: [DispatchGeneration current]] == [DispatchGeneration current]]");
    expect(b"[[Generation commitPreparedRollback: boundaryToken] == boundaryUnit]");
    expect(b"[[3 imageBoundaryValue] = 1]");
    eval(
        format!(
            "let boundaryUnit := [CompiledUnit open: \"{}\"]",
            path.display()
        )
        .as_bytes(),
    );
    eval(b"[boundaryUnit map]");
    expect(b"[[boundaryUnit activate] == boundaryUnit]");
    eval(b"[DispatchGeneration adoptCurrent]");
    eval(b"let boundaryToken := [Generation prepareRollback: boundaryUnit plan: [boundaryUnit publicationPlan: #rollback]]");
    expect(b"[[boundaryToken unit] == boundaryUnit]");
    eval(b"let boundaryHeld := [3 methodFor: #imageBoundaryValue]");

    // Invalidation happens at begin, not at later success/abandonment. Cleanup
    // cannot resurrect old process-local authority or unmap held executable code.
    eval(b"[Generation beginImageApplication]");
    expect(b"[[[CompiledUnit activeFor: #imageBoundaryValue] kind] = #imageApplicationActive]");
    eval(b"[Generation finishImageApplication]; [Codegen evictMaterializations]; [System gc]");
    expect(b"[[CompiledUnit activeFor: #imageBoundaryValue] isNil]");
    expect(b"[[[Generation commitPreparedRollback: boundaryToken] kind] = #invalidPreparedToken]");
    expect(b"[[[Generation reconcileCandidate: [DispatchGeneration current]] kind] = #generationNotAdopted]");
    expect(b"[[boundaryUnit rollback] isError]");
    expect(b"[[boundaryHeld invokeOn: 3] = 2]");
    expect(b"[[3 imageBoundaryValue] = 2]");

    // Image now restores the ordinary source owner itself; there is no native
    // source stage or registry inverse for cleanup to exchange back.
    eval(b"[Generation finishImageApplication]; [DispatchGeneration adoptCurrent]");
    expect(b"[[Generation reconcileCandidate: [DispatchGeneration current]] == [DispatchGeneration current]]");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "requires production native modules"]
fn source_registry_sends_retain_partial_edits_for_explicit_undo() {
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
    let fixtures = Hooks::new(&host.context);
    let q = host.scope();
    let cg = host.named(b"Codegen").unwrap();
    let eval = |source: &[u8]| {
        let v = q
            .call(cg, b"evalPersisting:", &[q.string(source).unwrap()])
            .unwrap();
        q.check_result(v).unwrap();
        v
    };
    let expect = |source: &[u8]| {
        assert_eq!(
            eval(source),
            host.named(b"true").unwrap(),
            "{}",
            String::from_utf8_lossy(source)
        )
    };
    let owner = host.global(&q, b"SourceRegistry").unwrap();
    let vt = q
        .call(host.context.id.root, b"behaviorOf:", &[owner])
        .unwrap();
    let selector = host
        .context
        .intern(b"replaceRecord:named:expected:")
        .unwrap();
    let _original_method = q.call(owner, b"methodFor:", &[selector]).unwrap();
    let original = fixtures.capture(vt, selector);
    let registry_name = host.context.intern(b"SourceRegistry").unwrap();
    let error = q
        .call(
            host.named(b"Error").unwrap(),
            b"kind:",
            &[host.context.intern(b"sourceWriteFailure").unwrap()],
        )
        .unwrap();
    let dir = env::temp_dir().join(format!("birnam-source-owner-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for mode in 0..8 {
        let name = format!("sourceOwnerMode{mode}");
        let symbol = host.context.intern(name.as_bytes()).unwrap();
        let source = |value| {
            format!(
                "[Module named: #{name} requires: #[]]; [Reflect defineMethod: #{name} on: Integer as: [:self | {value}]]"
            )
        };
        q.evaluate(
            source(1).as_bytes(),
            Path::new("<source-owner>"),
            Some(name.as_bytes()),
        )
        .unwrap();
        let before = q.call(owner, b"recordFor:", &[symbol]).unwrap();
        let path = dir.join(format!("{name}.o"));
        eval(
            format!(
                "[CompiledUnit compile: \"{}\" named: #{name} output: \"{}\"]",
                source(2),
                path.display()
            )
            .as_bytes(),
        );
        eval(
            format!(
                "let sourceUnit := [CompiledUnit open: \"{}\"]",
                path.display()
            )
            .as_bytes(),
        );
        eval(b"[sourceUnit map]");
        if mode == 0 {
            eval(b"let savedRecordRead := [SourceRegistry methodFor: #recordFor:]; let sourceReadFailure := [Error kind: #sourceReadFailure]; [Reflect defineMethod: #recordFor: on: SourceRegistry as: [:self :name | sourceReadFailure]]");
            expect(b"[[CompiledUnit activeFor: #sourceOwnerMode0] == sourceReadFailure]");
            expect(b"[[sourceUnit activate] == sourceReadFailure]");
            eval(
                b"[[Reflect vtableOf: SourceRegistry] addMethod: savedRecordRead as: #recordFor:]",
            );
            expect(b"[[3 sourceOwnerMode0] = 1]");
            expect(b"[[[[Generation candidatePolicyCellFor: sourceUnit] value] at: #busy] not]");
        }
        let other = q.call(owner, b"clone", &[]).unwrap();
        let changed_name = if mode == 2 {
            symbol
        } else {
            host.context
                .intern(format!("sourceOutside{mode}").as_bytes())
                .unwrap()
        };
        let changed_text = if mode == 2 {
            source(3)
        } else {
            format!("[Module named: #sourceOutside{mode} requires: #[]]")
        };
        let changed_record = q
            .call(
                cg,
                b"analyzeSourceMetadata:",
                &[q.string(changed_text.as_bytes()).unwrap()],
            )
            .unwrap();
        q.check_result(changed_record).unwrap();
        let fault = fixtures.context(&[
            original,
            host.named(b"GC").unwrap(),
            host.named(b"Globals").unwrap(),
            registry_name,
            other,
            changed_name,
            changed_record,
            error,
            fixtures.fault(54323),
            birnam_abi::size(mode).unwrap(),
            birnam_abi::size(0).unwrap(),
            std::ptr::null_mut(),
        ]);
        host.send(
            vt,
            b"addMethod",
            &[selector, fixtures.method(b"registryFailure", fault)],
        )
        .unwrap();
        let result = q.call(
            cg,
            b"evalPersisting:",
            &[q.string(b"[sourceUnit activate]").unwrap()],
        );
        host.send(vt, b"addMethod", &[selector, original]).unwrap();
        assert!(
            fixtures.count(fault, 10) > 0,
            "mode {mode} bypassed the owner setter"
        );
        if mode == 6 || mode == 7 {
            q.check_result(result.unwrap()).unwrap();
        } else {
            assert!(
                result.is_err() || q.check_result(result.unwrap()).is_err(),
                "mode {mode} swallowed failure"
            );
        }
        // No automatic inverse: methods written before the source failure stay live.
        expect(format!("[[3 {name}] = 2]").as_bytes());
        expect(b"[sourceUnit rollbackAvailable]");
        if mode == 7 {
            assert_eq!(q.call(owner, b"recordFor:", &[symbol]).unwrap(), before);
        }
        if mode == 4 {
            assert_eq!(host.global(&q, b"SourceRegistry").unwrap(), other);
            assert_eq!(q.call(other, b"recordFor:", &[symbol]).unwrap(), before);
        }
        if mode == 2 {
            assert_eq!(
                q.call(owner, b"recordFor:", &[symbol]).unwrap(),
                changed_record
            );
            expect(b"[[[sourceUnit rollback] kind] = #undoConflict]");
            expect(format!("[[3 {name}] = 2]").as_bytes());
            let unit = host.global(&q, b"sourceUnit").unwrap();
            let info = q
                .call(
                    host.named(b"Generation").unwrap(),
                    b"candidateDataFor:",
                    &[unit],
                )
                .unwrap();
            let after = q.vector_at(info, 2).unwrap();
            q.call(owner, b"replaceCompiledSource:named:", &[after, symbol])
                .unwrap();
        }
        eval(b"[sourceUnit rollback]");
        assert_eq!(
            q.call(owner, b"recordFor:", &[symbol]).unwrap(),
            before,
            "mode {mode}"
        );
        if mode == 4 {
            assert_eq!(
                host.global(&q, b"SourceRegistry").unwrap(),
                other,
                "undo overwrote a rebound owner"
            );
            q.call(
                host.named(b"Globals").unwrap(),
                b"at:put:",
                &[registry_name, owner],
            )
            .unwrap();
        }
        if mode == 3 || mode == 6 {
            assert_eq!(
                q.call(owner, b"recordFor:", &[changed_name]).unwrap(),
                changed_record
            );
        }
        expect(format!("[[3 {name}] = 1]").as_bytes());
        eval(b"[sourceUnit discard]; [System gc]");
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "requires production native modules"]
fn source_registry_images_preserve_aliases_and_reject_changed_snapshots() {
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
    let fixtures = Hooks::new(&host.context);
    let q = host.scope();
    let cg = host.named(b"Codegen").unwrap();
    let system = host.named(b"System").unwrap();
    let eval = |source: &[u8]| {
        let v = q
            .call(cg, b"evalPersisting:", &[q.string(source).unwrap()])
            .unwrap();
        q.check_result(v).unwrap();
        v
    };
    let expect = |source: &[u8]| {
        assert_eq!(
            eval(source),
            host.named(b"true").unwrap(),
            "{}",
            String::from_utf8_lossy(source)
        )
    };
    let dir = env::temp_dir().join(format!("birnam-source-images-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("owner.image");
    let filename = q.string(path.as_os_str().as_bytes()).unwrap();
    eval(b"let sourceOwnerAlias := SourceRegistry; let sourceStateAlias := [SourceRegistry state]; let sourceRowsAlias := [SourceRegistry entries]");
    let old_owner = host.global(&q, b"SourceRegistry").unwrap();
    let saved = q.call(system, b"saveImage:", &[filename]).unwrap();
    q.check_result(saved).unwrap();
    assert!(path.is_file());
    for rebuild in [false, true] {
        let result = q
            .call(
                system,
                b"loadImage:rebuild:",
                &[
                    filename,
                    host.named(if rebuild { b"true" } else { b"false" })
                        .unwrap(),
                ],
            )
            .unwrap();
        q.check_result(result).unwrap();
        let current = host.global(&q, b"SourceRegistry").unwrap();
        assert_ne!(old_owner, current);
        expect(b"[SourceRegistry == sourceOwnerAlias]");
        expect(b"[[SourceRegistry state] == sourceStateAlias]");
        expect(b"[[SourceRegistry entries] == sourceRowsAlias]");
        expect(b"[[[Codegen portableImageSources] at: 1] == SourceRegistry]");
        expect(b"[[SourceRegistry validate] == SourceRegistry]");
        eval(b"[System gc]");
    }
    let owner = host.global(&q, b"SourceRegistry").unwrap();
    let vt = q
        .call(host.context.id.root, b"behaviorOf:", &[owner])
        .unwrap();
    let selector = host.context.intern(b"validate").unwrap();
    let original = q.call(owner, b"methodFor:", &[selector]).unwrap();
    eval(b"let sourceSaveError := [Error kind: #sourceSaveRefused]; [Reflect defineMethod: #validate on: SourceRegistry as: [:self | sourceSaveError]]");
    let rejected_path = dir.join("rejected.image");
    let rejected = q.string(rejected_path.as_os_str().as_bytes()).unwrap();
    let answer = q.call(system, b"saveImage:", &[rejected]).unwrap();
    assert_eq!(answer, host.global(&q, b"sourceSaveError").unwrap());
    assert!(!rejected_path.exists());
    q.call(vt, b"addMethod:as:", &[original, selector]).unwrap();

    // Registry data is an explicit save dependency. This callback changes only
    // ordinary registry/Cell state: no Globals binding or dictionary edit.
    eval(b"let sourceValidation := [SourceRegistry methodFor: #validate]; let sourceSaveOnce := [Cell with: true]; let sourceSaveRecord := [Codegen analyzeSourceMetadata: \"[Module named: #registeredDuringSave requires: #[]]\"]");
    eval(b"[Reflect defineMethod: #validate on: SourceRegistry as: [:self | [[sourceSaveOnce value] ifTrue: [| [sourceSaveOnce set: false]; [self recordCompiledSource: sourceSaveRecord named: #registeredDuringSave]]]; [sourceValidation invokeOn: self]]]");
    let epoch = birnam_abi::as_size(
        host.send(host.context.id.root, b"bindingEpoch", &[])
            .unwrap(),
    )
    .unwrap();
    let basis = q
        .call(host.named(b"Globals").unwrap(), b"bindingBasis", &[])
        .unwrap();
    let answer = q.call(system, b"saveImage:", &[rejected]).unwrap();
    assert_eq!(answer, host.named(b"nil").unwrap());
    assert!(!rejected_path.exists());
    assert_eq!(
        birnam_abi::as_size(
            host.send(host.context.id.root, b"bindingEpoch", &[])
                .unwrap()
        ),
        Some(epoch)
    );
    let after = q
        .call(host.named(b"Globals").unwrap(), b"bindingBasis", &[])
        .unwrap();
    assert_eq!(
        birnam_abi::as_size(
            host.send(host.named(b"Integer").unwrap(), b"wordOf:", &[basis])
                .unwrap()
        ),
        birnam_abi::as_size(
            host.send(host.named(b"Integer").unwrap(), b"wordOf:", &[after])
                .unwrap()
        )
    );
    expect(b"[[SourceRegistry recordFor: #registeredDuringSave] == sourceSaveRecord]");
    q.call(vt, b"addMethod:as:", &[original, selector]).unwrap();

    // Substitute an obsolete inventory at the actual pre-application boundary.
    // The loader has a real active stage; rejection cannot be a missing-stage error.
    let cvt = q.call(host.context.id.root, b"behaviorOf:", &[cg]).unwrap();
    let cselector = host
        .context
        .intern(b"preparePortableImageSources:")
        .unwrap();
    let centry = fixtures.capture(cvt, cselector);
    let legacy = q.call(owner, b"sourceRegistry", &[]).unwrap();
    let marker = fixtures.context(&[centry, legacy, birnam_abi::size(0).unwrap()]);
    host.send(
        cvt,
        b"addMethod",
        &[cselector, fixtures.method(b"legacySource", marker)],
    )
    .unwrap();
    let result = host.load_image(&path);
    host.send(cvt, b"addMethod", &[cselector, centry]).unwrap();
    assert!(result.is_err());
    assert_eq!(fixtures.count(marker, 2), 1);
    assert_eq!(host.global(&q, b"SourceRegistry").unwrap(), owner);
    expect(b"[[SourceRegistry validate] == SourceRegistry]");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "requires production native modules"]
fn registry_validation_uses_one_metadata_read_and_current_query() {
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
    let fixtures = Hooks::new(&host.context);
    let q = host.scope();
    let cg = host.named(b"Codegen").unwrap();
    let owner = host.global(&q, b"SourceRegistry").unwrap();
    let name = host.context.intern(b"stdlib").unwrap();
    let record = q.call(owner, b"recordFor:", &[name]).unwrap();
    let vt = q.call(host.context.id.root, b"behaviorOf:", &[cg]).unwrap();
    let selector = host.context.intern(b"retainedSourceMetadataOf:").unwrap();
    let original = fixtures.capture(vt, selector);
    let counter = fixtures.context(&[original, birnam_abi::size(0).unwrap()]);
    host.send(
        vt,
        b"addMethod",
        &[selector, fixtures.method(b"counted", counter)],
    )
    .unwrap();
    let checked = q.call(owner, b"validateRecord:named:", &[record, name]);
    host.send(vt, b"addMethod", &[selector, original]).unwrap();
    assert_eq!(checked.unwrap(), record);
    assert_eq!(
        fixtures.count(counter, 1),
        1,
        "registry validation decoded retained metadata redundantly"
    );
    let error = q
        .call(
            host.named(b"Error").unwrap(),
            b"kind:",
            &[host.context.intern(b"metadataQueryRefused").unwrap()],
        )
        .unwrap();
    for (query, expected_calls, expected) in [
        (b"sourceRevisionOf:".as_slice(), 0, record),
        (b"moduleMetadataOf:".as_slice(), 1, error),
    ] {
        let selector = host.context.intern(query).unwrap();
        let original = fixtures.capture(vt, selector);
        let fault = fixtures.context(&[error, birnam_abi::size(0).unwrap()]);
        host.send(
            vt,
            b"addMethod",
            &[selector, fixtures.method(b"fixedCount", fault)],
        )
        .unwrap();
        let actual = q.call(owner, b"validateRecord:named:", &[record, name]);
        host.send(vt, b"addMethod", &[selector, original]).unwrap();
        assert_eq!(actual.unwrap(), expected);
        assert_eq!(fixtures.count(fault, 1), expected_calls);
    }
}

#[test]
#[ignore = "requires production native modules"]
fn direct_method_observations_restore_native_abi_and_owned_identity() {
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
    let fixtures = Hooks::new(&host.context);
    let q = host.scope();
    let execution = host.named(b"Execution").unwrap();
    let vt = host.named(b"IntegerVt").unwrap();
    let key = q.keep(host.context.intern(b"word").unwrap()).unwrap();
    let original = fixtures.capture(vt, key);
    let held = q
        .call(execution, b"directMethodFor:in:", &[key, vt])
        .unwrap();
    assert_eq!(
        q.call(held, b"bindingMatchesIn:as:", &[vt, key]).unwrap(),
        host.named(b"true").unwrap()
    );
    host.send(vt, b"removeMethod:", &[key]).unwrap();
    host.send(host.named(b"GC").unwrap(), b"collectPrecisely", &[])
        .unwrap();
    assert_eq!(
        q.call(held, b"bindingMatchesIn:as:", &[vt, key]).unwrap(),
        host.named(b"false").unwrap()
    );
    q.call(vt, b"addMethod:as:", &[held, key]).unwrap();
    let restored = fixtures.capture(vt, key);
    assert_eq!(
        host.send(
            host.context.id.root,
            b"sameNativeMethod:as:",
            &[restored, original]
        )
        .unwrap(),
        birnam_abi::size(1).unwrap(),
        "restoration changed the captured native Method"
    );
    let integer = q
        .keep(
            host.send(
                host.named(b"Integer").unwrap(),
                b"fromWord:",
                &[birnam_abi::size(71).unwrap()],
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        birnam_abi::as_size(host.send(integer, b"word", &[]).unwrap()),
        Some(71)
    );
    q.evaluate(b"let bindingProbe := [Object clone]; [Reflect defineMethod: #answer on: bindingProbe as: [:self | 29]]",Path::new("<binding-probe>"),None).unwrap();
    let target = host.global(&q, b"bindingProbe").unwrap();
    let vt = q
        .call(host.context.id.root, b"behaviorOf:", &[target])
        .unwrap();
    let key = q.keep(host.context.intern(b"answer").unwrap()).unwrap();
    let one = q
        .call(execution, b"directMethodFor:in:", &[key, vt])
        .unwrap();
    let two = q
        .call(execution, b"directMethodFor:in:", &[key, vt])
        .unwrap();
    assert_eq!(one, two);
    assert_ne!(one, host.named(b"nil").unwrap());
    host.send(vt, b"removeMethod:", &[key]).unwrap();
    host.send(host.named(b"GC").unwrap(), b"collectPrecisely", &[])
        .unwrap();
    q.call(vt, b"addMethod:as:", &[one, key]).unwrap();
    let answer = q.call(target, b"answer", &[]).unwrap();
    q.check_result(answer).unwrap();
    assert_eq!(
        birnam_abi::as_size(
            host.send(host.named(b"Integer").unwrap(), b"wordOf:", &[answer])
                .unwrap()
        ),
        Some(29)
    );
}

#[test]
#[ignore = "requires production native modules"]
fn candidate_task_exit_retains_explicit_undo_and_releases_operation_scope() {
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
        let v = q
            .call(cg, b"evalPersisting:", &[q.string(source).unwrap()])
            .unwrap();
        q.check_result(v).unwrap();
        v
    };
    let expect = |source: &[u8]| {
        assert_eq!(
            eval(source),
            host.named(b"true").unwrap(),
            "{}",
            String::from_utf8_lossy(source)
        )
    };
    let dir = env::temp_dir().join(format!("birnam-candidate-task-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("task.o");
    q.evaluate(b"[Module named: #candidateTask requires: #[]]; [Reflect defineMethod: #candidateTaskValue on: Integer as: [:self | 1]]",Path::new("<candidate-task>"),Some(b"candidateTask")).unwrap();
    eval(format!("[CompiledUnit compile: \"[Module named: #candidateTask requires: #[]]; [Reflect defineMethod: #candidateTaskValue on: Integer as: [:self | 2]]\" named: #candidateTask output: \"{}\"]",path.display()).as_bytes());
    eval(
        format!(
            "let taskCandidate := [CompiledUnit open: \"{}\"]; [taskCandidate map]",
            path.display()
        )
        .as_bytes(),
    );
    eval(br#"
[Globals at: #outsidePolicyWrites put: [Cell with: 0]];
[Globals at: #checkedPolicyWrites put: [Cell with: 0]];
[Globals at: #interruptCandidate put: [Cell with: true]];
let taskSourceSetter := [SourceRegistry methodFor: #replaceRecord:named:expected:];
[Reflect defineMethod: #replaceRecord:named:expected: on: SourceRegistry as: [:self :record :name :expected |
  [checkedPolicyWrites inc];
  [[[[[TaskExecutor current] field: #transitionDepth] value] = 0] ifTrue: [| [outsidePolicyWrites inc]]];
  [taskSourceSetter invokeOn: self arguments: #[record name expected]];
  [[interruptCandidate value] ifTrue: [| [[TaskExecutor current] fail: #candidateInterrupted]] ifFalse: [| self]]
]]
"#);
    expect(b"[[checkedPolicyWrites value] = 0]");
    eval(b"let candidateTaskRun := [TaskControl start: [:task | [taskCandidate activate]]]");
    expect(b"[[[candidateTaskRun pollOutcome] at: 0] = #failure]");
    expect(b"[[[candidateTaskRun field: #transitionDepth] value] = 0]");
    eval(b"[interruptCandidate set: false]");
    expect(b"[[3 candidateTaskValue] = 2]");
    expect(b"[taskCandidate rollbackAvailable]");
    expect(b"[[[[Generation candidatePolicyCellFor: taskCandidate] value] at: #busy] = false]");
    eval(b"let candidateUndoTask := [TaskControl start: [:task | [taskCandidate rollback]]]");
    expect(b"[[[candidateUndoTask pollOutcome] at: 0] = #success]");
    let writes = eval(b"[checkedPolicyWrites value]");
    assert_eq!(
        birnam_abi::as_size(
            host.send(host.named(b"Integer").unwrap(), b"wordOf:", &[writes])
                .unwrap()
        ),
        Some(2)
    );
    expect(b"[[outsidePolicyWrites value] = 0]");
    expect(b"[[3 candidateTaskValue] = 1]");
    eval(b"[taskCandidate discard]");
    fs::remove_dir_all(dir).unwrap();
}
