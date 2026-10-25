//! Real ELF objects through descriptor/metadata admission and TCC relocation.
use birnam_abi::{self as abi, Oop, id_protocol::Protocol};
pub(super) unsafe fn check(
    id: &Protocol,
    send: &impl Fn(Oop, &[u8], &[Oop]) -> Result<Oop, Oop>,
    sel: &impl Fn(&[u8]) -> Oop,
    objects: [Oop; 5],
) {
    unsafe {
        let [ns, roots, gc, string, integer] = objects;
        let tokens = std::cell::RefCell::new(Vec::new());
        let keep = |v| {
            tokens
                .borrow_mut()
                .push(send(roots, b"retain:", &[v]).unwrap());
            v
        };
        let call = |r, name: &[u8], args: &[Oop]| {
            keep(send(r, name, args).unwrap_or_else(|e| {
                panic!("artifact send {}: {e:?}", String::from_utf8_lossy(name))
            }))
        };
        let named = |n: &[u8]| call(ns, b"at:", &[sel(n)]);
        let text = |bytes: &[u8]| {
            let values: Vec<_> = bytes
                .iter()
                .map(|b| abi::size(*b as usize).unwrap())
                .collect();
            let values = call(id.root, b"sequence", &values);
            call(string, b"fromBytes:", &[values])
        };
        let bytes = |v| {
            let v = call(v, b"bytes", &[]);
            id.read_bytes(v, sel(b"sizeWord"), sel(b"atWord:")).unwrap()
        };
        let codegen = named(b"Codegen");
        let tcc = named(b"TCC");
        let artifact = named(b"Artifact");
        let globals = named(b"Globals");
        let nil = named(b"nil");
        let truth = named(b"true");
        let eval = |s: &str| call(codegen, b"eval:", &[text(s.as_bytes())]);
        let bits = |v| abi::as_size(call(integer, b"wordOf:", &[v])).unwrap();
        let units = || abi::as_size(call(tcc, b"liveNativeUnits", &[])).unwrap();
        let collect = || {
            send(gc, b"collectPrecisely", &[]).unwrap();
            send(tcc, b"checkpoint", &[]).unwrap();
        };
        let actor = named(b"CompilerProbe");
        let av = call(id.root, b"behaviorOf:", &[actor]);
        let count = |n: &[u8]| abi::as_size(call(actor, b"resumeCount:", &[sel(n)])).unwrap();
        let root = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
        let temp = root
            .join(".cargo-target")
            .join(format!("artifact-admission-{}", std::process::id()));
        std::fs::create_dir_all(&temp).unwrap();
        let path = temp.join("libbirnam_artifactProbe.dylib");
        let path_value = text(path.as_os_str().as_encoded_bytes());
        let filename = temp.join("original.birnam");
        let filename_value = text(filename.as_os_str().as_encoded_bytes());
        let cold_path = temp.join("cold.dylib");
        let cold_value = text(cold_path.as_os_str().as_encoded_bytes());
        assert_eq!(
            call(codegen, b"compile:output:", &[text(b"41"), cold_value]),
            cold_value
        );
        let error_factory = named(b"Error");
        let base = named(b"ObjectVt");
        let is_error = call(base, b"lookup", &[sel(b"isError")]);
        let rethrow = call(av, b"lookup", &[sel(b"rethrow:")]);
        call(ns, b"at:put:", &[sel(b"Error"), std::ptr::null_mut()]);
        call(base, b"addMethod", &[sel(b"isError"), rethrow]);
        let cold = send(codegen, b"load:", &[cold_value]);
        call(base, b"addMethod", &[sel(b"isError"), is_error]);
        call(ns, b"at:put:", &[sel(b"Error"), error_factory]);
        assert_eq!(bits(keep(cold.unwrap())), 41);
        println!("ARTIFACT_COLD_ERROR_OWNER_OK");
        let source=b"[Module named: #artifactProbe requires: #[]]; [CompilerProbe resumeMark: #before]; [Reflect defineMethod: #artifactAnswer on: CompilerProbe as: [:self | 61]]; [Object clone]";
        std::fs::write(&filename, source).unwrap();
        call(actor, b"resumeReset", &[]);
        collect();
        let baseline = units();
        assert_eq!(
            call(
                codegen,
                b"compile:output:from:",
                &[text(source), path_value, filename_value]
            ),
            path_value
        );
        assert_eq!(count(b"before"), 0);
        assert_eq!(units(), baseline);
        let object = std::fs::read(&path).unwrap();
        assert!(object.starts_with(b"\x7fELF"));
        let descriptor = call(artifact, b"inspect:", &[path_value]);
        assert_eq!(call(descriptor, b"identity", &[]), sel(b"artifactProbe"));
        assert_eq!(
            call(codegen, b"moduleSourceFor:", &[sel(b"artifactProbe")]),
            nil
        );
        std::fs::remove_file(&filename).unwrap();
        let lang = named(b"Lang");
        let lv = call(id.root, b"behaviorOf:", &[lang]);
        let parser = call(lv, b"lookup", &[sel(b"parse:")]);
        let reject = call(av, b"lookup", &[sel(b"rethrow:")]);
        call(lv, b"addMethod", &[sel(b"parse:"), reject]);
        let loaded = call(codegen, b"load:", &[path_value]);
        assert_ne!(loaded, nil);
        assert_ne!(call(loaded, b"isError", &[]), truth);
        assert_eq!(count(b"before"), 1);
        assert_eq!(bits(call(actor, b"artifactAnswer", &[])), 61);
        assert_eq!(
            bytes(call(codegen, b"moduleSourceFor:", &[sel(b"artifactProbe")])),
            source
        );
        collect();
        assert_eq!(bits(call(actor, b"artifactAnswer", &[])), 61);
        call(lv, b"addMethod", &[sel(b"parse:"), parser]);
        println!("ARTIFACT_PARSER_FREE_ACTIVATION_OK");

        let bad_path = temp.join("tampered.dylib");
        let bad_value = text(bad_path.as_os_str().as_encoded_bytes());
        let inert = birnam_artifact_codec::inspect(&object).unwrap();
        let kind_offset = inert.kind.as_ptr() as usize - object.as_ptr() as usize;
        let digest_offset = inert.source_digest.as_ptr() as usize - object.as_ptr() as usize;
        let identity_offset = inert.identity.as_ptr() as usize - object.as_ptr() as usize;
        let abi_offset = inert.artifact_abi.as_ptr() as usize - object.as_ptr() as usize;
        let capability_offset = object
            .windows(b"id-object-v3".len())
            .position(|w| w == b"id-object-v3")
            .unwrap();
        let metadata_offset = object
            .windows(b"birnam.source-metadata.v1\0".len())
            .position(|w| w == b"birnam.source-metadata.v1\0")
            .unwrap();
        for (label, offset, replacement) in [
            ("kind", kind_offset, b'X'),
            ("identity", identity_offset, b'X'),
            ("abi", abi_offset, b'X'),
            (
                "digest",
                digest_offset,
                if object[digest_offset] == b'0' {
                    b'1'
                } else {
                    b'0'
                },
            ),
            ("capability", capability_offset, b'X'),
            ("metadata", metadata_offset, b'X'),
        ] {
            let mut altered = object.clone();
            altered[offset] = replacement;
            std::fs::write(&bad_path, &altered).unwrap();
            call(actor, b"resumeReset", &[]);
            collect();
            let mapped = units();
            assert_eq!(call(codegen, b"load:", &[bad_value]), nil, "{label}");
            assert_eq!(count(b"before"), 0, "rejected body must not run: {label}");
            assert_ne!(call(codegen, b"lastError", &[]), nil);
            collect();
            assert_eq!(units(), mapped, "rejected mapping released: {label}");
        }
        let needs = temp.join("needs.dylib");
        let needs_value = text(needs.as_os_str().as_encoded_bytes());
        assert_eq!(call(codegen,b"compile:output:",&[text(b"[Module named: #artifactNeeds requires: #[#absentArtifactDependency]]; [CompilerProbe resumeMark: #before]; 7"),needs_value]),needs_value);
        call(actor, b"resumeReset", &[]);
        collect();
        let mapped = units();
        assert_eq!(call(codegen, b"load:", &[needs_value]), nil);
        assert_eq!(count(b"before"), 0);
        assert_eq!(units(), mapped);
        let conflict=text(b"[Module named: #artifactProbe requires: #[]]; [CompilerProbe resumeMark: #before]; 99");
        assert_eq!(
            call(codegen, b"compile:output:", &[conflict, bad_value]),
            bad_value
        );
        assert_eq!(call(codegen, b"load:", &[bad_value]), nil);
        assert_eq!(count(b"before"), 0);
        assert_eq!(bits(call(actor, b"artifactAnswer", &[])), 61);
        eval(
            "[Module named: #absentArtifactDependency requires: #[]]; [Module markLoaded: #absentArtifactDependency]",
        );
        assert_eq!(bits(call(codegen, b"load:", &[needs_value])), 7);
        assert_eq!(count(b"before"), 1);
        println!("ARTIFACT_REJECTION_BEFORE_EFFECTS_OK");

        // The reader can replace the on-disk file and collect. Inspection and relocation
        // must consume the returned frozen bytes, without rereading the changed path.
        let system = named(b"System");
        let sv = call(id.root, b"behaviorOf:", &[system]);
        let reader = call(sv, b"lookup", &[sel(b"readFile:")]);
        call(
            globals,
            b"at:put:",
            &[sel(b"FrozenArtifact"), text(&object)],
        );
        call(
            globals,
            b"at:put:",
            &[sel(b"ChangedArtifact"), text(b"changed")],
        );
        eval(
            "[Reflect defineMethod: #readFile: on: System as: [:self :path | [CompilerProbe resumeMark: #entered]; [System writeFile: path contents: ChangedArtifact]; [GC collectPrecisely]; FrozenArtifact]]",
        );
        call(actor, b"resumeReset", &[]);
        assert_ne!(call(codegen, b"load:", &[path_value]), nil);
        assert_eq!(count(b"entered"), 1);
        assert_eq!(count(b"before"), 1);
        assert_eq!(std::fs::read(&path).unwrap(), b"changed");
        call(sv, b"addMethod", &[sel(b"readFile:"), reader]);
        call(globals, b"at:put:", &[sel(b"FrozenArtifact"), nil]);
        call(globals, b"at:put:", &[sel(b"ChangedArtifact"), nil]);
        println!("ARTIFACT_FROZEN_SNAPSHOT_OK");

        let task_path = temp.join("task.dylib");
        let task_value = text(task_path.as_os_str().as_encoded_bytes());
        assert_eq!(call(codegen,b"compile:output:",&[text(b"[Module named: #artifactTask requires: #[]]; [TaskControl start: [:task | [task wait]; 77]]"),task_value]),task_value);
        let task = call(codegen, b"load:", &[task_value]);
        let token = call(task, b"waitToken", &[]);
        assert_ne!(token, nil);
        std::fs::remove_file(&task_path).unwrap();
        collect();
        assert_eq!(call(task, b"resolve:with:", &[token, nil]), truth);
        let outcome = call(task, b"pollOutcome", &[]);
        let expected = eval("#[#success 77]");
        assert_eq!(call(outcome, b"=", &[expected]), truth);
        println!("ARTIFACT_TASK_CODE_LIFETIME_OK");
        // The initializer remains resumable; explicit body selection controls
        // whether invocation crosses the retained synchronous native boundary.
        let modes_path = temp.join("modes.dylib");
        let modes_value = text(modes_path.as_os_str().as_encoded_bytes());
        for synchronous in [false, true] {
            let source = text(b"[| [ControlMachine controlEligible]]");
            assert_eq!(
                call(
                    codegen,
                    b"compile:output:from:synchronousBodies:",
                    &[
                        source,
                        modes_value,
                        filename_value,
                        if synchronous { truth } else { named(b"false") }
                    ]
                ),
                modes_value
            );
            let body = call(codegen, b"load:", &[modes_value]);
            let machine = eval("[Machine new]");
            call(globals, b"at:put:", &[sel(b"ControlMachine"), machine]);
            assert_eq!(
                call(machine, b"runBlock:", &[body]),
                if synchronous { named(b"false") } else { truth }
            );
        }
        call(globals, b"at:put:", &[sel(b"ControlMachine"), nil]);
        println!("ARTIFACT_SYNCHRONOUS_BODIES_OK");

        let previous = call(globals, b"at:", &[sel(b"Alien")]);
        eval(
            "let Alien := [Object clone]; [Reflect defineMethod: #provideLibraryPath:named: on: Alien as: [:self :path :name | [Globals at: #LibraryPath put: path]; self]]; [Reflect defineMethod: #provideLibrary:named: on: Alien as: [:self :bytes :name | [Globals at: #LibraryBytes put: bytes]; self]]; [Reflect defineMethod: #dlopen: on: Alien as: [:self :name | [CompilerProbe resumeMark: #entered]; 99]]",
        );
        let library = temp.join("libprobe.dylib");
        std::fs::write(&library, &object).unwrap();
        let adjacent = temp.join("adjacent.dylib");
        let adjacent_value = text(adjacent.as_os_str().as_encoded_bytes());
        let root_value = text(temp.as_os_str().as_encoded_bytes());
        let adjacent_source = text(
            b"[Module named: #artifactAdjacent requires: #[]]; [Alien dlopen: \"libprobe.dylib\"]",
        );
        assert_eq!(
            call(
                codegen,
                b"compile:output:root:",
                &[adjacent_source, adjacent_value, root_value]
            ),
            adjacent_value
        );
        assert_eq!(bits(call(codegen, b"load:", &[adjacent_value])), 99);
        assert_eq!(
            bytes(call(globals, b"at:", &[sel(b"LibraryPath")])),
            library.as_os_str().as_encoded_bytes()
        );
        call(
            globals,
            b"at:put:",
            &[sel(b"LibraryPayload"), text(&object)],
        );
        call(
            globals,
            b"at:put:",
            &[sel(b"LibraryName"), text(b"libprobe.dylib")],
        );
        let libraries = eval("#[#[LibraryName LibraryPayload]]");
        let embedded_path = temp.join("embedded.dylib");
        let embedded_value = text(embedded_path.as_os_str().as_encoded_bytes());
        let embedded_source = text(
            b"[Module named: #artifactEmbedded requires: #[]]; [Alien dlopen: \"libprobe.dylib\"]",
        );
        assert_eq!(
            call(
                codegen,
                b"compile:output:root:embeddedLibraries:",
                &[embedded_source, embedded_value, root_value, libraries]
            ),
            embedded_value
        );
        std::fs::remove_file(&library).unwrap();
        let descriptor = call(artifact, b"inspect:", &[embedded_value]);
        assert_ne!(descriptor, nil);
        assert_eq!(
            bits(call(call(descriptor, b"nativeRequires", &[]), b"size", &[])),
            0
        );
        assert_eq!(bits(call(codegen, b"load:", &[embedded_value])), 99);
        assert_eq!(
            bytes(call(globals, b"at:", &[sel(b"LibraryBytes")])),
            object
        );
        let invalid = eval("#[#[LibraryName LibraryPayload] #[LibraryName LibraryPayload]]");
        let before = std::fs::read(&embedded_path).unwrap();
        assert_eq!(
            call(
                codegen,
                b"compile:output:root:embeddedLibraries:",
                &[embedded_source, embedded_value, root_value, invalid]
            ),
            nil
        );
        assert_eq!(std::fs::read(&embedded_path).unwrap(), before);
        call(actor, b"resumeReset", &[]);
        eval(
            "[Reflect defineMethod: #provideLibrary:named: on: Alien as: [:self :bytes :name | nil]]",
        );
        assert_eq!(call(codegen, b"load:", &[embedded_value]), nil);
        assert_eq!(count(b"entered"), 0);
        call(globals, b"at:put:", &[sel(b"Alien"), previous]);
        for name in [
            b"LibraryName".as_slice(),
            b"LibraryPayload",
            b"LibraryPath",
            b"LibraryBytes",
        ] {
            call(globals, b"at:put:", &[sel(name), nil]);
        }
        println!("ARTIFACT_LIBRARY_PACKAGING_OK");
        call(av, b"removeMethod:", &[sel(b"artifactAnswer")]);
        std::fs::remove_dir_all(&temp).unwrap();
        for token in tokens.into_inner() {
            send(roots, b"release:", &[token]).unwrap();
        }
        collect();
    }
}
