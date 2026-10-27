//! Real files through the authored loader and current compiler/file owners.
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
        let text = |bytes: &[u8]| {
            let v: Vec<_> = bytes
                .iter()
                .map(|b| abi::size(*b as usize).unwrap())
                .collect();
            let v = send(id.root, b"sequence", &v).unwrap();
            keep(send(string, b"fromBytes:", &[v]).unwrap())
        };
        let call = |r, name: &[u8], a: &[Oop]| {
            keep(send(r, name, a).unwrap_or_else(|e| {
                panic!(
                    "source loader send {} ({} args): {e:?}",
                    String::from_utf8_lossy(name),
                    a.len()
                )
            }))
        };
        let named = |name: &[u8]| call(ns, b"at:", &[sel(name)]);
        let bytes = |v| {
            let v = call(v, b"bytes", &[]);
            id.read_bytes(v, sel(b"sizeWord"), sel(b"atWord:")).unwrap()
        };
        let codegen = named(b"Codegen");
        let system = named(b"System");
        let globals = named(b"Globals");
        let nil = named(b"nil");
        let truth = named(b"true");
        let eval = |s: &str| call(codegen, b"eval:", &[text(s.as_bytes())]);
        let bits = |v| {
            let word = call(integer, b"wordOf:", &[v]);
            abi::as_size(word).unwrap_or_else(|| abi::as_size(call(word, b"low32", &[])).unwrap())
        };
        let collect = || {
            send(gc, b"collectPrecisely", &[]).unwrap();
        };
        let root = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
        let temp = root
            .join(".cargo-target")
            .join(format!("source-loading-{}", std::process::id()));
        std::fs::create_dir_all(&temp).unwrap();
        let path = temp.join("example.birnam");
        let path_value = text(path.as_os_str().as_encoded_bytes());
        let load = || call(system, b"load:", &[path_value]);
        let source =
            b"[Module named: #fileLoadExample requires: #[]]; let fileAnswer := 42; [Object clone]";
        std::fs::write(&path, source).unwrap();
        let result = load();
        assert_ne!(result, nil);
        assert_ne!(call(result, b"isError", &[]), truth);
        assert_eq!(bits(eval("fileAnswer")), 42);
        assert_eq!(eval("[Module isLoaded: #fileLoadExample]"), named(b"false"));
        let registered = call(codegen, b"moduleSourceFor:", &[sel(b"fileLoadExample")]);
        assert_eq!(bytes(registered), source);
        let manager = eval("Module");
        let old_manager = named(b"Module");
        call(ns, b"at:put:", &[sel(b"Module"), manager]);
        load();
        assert_eq!(eval("[Module isLoaded: #fileLoadExample]"), truth);
        std::fs::write(
            &path,
            b"[Module named: #fileLoadExample requires: #[]]; let fileAnswer := 43; 43",
        )
        .unwrap();
        assert_eq!(bits(load()), 43);
        assert_eq!(
            call(codegen, b"moduleSourceFor:", &[sel(b"fileLoadExample")]),
            registered
        );
        assert_eq!(call(codegen, b"lastError", &[]), nil);
        std::fs::write(&path, b"\n[").unwrap();
        let result = load();
        assert_eq!(call(result, b"kind", &[]), sel(b"syntaxError"));
        let diagnostic = bytes(call(codegen, b"lastError", &[]));
        assert!(diagnostic.windows(12).any(|w| w == b"syntax error"));
        assert!(diagnostic.contains(&b'^'));
        std::fs::write(&path, b"nil").unwrap();
        assert_eq!(load(), nil);
        assert_eq!(call(codegen, b"lastError", &[]), nil);
        std::fs::write(&path, b"[Error kind: #testFailure message: \"kept\"]").unwrap();
        let result = load();
        assert_eq!(call(result, b"kind", &[]), sel(b"testFailure"));
        assert_eq!(bytes(call(result, b"message", &[])), b"kept");
        assert_eq!(call(codegen, b"lastError", &[]), nil);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(call(load(), b"kind", &[]), sel(b"ioError"));
        assert_eq!(call(system, b"load:", &[nil]), nil);
        assert_eq!(call(system, b"load:", &[]), nil);
        // Constructor rejection is a diagnostic, distinct from an evaluated Error.
        let tcc = named(b"TCC");
        let tv = call(id.root, b"behaviorOf:", &[tcc]);
        let original = call(tv, b"lookup", &[sel(b"compile:")]);
        let actor = named(b"CompilerProbe");
        let av = call(id.root, b"behaviorOf:", &[actor]);
        let constant = call(av, b"lookup", &[sel(b"constantValue")]);
        let body_error = eval("[Error kind: #testFailure]");
        for rejected in [nil, body_error] {
            let replacement = call(constant, b"withContext:", &[rejected]);
            call(tv, b"addMethod", &[sel(b"compile:"), replacement]);
            assert_eq!(
                call(codegen, b"evalPersisting:from:", &[text(b"42"), path_value]),
                nil
            );
            assert_eq!(
                bytes(call(codegen, b"lastError", &[])),
                b"RustCodegen generated C failed to compile"
            );
        }
        let rethrow = call(av, b"lookup", &[sel(b"rethrow:")]);
        call(tv, b"addMethod", &[sel(b"compile:"), rethrow]);
        assert!(send(codegen, b"evalPersisting:from:", &[text(b"42"), path_value]).is_err());
        assert_eq!(call(codegen, b"lastError", &[]), nil);
        call(tv, b"addMethod", &[sel(b"compile:"), original]);
        assert_eq!(
            call(codegen, b"evalPersisting:from:", &[text(b"42"), nil]),
            nil
        );
        assert_eq!(
            bytes(call(codegen, b"lastError", &[])),
            b"RustCodegen source path was not a String"
        );
        println!("SOURCE_FILE_LOADING_DIAGNOSTICS_OK");

        // Observe the public descriptor arguments, without reading frame payloads.
        let target = std::env::var_os("BIRNAM_OOP_TARGET")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| root.join(".cargo-target/oop-slice"));
        let name: Vec<_> = target
            .join("fixtures/continuation.dylib")
            .as_os_str()
            .as_encoded_bytes()
            .iter()
            .map(|b| abi::size(*b as usize).unwrap())
            .collect();
        let image_path = call(id.root, b"nameWithBytes", &name);
        let empty = call(id.root, b"sequence", &[]);
        let image = keep(id.send(image_path, id.load, &[empty]).unwrap());
        let execution = named(b"Execution");
        let ev = call(id.root, b"behaviorOf:", &[execution]);
        let register = call(
            ev,
            b"lookup",
            &[sel(b"fromStep:descriptor:selection:owner:callPlans:")],
        );
        let metadata = call(id.root, b"sequence", &[nil, nil]);
        let context = call(id.root, b"sequence", &[register, metadata]);
        let capture = call(image, b"methodAt:", &[sel(b"captureRegistration")]);
        let capture = call(capture, b"withContext:", &[context]);
        call(
            ev,
            b"addMethod",
            &[sel(b"fromStep:descriptor:selection:owner:callPlans:"), capture],
        );
        call(
            codegen,
            b"evalPersisting:from:",
            &[text(b"[| 41]"), path_value],
        );
        call(
            ev,
            b"addMethod",
            &[sel(b"fromStep:descriptor:selection:owner:callPlans:"), register],
        );
        let descriptor = bytes(call(metadata, b"at:", &[abi::size(0).unwrap()]));
        let filename = path.as_os_str().as_encoded_bytes();
        assert!(
            descriptor.windows(filename.len()).any(|w| w == filename),
            "nested frame descriptor must retain the supplied filename"
        );

        let backend = named(b"NativeBackend");
        let bv = call(id.root, b"behaviorOf:", &[backend]);
        let emit_selector = sel(b"emit:source:filename:persistent:synchronous:");
        let emitter = call(bv, b"lookup", &[emit_selector]);
        let captured = call(id.root, b"sequence", &[nil]);
        let capture_context = call(id.root, b"sequence", &[emitter, captured]);
        let hook = call(image, b"methodAt:", &[sel(b"captureFactory")]);
        let hook = call(hook, b"withContext:", &[capture_context]);
        call(bv, b"addMethod", &[emit_selector, hook]);
        let source = text(b"let BackendLocal := 73; BackendLocal");
        assert_eq!(
            bits(call(
                codegen,
                b"evalPersisting:from:",
                &[source, path_value]
            )),
            73
        );
        call(bv, b"addMethod", &[emit_selector, emitter]);
        let program = call(captured, b"at:", &[abi::size(0).unwrap()]);
        let falsity = named(b"false");
        let blocks = named(b"Block");
        for (persistent, synchronous) in [(false, false), (false, true), (true, false)] {
            call(globals, b"at:put:", &[sel(b"BackendLocal"), nil]);
            let native = call(
                backend,
                b"emit:source:filename:persistent:synchronous:",
                &[
                    program,
                    source,
                    path_value,
                    if persistent { truth } else { falsity },
                    if synchronous { truth } else { falsity },
                ],
            );
            let unit = call(tcc, b"compile:", &[native]);
            let method = call(unit, b"initialize", &[]);
            let body = call(blocks, b"fromMethod:captureOperands:", &[method]);
            assert_eq!(bits(call(body, b"value", &[])), 73);
            let published = call(globals, b"at:", &[sel(b"BackendLocal")]);
            if persistent {
                assert_eq!(bits(published), 73);
            } else {
                assert_eq!(published, nil);
            }
        }
        let rejected = call(
            backend,
            b"emit:source:filename:persistent:synchronous:",
            &[program, source, path_value, truth, truth],
        );
        assert_eq!(call(rejected, b"isError", &[]), truth);
        call(globals, b"at:put:", &[sel(b"BackendLocal"), nil]);
        println!("SOURCE_BACKEND_MODES_OK");

        // Select Codegen after reading; preserve that choice if String construction
        // then rebinds it. Both callbacks collect before answering.
        std::fs::write(&path, b"42").unwrap();
        let snapshot = call(system, b"readFileSnapshot:", &[path_value]);
        call(globals, b"at:put:", &[sel(b"FileSnapshot"), snapshot]);
        call(globals, b"at:put:", &[sel(b"OriginalCompiler"), codegen]);
        call(globals, b"at:put:", &[sel(b"LoaderSource"), text(b"42")]);
        let sv = call(id.root, b"behaviorOf:", &[system]);
        let reader = call(sv, b"lookup", &[sel(b"readFileSnapshot:")]);
        let string_vt = call(id.root, b"behaviorOf:", &[string]);
        let constructor = call(string_vt, b"lookup", &[sel(b"fromBytes:")]);
        eval(
            "let LoaderCompiler := [Object clone]; [Reflect defineMethod: #sourceRevisionOf: on: LoaderCompiler as: [:self :source | nil]]; [Reflect defineMethod: #evalPersisting:from: on: LoaderCompiler as: [:self :source :path | 77]]",
        );
        eval(
            "[Reflect defineMethod: #readFileSnapshot: on: System as: [:self :path | [Namespace at: #Codegen put: LoaderCompiler]; [GC collectPrecisely]; FileSnapshot]]",
        );
        eval(
            "[Reflect defineMethod: #fromBytes: on: String as: [:self :bytes | [Namespace at: #Codegen put: OriginalCompiler]; [GC collectPrecisely]; LoaderSource]]",
        );
        let selected = load();
        call(string_vt, b"addMethod", &[sel(b"fromBytes:"), constructor]);
        call(sv, b"addMethod", &[sel(b"readFileSnapshot:"), reader]);
        assert_eq!(named(b"Codegen"), codegen);
        assert_eq!(bits(selected), 77);
        for name in [
            b"FileSnapshot".as_slice(),
            b"OriginalCompiler",
            b"LoaderSource",
            b"LoaderCompiler",
        ] {
            call(globals, b"at:put:", &[sel(name), nil]);
        }
        println!("SOURCE_FILE_LIVE_OWNERS_FILENAME_OK");

        let asset = temp.join("asset.bin");
        let contents = b"\0\xff\x80embedded";
        std::fs::write(&asset, contents).unwrap();
        std::fs::write(&path, b"[| [System embedFile: \"asset.bin\"]]").unwrap();
        let embedded = load();
        std::fs::remove_file(&asset).unwrap();
        collect();
        assert_eq!(bytes(call(embedded, b"value", &[])), contents);
        eval(
            "let FileEmbedProbe := [Object clone]; [Reflect defineMethod: #embedFile: on: FileEmbedProbe as: [:self :path | 91]]",
        );
        std::fs::write(
            &path,
            b"[[:System | [System embedFile: \"missing.bin\"]] value: FileEmbedProbe]",
        )
        .unwrap();
        assert_eq!(bits(load()), 91);
        for invalid in [
            b"[System embedFile: \"../outside.bin\"]".as_slice(),
            b"[System embedFile: [\"asset\" , \".bin\"]]",
        ] {
            std::fs::write(&path, invalid).unwrap();
            assert_eq!(call(load(), b"kind", &[]), sel(b"syntaxError"));
            assert_ne!(call(codegen, b"lastError", &[]), nil);
        }
        println!("SOURCE_FILE_EMBEDDING_OK");
        let machine = eval("[Machine new]");
        call(globals, b"at:put:", &[sel(b"ControlMachine"), machine]);
        call(globals, b"at:put:", &[sel(b"FileLoadPath"), path_value]);
        std::fs::write(&path,b"[CompilerProbe resumeMark: #before]; [ControlMachine park]; [CompilerProbe resumeMark: #after]; 29").unwrap();
        call(actor, b"resumeReset", &[]);
        let body = eval("[| [System load: FileLoadPath]]");
        assert_eq!(call(machine, b"runBlock:", &[body]), machine);
        assert_eq!(call(machine, b"executionState", &[]), sel(b"suspended"));
        assert_eq!(
            abi::as_size(call(actor, b"resumeCount:", &[sel(b"before")])),
            Some(1)
        );
        std::fs::remove_file(&path).unwrap();
        call(globals, b"at:put:", &[sel(b"FileLoadPath"), nil]);
        collect();
        assert_eq!(bits(call(machine, b"resumeWith:", &[nil])), 29);
        assert_eq!(
            abi::as_size(call(actor, b"resumeCount:", &[sel(b"before")])),
            Some(1)
        );
        assert_eq!(
            abi::as_size(call(actor, b"resumeCount:", &[sel(b"after")])),
            Some(1)
        );
        call(ns, b"at:put:", &[sel(b"Module"), old_manager]);
        call(globals, b"at:put:", &[sel(b"ControlMachine"), nil]);
        std::fs::remove_dir_all(&temp).unwrap();
        for token in tokens.into_inner() {
            send(roots, b"release:", &[token]).unwrap();
        }
        collect();
        println!("SOURCE_FILE_SUSPENSION_GC_OK");
    }
}
