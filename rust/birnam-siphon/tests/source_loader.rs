//! Source loading and authored Module policy through Oop owner messages.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, size};
use birnam_host_support::{Context, Scope};
use std::{os::unix::ffi::OsStrExt, path::Path, ptr};
struct H<'a> {
    host: &'a Context,
    q: Scope<'a>,
    gc: Oop,
    hooks: oop::hooks::Hooks<'a>,
    fixture: Oop,
}
impl H<'_> {
    fn named(&self, n: &[u8]) -> Oop {
        self.host.named(n).unwrap()
    }
    fn key(&self, n: &[u8]) -> Oop {
        self.hooks.symbol(n)
    }
    fn string(&self, b: &[u8]) -> Oop {
        self.q.string(b).unwrap()
    }
    fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        self.q.call(r, n, a).unwrap()
    }
    fn error(&self, v: Oop, k: &[u8], m: &[u8]) {
        assert_eq!(self.call(v, b"kind", &[]), self.key(k));
        assert_eq!(self.bytes(self.call(v, b"message", &[])), m);
    }
    fn eval(&self, b: &[u8]) -> Oop {
        self.call(self.named(b"Codegen"), b"eval:", &[self.string(b)])
    }
    fn bytes(&self, v: Oop) -> Vec<u8> {
        self.q.read_bytes(self.call(v, b"bytes", &[])).unwrap()
    }
    fn number(&self, v: Oop) -> usize {
        self.q
            .word_bits(self.call(self.named(b"Integer"), b"wordOf:", &[v]))
            .unwrap()
    }
    fn at(&self, v: Oop, i: usize) -> Oop {
        let index = self.call(self.named(b"Integer"), b"fromWord:", &[size(i).unwrap()]);
        self.call(v, b"at:", &[index])
    }
    fn method(&self, n: &[u8], c: Oop) -> Oop {
        self.call(
            self.call(self.fixture, b"methodAt:", &[self.key(n)]),
            b"withContext:",
            &[c],
        )
    }
    fn binding(&self, vt: Oop, n: &[u8]) -> Oop {
        self.call(vt, b"lookup", &[self.key(n)])
    }
    fn set(&self, vt: Oop, n: &[u8], m: Oop) {
        self.call(vt, b"addMethod", &[self.key(n), m]);
    }
    fn vt(&self, v: Oop) -> Oop {
        self.call(self.host.id.root, b"behaviorOf:", &[v])
    }
}
#[test]
#[ignore = "requires production Oop modules"]
fn source_loading_uses_live_owners() {
    let host = oop::bootstrap();
    let q = host.scope();
    let path = std::env::temp_dir().join(format!(
        "birnam-source-loader-hooks-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/source-loader.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = host.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let h = H {
        host: &host,
        q,
        gc: host.named(b"GC").unwrap(),
        hooks: oop::hooks::Hooks::new(&host),
        fixture,
    };
    h.call(h.gc, b"precisePayloads:", &[size(1).unwrap()]);
    let codegen = h.named(b"Codegen");
    let system = h.named(b"System");
    let nil = h.named(b"nil");
    let temp = std::env::temp_dir().join(format!("birnam-source-loader-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let manager = temp.join("modules.birnam");
    std::fs::write(&manager, include_bytes!("../../../modules/modules.birnam")).unwrap();
    let result = h.call(
        system,
        b"load:",
        &[h.string(manager.as_os_str().as_bytes())],
    );
    assert_ne!(h.call(result, b"isError", &[]), h.named(b"true"));
    assert_eq!(h.eval(b"[Module isLoaded: #modules]"), h.named(b"true"));
    assert_eq!(h.eval(b"[Module isLoaded: #cell]"), h.named(b"true"));
    assert_eq!(h.eval(b"[Module kindOf: #cell]"), h.key(b"selfHosted"));
    let artifact_path = host.resources.lib_dir.join("libbirnam_modules.dylib");
    let descriptor = h.call(
        h.named(b"Artifact"),
        b"inspect:",
        &[h.string(artifact_path.as_os_str().as_bytes())],
    );
    assert_eq!(h.call(descriptor, b"identity", &[]), h.key(b"modules"));
    h.eval(b"[Module named: #a requires: #[]]; [Module named: #b requires: #[#a]]");
    let order = h.eval(b"[Module loadOrder: #[#b]]");
    assert_eq!(h.call(order, b"at:", &[h.eval(b"0")]), h.key(b"a"));
    assert_eq!(h.call(order, b"at:", &[h.eval(b"1")]), h.key(b"b"));
    h.call(h.gc, b"collectPrecisely", &[]);
    assert_eq!(h.eval(b"[Module isRegistered: #b]"), h.named(b"true"));

    let module = h.eval(b"Module");
    assert!(
        host.send(host.namespace, b"at:", &[h.key(b"Module")])
            .unwrap()
            .is_null()
    );
    assert_ne!(
        h.call(codegen, b"moduleSourceFor:", &[h.key(b"modules")]),
        nil
    );
    let gold = h.string(b"[Module named: #gold requires: #[#dep]]");
    let revision = h.call(codegen, b"sourceRevisionOf:", &[gold]);
    let metadata = h.call(codegen, b"moduleMetadataOf:", &[gold]);
    assert_eq!(h.number(h.call(metadata, b"size", &[])), 5);
    assert_eq!(h.at(metadata, 0), h.key(b"moduleMetadata"));
    assert_eq!(h.at(metadata, 1), h.key(b"gold"));
    assert_eq!(h.at(h.at(metadata, 2), 0), h.key(b"dep"));
    assert_eq!(h.bytes(h.at(metadata, 4)), h.bytes(h.at(revision, 6)));
    assert_eq!(h.call(codegen, b"moduleMetadataOf:", &[nil]), nil);
    let assets = h.call(codegen, b"embeddedFilesOf:", &[nil]);
    assert_eq!(h.number(h.call(assets, b"size", &[])), 0);
    assert_eq!(
        h.call(codegen, b"moduleMetadataOf:", &[h.string(b"42")]),
        nil
    );
    assert_eq!(h.number(h.call(revision, b"size", &[])), 8);
    assert_eq!(h.at(revision, 0), h.key(b"sourceRevision"));
    assert_eq!(h.at(revision, 1), h.key(b"gold"));
    assert_eq!(h.at(h.at(revision, 2), 0), h.key(b"dep"));
    assert_eq!(h.at(revision, 3), gold);
    assert_eq!(
        h.bytes(h.at(revision, 4)),
        b"c8c54eaf253655e27e4d4cee9b55fc09f044f3761c8c67ebcbbe4017768c1710"
    );
    assert_eq!(
        h.bytes(h.at(revision, 5)),
        b"14a1c4f166b7504907b76034d87e440d2c26d8498b8776f872fbeab3ada986ad"
    );
    assert_eq!(
        h.bytes(h.at(revision, 6)),
        b"92749bc6da04d6045dc5a0eb386d10ad952542a70ca6754c431c1d2b8bbeed9f"
    );

    for bad in [
            b"42".as_slice(),
            b"[Module named: #a requires: #[]]; [Module named: #b requires: #[]]",
            b"[Module named: #a requires: #[]]; [Reflect defineMethod: #foo on: Object as: [| 1]]",
            b"[Module named: #a requires: #[]]; [Reflect defineMethod: #foo on: Object as: [:s | 1]]; [Reflect removeMethod: #foo on: Object]",
        ] { assert_eq!(h.call(codegen, b"sourceRevisionOf:", &[h.string(bad)]), nil); }
    assert_eq!(
        h.call(
            codegen,
            b"recordModuleSource:named:",
            &[gold, h.key(b"wrong")]
        ),
        nil
    );
    assert_eq!(
        h.call(
            codegen,
            b"recordModuleSource:named:",
            &[gold, h.key(b"gold")]
        ),
        gold
    );
    let docs = h.call(codegen, b"recordedMethodSources", &[]);
    assert!(
        h.bytes(h.at(docs, 0))
            .starts_with(b"[Reflect defineMethod: #named:requires:")
    );
    // Descriptions include declarations inside initializer blocks, without
    // executing them or treating quoted examples as declarations.
    let nested = h.string(
        br#"[Module named: #nestedDocumentation requires: #[]];
[[| [Reflect defineMethod: #nestedDocumentationProbe on: Object as: [:self | 17]] ] value];
let quoted := "[Reflect defineMethod: #quotedDocumentationProbe on: Object as: [:self | 23]]";
let dynamic := [:selector | [Reflect defineMethod: selector on: Object as: [:self | 31]]];"#,
    );
    h.call(
        codegen,
        b"recordModuleSource:named:",
        &[nested, h.key(b"nestedDocumentation")],
    );
    let descriptions = h.call(codegen, b"recordedMethodSources", &[]);
    let count = h.number(h.call(descriptions, b"size", &[]));
    let rows = (0..count)
        .map(|i| h.bytes(h.at(descriptions, i as usize)))
        .collect::<Vec<_>>();
    assert_eq!(
        rows.iter()
            .filter(|s| s.starts_with(b"[Reflect defineMethod: #nestedDocumentationProbe "))
            .count(),
        1
    );
    assert!(
        !rows
            .iter()
            .any(|s| s.starts_with(b"[Reflect defineMethod: #quotedDocumentationProbe "))
    );
    assert!(
        !rows
            .iter()
            .any(|s| s.starts_with(b"[Reflect defineMethod: selector "))
    );
    assert_eq!(
        h.call(
            h.named(b"Object"),
            b"methodFor:",
            &[h.key(b"nestedDocumentationProbe")]
        ),
        nil
    );

    let equal = h.string(b"[Module named: #gold requires: #[#dep]]");
    assert_ne!(equal, gold);
    assert_eq!(
        h.call(
            codegen,
            b"recordModuleSource:named:",
            &[equal, h.key(b"gold")]
        ),
        equal
    );

    assert_eq!(
        h.call(codegen, b"moduleSourceFor:", &[h.key(b"gold")]),
        gold
    );

    let path = temp.join("example.birnam");
    let path_object = h.string(path.as_os_str().as_bytes());
    std::fs::write(
        &path,
        b"[Module named: #example requires: #[]]; let loadedAnswer := 42; [Object clone]",
    )
    .unwrap();
    let loaded = h.call(system, b"load:", &[path_object]);
    assert_ne!(loaded, nil);
    assert_eq!(h.number(h.eval(b"loadedAnswer")), 42);
    // The authored manager lives in Globals. The loader deliberately does
    // not bridge Namespace and Globals to mark this module loaded.
    assert_eq!(h.eval(b"[Module isLoaded: #example]"), h.named(b"false"));
    let registered = h.call(codegen, b"moduleSourceFor:", &[h.key(b"example")]);
    assert_eq!(h.bytes(registered), std::fs::read(&path).unwrap());
    h.call(host.namespace, b"at:put:", &[h.key(b"Module"), module]);
    h.call(system, b"load:", &[path_object]);
    assert_eq!(h.eval(b"[Module isLoaded: #example]"), h.named(b"true"));
    // A duplicate-source nil response still allows ordinary evaluation.
    std::fs::write(
        &path,
        b"[Module named: #example requires: #[]]; let loadedAnswer := 43; 43",
    )
    .unwrap();
    assert_eq!(h.number(h.call(system, b"load:", &[path_object])), 43);
    assert_eq!(
        h.call(codegen, b"moduleSourceFor:", &[h.key(b"example")]),
        registered
    );
    assert_eq!(h.call(codegen, b"lastError", &[]), nil);

    std::fs::write(&path, b"nil").unwrap();
    assert_eq!(h.call(system, b"load:", &[path_object]), nil);
    assert_eq!(h.call(codegen, b"lastError", &[]), nil);
    std::fs::write(&path, b"[").unwrap();
    h.error(
        h.call(system, b"load:", &[path_object]),
        b"syntaxError",
        b"source file failed to compile",
    );
    assert!(
        String::from_utf8_lossy(&h.bytes(h.call(codegen, b"lastError", &[])))
            .contains("syntax error")
    );
    std::fs::write(
        &path,
        b"[Module named: #failed requires: #[]]; [Error kind: #testFailure message: \"kept\"]",
    )
    .unwrap();
    h.error(
        h.call(system, b"load:", &[path_object]),
        b"testFailure",
        b"kept",
    );
    assert_ne!(
        h.call(codegen, b"moduleSourceFor:", &[h.key(b"failed")]),
        nil
    );
    assert_eq!(h.eval(b"[Module isLoaded: #failed]"), h.named(b"false"));
    std::fs::remove_file(&path).unwrap();
    h.error(
        h.call(system, b"load:", &[path_object]),
        b"ioError",
        b"could not read source file",
    );
    assert_eq!(h.call(system, b"load:", &[]), nil);
    assert_eq!(h.call(system, b"load:", &[nil]), nil);
    h.error(
        h.call(system, b"load:", &[h.string(b"\xff")]),
        b"ioError",
        b"could not read source file",
    );

    // Only SourceRegistry retains this text after the registration returns.
    let retained = {
        let transient = host.scope();
        let text = transient
            .string(b"[Module named: #retained requires: #[]]")
            .unwrap();
        assert_eq!(
            host.send(
                codegen,
                b"recordModuleSource:named:",
                &[text, h.key(b"retained")]
            )
            .unwrap(),
            text
        );
        text
    };
    h.call(h.gc, b"collectPrecisely", &[]);
    assert_eq!(
        h.call(codegen, b"moduleSourceFor:", &[h.key(b"retained")]),
        retained
    );
    exercise_callbacks(&h, &temp);
    assert_eq!(
        h.call(
            codegen,
            b"recordModuleSource:named:",
            &[gold, h.key(b"gold")]
        ),
        gold
    );
    let later = b"[Module named: #afterRegistration requires: #[]]; 99";
    std::fs::write(&path, later).unwrap();
    assert_eq!(h.number(h.call(system, b"load:", &[path_object])), 99);
    let registered = h.call(codegen, b"moduleSourceFor:", &[h.key(b"afterRegistration")]);
    assert_eq!(h.bytes(registered), later);
    std::fs::remove_dir_all(&temp).unwrap();
}
fn exercise_callbacks(h: &H<'_>, temp: &Path) {
    let codegen = h.named(b"Codegen");
    let system = h.named(b"System");
    let nil = h.named(b"nil");
    let lang_vt = h.vt(h.named(b"Lang"));
    let string_vt = h.vt(h.named(b"String"));
    let cg_vt = h.vt(codegen);
    let system_vt = h.vt(system);
    let mut wrappers = vec![];
    for (vt, name, alias) in [
        (
            lang_vt,
            b"parse:".as_slice(),
            b"testSourceParse:".as_slice(),
        ),
        (string_vt, b"fromBytes:", b"testSourceString:"),
        (cg_vt, b"sourceRevisionOf:", b"testSourceRevision:"),
        (
            cg_vt,
            b"recordModuleSource:named:",
            b"testSourceRecord:named:",
        ),
        (cg_vt, b"evalPersisting:from:", b"testSourceEval:from:"),
    ] {
        let original = h.binding(vt, name);
        h.set(vt, alias, original);
        let state = h
            .hooks
            .context(&[h.gc, h.key(alias), size(0).unwrap(), ptr::null_mut()]);
        h.set(vt, name, h.method(b"forward", state));
        wrappers.push((vt, name, alias, original, state));
    }
    let source = h.string(b"[Module named: #collected requires: #[]]; 67");
    assert_eq!(
        h.call(
            codegen,
            b"recordModuleSource:named:",
            &[source, h.key(b"collected")]
        ),
        source
    );
    let revision = h.call(codegen, b"sourceRevisionOf:", &[source]);
    assert_eq!(h.at(revision, 3), source);
    let path = temp.join("collecting.birnam");
    let file = b"[Module named: #loadedCollected requires: #[]]; 68";
    std::fs::write(&path, file).unwrap();
    let path_object = h.string(path.as_os_str().as_bytes());
    assert_eq!(h.number(h.call(system, b"load:", &[path_object])), 68);
    let old = h.call(codegen, b"moduleSourceFor:", &[h.key(b"gold")]);
    let equal = h.string(&h.bytes(old));
    let nested = h.string(b"[Module named: #reentrant requires: #[]]");
    let original_bytes = h.binding(string_vt, b"bytes");
    h.set(string_vt, b"testSourceBytes", original_bytes);
    let path_bytes = h.q.byte_values(path.as_os_str().as_bytes()).unwrap();
    let state = h.hooks.context(&[
        h.gc,
        h.key(b"testSourceBytes"),
        old,
        codegen,
        nested,
        h.key(b"reentrant"),
        size(0).unwrap(),
        path_object,
        path_bytes,
        h.key(b"testSourceEquality:"),
    ]);
    h.set(string_vt, b"bytes", h.method(b"bytes", state));
    let original_equal = h.binding(string_vt, b"=");
    h.set(string_vt, b"testSourceEquality:", original_equal);
    h.set(string_vt, b"=", h.method(b"equal", state));
    assert_eq!(
        h.call(
            codegen,
            b"recordModuleSource:named:",
            &[equal, h.key(b"gold")]
        ),
        equal
    );
    h.set(string_vt, b"=", original_equal);
    h.call(
        string_vt,
        b"removeMethod:",
        &[h.key(b"testSourceEquality:")],
    );
    assert_eq!(h.hooks.count(state, 6), 1);
    assert_eq!(
        h.call(codegen, b"moduleSourceFor:", &[h.key(b"reentrant")]),
        nested
    );
    h.hooks.put(state, 6, size(0).unwrap());
    h.hooks.put(state, 2, source);
    let original_factory = h.binding(string_vt, b"fromBytes:");
    h.set(string_vt, b"testSuppliedSource:", original_factory);
    let supply = h.hooks.context(&[
        h.key(b"testSuppliedSource:"),
        source,
        h.q.byte_values(file).unwrap(),
    ]);
    h.set(string_vt, b"fromBytes:", h.method(b"supply", supply));
    assert_eq!(h.number(h.call(system, b"load:", &[path_object])), 67);
    assert_eq!(h.hooks.count(state, 6), 1);
    assert!(
        h.q.read_bytes(path_bytes)
            .unwrap()
            .iter()
            .all(|&b| b == b'x')
    );
    h.set(string_vt, b"fromBytes:", original_factory);
    h.call(
        string_vt,
        b"removeMethod:",
        &[h.key(b"testSuppliedSource:")],
    );
    h.set(string_vt, b"bytes", original_bytes);
    h.call(string_vt, b"removeMethod:", &[h.key(b"testSourceBytes")]);
    let failure = h.hooks.fault(883);
    h.hooks.put(wrappers[0].4, 3, failure);
    assert_eq!(
        unsafe { h.host.id.send(system, h.key(b"load:"), &[path_object]) }.unwrap_err(),
        failure
    );
    h.hooks.put(wrappers[0].4, 3, ptr::null_mut());
    let rejected = h.eval(b"[Error kind: #registrationRefused message: \"stop\"]");
    let record = h.binding(cg_vt, b"recordModuleSource:named:");
    let before = h.hooks.count(wrappers[4].4, 2);
    h.set(
        cg_vt,
        b"recordModuleSource:named:",
        h.hooks.method(b"fixed", rejected),
    );
    assert_eq!(h.call(system, b"load:", &[path_object]), rejected);
    assert_eq!(h.hooks.count(wrappers[4].4, 2), before);
    h.set(cg_vt, b"recordModuleSource:named:", record);
    for (vt, name, alias, original, state) in wrappers {
        assert!(h.hooks.count(state, 2) > 0);
        h.set(vt, name, original);
        h.call(vt, b"removeMethod:", &[h.key(alias)]);
    }
    let tcc_vt = h.vt(h.named(b"TCC"));
    let compile = h.binding(tcc_vt, b"compile:");
    let input = h.string(b"42");
    for result in [nil, rejected] {
        h.set(tcc_vt, b"compile:", h.hooks.method(b"fixed", result));
        assert_eq!(
            h.call(codegen, b"evalPersisting:from:", &[input, path_object]),
            nil
        );
        assert_eq!(
            h.bytes(h.call(codegen, b"lastError", &[])),
            b"RustCodegen generated C failed to compile"
        );
    }
    h.set(tcc_vt, b"compile:", compile);
    assert_eq!(h.call(codegen, b"evalPersisting:from:", &[input, nil]), nil);
    assert_eq!(
        h.bytes(h.call(codegen, b"lastError", &[])),
        b"RustCodegen source path was not a String"
    );
    h.call(h.host.namespace, b"at:put:", &[h.key(b"Codegen"), nil]);
    h.error(
        h.call(system, b"load:", &[path_object]),
        b"noCodegen",
        b"Codegen module is not loaded",
    );
    h.error(
        h.call(
            system,
            b"load:",
            &[h.string(temp.join("absent.birnam").as_os_str().as_bytes())],
        ),
        b"ioError",
        b"could not read source file",
    );
    h.call(h.host.namespace, b"at:put:", &[h.key(b"Codegen"), codegen]);
    let original = h.binding(system_vt, b"load:");
    h.set(system_vt, b"load:", h.hooks.method(b"fixed", nil));
    let manager = h.eval(b"Module");
    let loader = h.string(
        h.host
            .resources
            .lib_dir
            .join("libbirnam_sourceLoader.dylib")
            .as_os_str()
            .as_bytes(),
    );
    let loaded = h.call(manager, b"load:path:", &[h.key(b"sourceLoader"), loader]);
    assert_eq!(h.call(loaded, b"isError", &[]), h.named(b"false"));
    assert_eq!(h.call(system, b"load:", &[path_object]), nil);
    h.set(system_vt, b"load:", original);
    h.call(codegen, b"evalPersisting:", &[h.string(b"nil")]);
}
