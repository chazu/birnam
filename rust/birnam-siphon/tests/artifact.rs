//! Inert Artifact inspection through public Oop owners and scoped roots.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use std::path::Path;
fn fixture(abi: &[u8], target: &[u8], native: &[&[u8]]) -> Vec<u8> {
    fn field(out: &mut Vec<u8>, b: &[u8]) {
        out.extend_from_slice(&(b.len() as u32).to_le_bytes());
        out.extend_from_slice(b);
    }
    let mut payload = Vec::new();
    for value in [
        b"module".as_slice(),
        b"candidate",
        abi,
        target,
        b"source-digest",
        b"revision-digest",
    ] {
        field(&mut payload, value);
    }
    for list in [
        &[b"depA".as_slice(), b"depB".as_slice()][..],
        native,
        &[b"activate".as_slice()][..],
    ] {
        payload.extend_from_slice(&(list.len() as u32).to_le_bytes());
        for value in list {
            field(&mut payload, value);
        }
    }
    let mut bytes = b"BIRNAM-ARTIFACT-DESCRIPTOR-V1".to_vec();
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend(payload);
    bytes
}
#[test]
#[ignore = "requires production Oop modules"]
fn artifact_inspection_uses_messages_and_traced_descriptors() {
    let host = oop::bootstrap();
    let q = host.scope();
    let hooks = oop::hooks::Hooks::new(&host);
    let named = |n: &[u8]| host.named(n).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let text = |b: &[u8]| q.string(b).unwrap();
    let bytes = |v| q.read_bytes(call(v, b"bytes", &[])).unwrap();
    let integer = |n| call(named(b"Integer"), b"fromWord:", &[q.word(n).unwrap()]);
    let number = |v| {
        q.word_bits(call(named(b"Integer"), b"wordOf:", &[v]))
            .unwrap()
    };
    let gc = named(b"GC");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let artifact = named(b"Artifact");
    let nil = named(b"nil");
    assert_eq!(call(artifact, b"lastFailure", &[]), nil);
    let dir =
        std::env::temp_dir().join(format!("birnam-artifact-inspection-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("candidate.dylib");
    let path_obj = text(path.as_os_str().as_encoded_bytes());
    let abi = b"birnam-artifact-generation-v1";
    let target = b"aarch64-macos";
    let good = fixture(abi, target, &[b"libdep.dylib"]);
    let failure = |want: &[u8]| assert_eq!(call(artifact, b"lastFailure", &[]), key(want));
    for args in [vec![], vec![nil], vec![path_obj, path_obj]] {
        assert_eq!(call(artifact, b"inspect:", &args), nil);
        failure(b"artifactPathInvalid");
    }
    assert_eq!(call(artifact, b"inspect:", &[path_obj]), nil);
    failure(b"artifactUnavailable");
    for (data, reason) in [
        (b"junk".to_vec(), b"artifactDescriptionMissing".as_slice()),
        (
            good[..good.len() - 2].to_vec(),
            b"artifactDescriptionMissing",
        ),
        (
            [good.clone(), good.clone()].concat(),
            b"artifactDescriptionMissing",
        ),
        (fixture(b"wrong", target, &[]), b"artifactAbiMismatch"),
        (fixture(abi, b"wrong", &[]), b"artifactTargetMismatch"),
        (good.clone(), b"nativeRequirementUnavailable"),
        (
            fixture(abi, target, &[b"../escape"]),
            b"nativeRequirementUnavailable",
        ),
    ] {
        std::fs::write(&path, data).unwrap();
        assert_eq!(call(artifact, b"inspect:", &[path_obj]), nil);
        failure(reason);
    }
    let file = std::fs::File::create(&path).unwrap();
    file.set_len((1u64 << 30) + 1).unwrap();
    drop(file);
    assert_eq!(call(artifact, b"inspect:", &[path_obj]), nil);
    failure(b"artifactUnavailable");
    std::fs::write(dir.join("libdep.dylib"), b"present").unwrap();
    let marker = dir.join("activated");
    let c = dir.join("candidate.c");
    let encoded = good
        .iter()
        .map(|b| b.to_string())
        .collect::<Vec<_>>()
        .join(",");
    std::fs::write(&c,format!("#include <stdio.h>\n__attribute__((used)) const unsigned char descriptor[] = {{{encoded}}};\n__attribute__((constructor)) static void activate(void) {{ FILE *f = fopen(\"{}\", \"w\"); if (f) fclose(f); }}\n",marker.display())).unwrap();
    assert!(
        std::process::Command::new("cc")
            .args(["-dynamiclib", "-Wall", "-Wextra", "-Werror", "-o"])
            .arg(&path)
            .arg(&c)
            .status()
            .unwrap()
            .success()
    );
    let mut forwards = vec![];
    for (family, name) in [
        (b"String".as_slice(), b"fromBytes:".as_slice()),
        (b"String", b"bytes"),
        (b"Integer", b"fromWord:"),
        (b"Integer", b"wordOf:"),
        (b"Vector", b"new"),
        (b"Vector", b"add:"),
        (b"ObjectModel", b"allocate:bytes:"),
        (b"ObjectModel", b"slotAt:in:put:"),
        (b"ObjectModel", b"slotAt:in:"),
    ] {
        let vt = call(host.id.root, b"behaviorOf:", &[named(family)]);
        let selector = key(name);
        let original = call(vt, b"lookup", &[selector]);
        let state = hooks.context(&[original, gc, size(0).unwrap()]);
        let forward = hooks.method(b"collectEvery", state);
        call(vt, b"addMethod", &[selector, forward]);
        forwards.push((vt, selector, original, state, forward));
    }
    let desc = call(artifact, b"inspect:", &[path_obj]);
    assert_ne!(desc, nil);
    assert!(!marker.exists());
    assert_eq!(call(artifact, b"lastFailure", &[]), nil);
    assert_eq!(
        call(host.id.root, b"behaviorOf:", &[desc]),
        named(b"ArtifactDescriptorVt")
    );
    for (n, v) in [
        (b"kind".as_slice(), b"module".as_slice()),
        (b"identity", b"candidate"),
    ] {
        assert_eq!(call(desc, n, &[]), key(v));
    }
    for (n, v) in [
        (b"artifactAbi".as_slice(), abi.as_slice()),
        (b"target", target.as_slice()),
        (b"sourceDigest", b"source-digest"),
        (b"revisionDigest", b"revision-digest"),
        (b"path", path.as_os_str().as_encoded_bytes()),
    ] {
        assert_eq!(bytes(call(desc, n, &[])), v);
    }
    let zero = integer(0);
    let one = integer(1);
    let negative = integer(usize::MAX);
    for (list, count, at, expected, strings) in [
        (
            b"roots".as_slice(),
            b"rootsCount".as_slice(),
            b"rootAt:".as_slice(),
            vec![b"candidate".as_slice()],
            false,
        ),
        (
            b"requires",
            b"requiresCount",
            b"requireAt:",
            vec![b"depA".as_slice(), b"depB".as_slice()],
            false,
        ),
        (
            b"nativeRequires",
            b"nativeRequiresCount",
            b"nativeRequireAt:",
            vec![b"libdep.dylib".as_slice()],
            true,
        ),
        (
            b"capabilities",
            b"capabilitiesCount",
            b"capabilityAt:",
            vec![b"activate".as_slice()],
            false,
        ),
    ] {
        assert_eq!(number(call(desc, count, &[])), expected.len());
        let vector = call(desc, list, &[]);
        assert_eq!(number(call(vector, b"size", &[])), expected.len());
        for (i, want) in expected.iter().enumerate() {
            let index = integer(i);
            for value in [call(desc, at, &[index]), call(vector, b"at:", &[index])] {
                if strings {
                    assert_eq!(bytes(value), *want);
                } else {
                    assert_eq!(value, key(want));
                }
            }
        }
        for args in [
            vec![],
            vec![negative],
            vec![integer(expected.len())],
            vec![nil],
            vec![zero, one],
        ] {
            assert_eq!(call(desc, at, &args), nil);
        }
    }
    assert!(forwards.iter().all(|f| hooks.count(f.3, 2) > 0));
    let error = unsafe {
        host.id.send(
            named(b"Execution"),
            key(b"failNative:detail:"),
            &[size(881).unwrap(), size(23).unwrap()],
        )
    }
    .unwrap_err();
    q.keep(error).unwrap();
    let denied = hooks.context(&[error, size(0).unwrap()]);
    let failing = hooks.context(&[hooks.method(b"deny", denied), gc, size(0).unwrap()]);
    let (vt, selector, _, _, forward) = forwards[0];
    call(
        vt,
        b"addMethod",
        &[selector, hooks.method(b"collectEvery", failing)],
    );
    assert_eq!(
        unsafe { host.id.send(artifact, key(b"inspect:"), &[path_obj]) }.unwrap_err(),
        error
    );
    assert_eq!(hooks.count(denied, 1), 1);
    assert_eq!(call(artifact, b"lastFailure", &[]), nil);
    call(vt, b"addMethod", &[selector, forward]);
    // A tiny observer lists allocation identities without retaining the observed object.
    let library = dir.join("presence.dylib");
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/allocation-presence.c")
            )
            .arg("-o")
            .arg(&library)
            .status()
            .unwrap()
            .success()
    );
    let module = host.load_native(&q, &library, &[]).unwrap();
    let monitor = call(named(b"Object"), b"clone", &[]);
    let mvt = call(host.id.root, b"ensurePrivateVtableFor:", &[monitor]);
    for name in [b"allocation:bytes:identity:".as_slice(), b"present:"] {
        call(
            mvt,
            b"addMethod",
            &[key(name), call(module, b"methodAt:", &[key(name)])],
        );
    }
    let present = |id| as_size(call(monitor, b"present:", &[id])).unwrap() == 1;
    let input = host.scope();
    let transient_path = input.string(path.as_os_str().as_encoded_bytes()).unwrap();
    let path_id = input
        .call(host.id.root, b"identityOf:", &[transient_path])
        .unwrap();
    let inspect = key(b"inspect:");
    drop(input);
    let transient = unsafe { host.id.send(artifact, inspect, &[transient_path]) }.unwrap();
    let pin = host.scope();
    pin.keep(transient).unwrap();
    let result_id = pin
        .call(host.id.root, b"identityOf:", &[transient])
        .unwrap();
    call(gc, b"collectPrecisely", &[]);
    assert!(present(result_id));
    assert!(!present(path_id));
    {
        let data = pin.call(transient, b"path", &[]).unwrap();
        assert_eq!(
            pin.string_bytes(data).unwrap(),
            path.as_os_str().as_encoded_bytes()
        );
    }
    drop(pin);
    call(gc, b"collectPrecisely", &[]);
    assert!(!present(result_id));
    for (vt, selector, original, _, _) in forwards {
        call(vt, b"addMethod", &[selector, original]);
    }
    let mut unusual = good.clone();
    let start = unusual
        .windows(b"candidate".len())
        .position(|v| v == b"candidate")
        .unwrap();
    unusual[start + 4] = 0;
    let digest = unusual
        .windows(b"source-digest".len())
        .position(|v| v == b"source-digest")
        .unwrap();
    unusual[digest + 3] = 0;
    let odd_path = dir.join("unusual.bin");
    std::fs::write(&odd_path, &unusual).unwrap();
    let odd = call(
        artifact,
        b"inspect:",
        &[text(odd_path.as_os_str().as_encoded_bytes())],
    );
    assert_eq!(call(odd, b"identity", &[]), key(b"cand"));
    assert_eq!(bytes(call(odd, b"sourceDigest", &[])), b"sou\0ce-digest");
    std::fs::write(&odd_path, fixture(abi, target, &[b"bad\0name"])).unwrap();
    assert_eq!(
        call(
            artifact,
            b"inspect:",
            &[text(odd_path.as_os_str().as_encoded_bytes())]
        ),
        nil
    );
    failure(b"nativeRequirementUnavailable");
    let clone = call(desc, b"clone", &[]);
    call(gc, b"collectPrecisely", &[]);
    assert_eq!(call(clone, b"identity", &[]), key(b"candidate"));
    let eval = |s: &[u8]| call(named(b"Codegen"), b"eval:", &[text(s)]);
    q.check_result(eval(
        b"[Reflect defineMethod: #artifactMigrationProbe on: Artifact as: [:self | 17]]",
    ))
    .unwrap();
    assert_eq!(number(eval(b"[Artifact artifactMigrationProbe]")), 17);
    let vt = named(b"ArtifactDescriptorVt");
    let identity = call(vt, b"lookup", &[key(b"identity")]);
    let kind = call(vt, b"lookup", &[key(b"kind")]);
    call(vt, b"addMethod", &[key(b"identity"), kind]);
    assert_eq!(call(desc, b"identity", &[]), key(b"module"));
    assert_eq!(
        host.load_native(
            &q,
            &host.resources.lib_dir.join("libbirnam_artifact.dylib"),
            &[host.namespace]
        )
        .unwrap(),
        artifact
    );
    assert_eq!(number(eval(b"[Artifact artifactMigrationProbe]")), 17);
    assert_eq!(call(desc, b"identity", &[]), key(b"module"));
    call(vt, b"addMethod", &[key(b"identity"), identity]);
    call(
        host.namespace,
        b"at:put:",
        &[key(b"ArtifactTestPath"), path_obj],
    );
    assert_eq!(
        eval(b"[[Artifact inspect: ArtifactTestPath] identity]"),
        key(b"candidate")
    );
    assert!(!marker.exists());
    std::fs::remove_dir_all(dir).unwrap();
}
