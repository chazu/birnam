//! Retained source records, authored publication, and live continuation handoff.
use birnam_abi::{self as abi, Oop, id_protocol::Protocol};
pub(super) unsafe fn check(
    id: &Protocol,
    send: &impl Fn(Oop, &[u8], &[Oop]) -> Result<Oop, Oop>,
    sel: &impl Fn(&[u8]) -> Oop,
    objects: [Oop; 5],
) {
    {
        let [ns, roots, gc, string, integer] = objects;
        let byte_context = birnam_host_support::Context::from_namespace(*id, ns, Default::default(), sel(b"nameWithBytes")).unwrap();
        let retain = |v| send(roots, b"retain:", &[v]).unwrap();
        let release = |v| {
            send(roots, b"release:", &[v]).unwrap();
        };
        let text = |bytes: &[u8]| {
            let values: Vec<_> = bytes
                .iter()
                .map(|b| abi::size(*b as usize).unwrap())
                .collect();
            let values = send(id.root, b"sequence", &values).unwrap();
            send(string, b"fromBytes:", &[values]).unwrap()
        };
        let bytes = |v| {
            let vt = retain(v);
            let v = send(v, b"bytes", &[]).unwrap();
            let bt = retain(v);
            let result = byte_context.scope().read_bytes(v).unwrap();
            release(bt);
            release(vt);
            result
        };
        let named = |name: &[u8]| send(ns, b"at:", &[sel(name)]).unwrap();
        let codegen = named(b"Codegen");
        let globals = named(b"Globals");
        let nil = named(b"nil");
        let truth = named(b"true");
        let registry = send(globals, b"at:", &[sel(b"SourceRegistry")]).unwrap();
        let registry_token = retain(registry);
        let number = |n| send(integer, b"fromWord:", &[abi::size(n).unwrap()]).unwrap();
        let index = |v| abi::as_size(send(integer, b"wordOf:", &[v]).unwrap()).unwrap();
        let field = |v, i| {
            let t = retain(v);
            let result = send(v, b"at:", &[number(i)]).unwrap();
            release(t);
            result
        };
        let revision = || index(send(registry, b"revision", &[]).unwrap());
        let eval = |source: &str| {
            let s = text(source.as_bytes());
            let t = retain(s);
            let v = send(codegen, b"eval:", &[s]).unwrap();
            let rt = retain(v);
            release(t);
            assert_ne!(send(v, b"isError", &[]), Ok(truth), "{source}");
            (v, rt)
        };
        let collect = || {
            send(gc, b"collectPrecisely", &[]).unwrap();
        };
        let original=b"[Module named: #sourceProbe requires: #[#dependency]]; [Reflect defineMethod: #answer on: SourceTarget as: [:self | 17]]";
        let changed=b"[Module named: #sourceProbe requires: #[#dependency]]; [Reflect defineMethod: #answer on: SourceTarget as: [:self | 18]]";
        send(gc, b"budget:", &[abi::size(64 << 10).unwrap()]).unwrap();
        send(gc, b"automaticCollection:", &[abi::size(1).unwrap()]).unwrap();
        let source = text(original);
        let source_token = retain(source);
        assert_eq!(revision(), 0);
        assert_eq!(send(codegen, b"compiledSourceFor:", &[source]), Ok(nil));
        let record = send(codegen, b"analyzeSourceMetadata:", &[source]).unwrap();
        let record_token = retain(record);
        assert_ne!(record, nil);
        assert_eq!(send(codegen, b"sourceTextOf:", &[record]), Ok(source));
        let snapshot = send(codegen, b"sourceRevisionOf:", &[record]).unwrap();
        let snapshot_token = retain(snapshot);
        assert_eq!(field(snapshot, 1), sel(b"sourceProbe"));
        assert_eq!(field(snapshot, 3), source);
        assert_eq!(field(snapshot, 7), record);
        let encoded = send(codegen, b"compiledSourceMetadataFor:", &[record]).unwrap();
        let encoded_bytes = bytes(encoded);
        assert_eq!(revision(), 0, "inspection does not publish");
        assert_eq!(
            send(codegen, b"compiledSourceFor:", &[source]),
            Ok(nil),
            "inspection must not attach metadata to the String"
        );
        let lang = named(b"Lang");
        let lv = send(id.root, b"behaviorOf:", &[lang]).unwrap();
        let parser = send(lv, b"lookup", &[sel(b"parse:")]).unwrap();
        let pt = retain(parser);
        let actor = named(b"CompilerProbe");
        let av = send(id.root, b"behaviorOf:", &[actor]).unwrap();
        let reject = send(av, b"lookup", &[sel(b"rethrow:")]).unwrap();
        let rt = retain(reject);
        send(lv, b"addMethod", &[sel(b"parse:"), reject]).unwrap();
        assert_eq!(
            send(codegen, b"analyzeSourceMetadata:", &[record]),
            Ok(record)
        );
        assert_eq!(
            bytes(send(codegen, b"compiledSourceMetadataFor:", &[record]).unwrap()),
            encoded_bytes
        );
        assert_eq!(
            send(
                codegen,
                b"recordCompiledSource:named:",
                &[record, sel(b"sourceProbe")]
            ),
            Ok(source)
        );
        assert_eq!(revision(), 1);
        collect();
        let inventory = send(codegen, b"recordedMethodSources", &[]).unwrap();
        assert_eq!(index(send(inventory, b"size", &[]).unwrap()), 1);
        assert!(bytes(field(inventory, 0)).windows(2).any(|w| w == b"17"));
        assert_eq!(
            send(codegen, b"moduleSourceFor:", &[sel(b"sourceProbe")]),
            Ok(source)
        );
        assert_eq!(send(codegen, b"compiledSourceFor:", &[source]), Ok(record));
        assert!(
            send(codegen, b"analyzeSourceMetadata:", &[source]).is_err(),
            "raw source still uses the parser"
        );
        send(lv, b"addMethod", &[sel(b"parse:"), parser]).unwrap();
        release(pt);
        release(rt);
        let duplicate = text(original);
        let dt = retain(duplicate);
        assert_eq!(
            send(
                codegen,
                b"recordModuleSource:named:",
                &[duplicate, sel(b"sourceProbe")]
            ),
            Ok(duplicate)
        );
        assert_eq!(revision(), 1);
        assert_eq!(
            send(codegen, b"compiledSourceFor:", &[duplicate]),
            Ok(nil),
            "equal text is not the registered source identity"
        );
        release(dt);
        let replacement = text(changed);
        let replacement_token = retain(replacement);
        send(codegen, b"clearCompilationError", &[]).unwrap();
        assert_eq!(
            send(
                codegen,
                b"recordModuleSource:named:",
                &[replacement, sel(b"sourceProbe")]
            ),
            Ok(nil)
        );
        assert_eq!(revision(), 1);
        assert_ne!(send(codegen, b"lastError", &[]).unwrap(), nil);
        let next = send(codegen, b"analyzeSourceMetadata:", &[replacement]).unwrap();
        let nt = retain(next);
        assert_eq!(
            send(
                codegen,
                b"replaceCompiledSource:named:",
                &[next, sel(b"sourceProbe")]
            ),
            Ok(replacement)
        );
        assert_eq!(revision(), 2);
        release(source_token);
        release(record_token);
        collect();
        assert_eq!(
            bytes(send(codegen, b"sourceTextOf:", &[field(snapshot, 7)]).unwrap()),
            original
        );
        let metadata = send(codegen, b"retainedSourceMetadataOf:", &[field(snapshot, 7)]).unwrap();
        let forged = send(
            codegen,
            b"compiledSource:metadata:",
            &[replacement, metadata],
        )
        .unwrap();
        let ft = retain(forged);
        assert!(send(codegen, b"moduleMetadataOf:", &[forged]).is_err());
        let rejected = send(
            codegen,
            b"recordCompiledSource:named:",
            &[forged, sel(b"sourceProbe")],
        )
        .unwrap();
        assert_eq!(send(rejected, b"isError", &[]), Ok(truth));
        assert_eq!(revision(), 2);
        release(ft);
        send(codegen, b"clearCompilationError", &[]).unwrap();
        assert_eq!(
            send(
                codegen,
                b"recordCompiledSource:named:",
                &[next, sel(b"sourceWrong")]
            ),
            Ok(nil)
        );
        assert_eq!(revision(), 2);
        assert_ne!(send(codegen, b"lastError", &[]).unwrap(), nil);
        release(nt);
        release(replacement_token);
        release(snapshot_token);
        collect();
        let (registered, checked_token) = eval("[Module isRegistered: #sourceProbe]");
        assert_eq!(registered, named(b"false"));
        release(checked_token);
        println!("SOURCE_REGISTRY_METADATA_OWNERSHIP_OK");

        // A replacement registry can suspend; metadata preparation must not leave a
        // native publication frame waiting. Rebinding affects only future calls.
        let (machine, mt) = eval("[Machine new]");
        send(globals, b"at:put:", &[sel(b"ControlMachine"), machine]).unwrap();
        let (_, t) = eval(
            "[[| let replacement := [Object clone]; [Reflect defineMethod: #recordCompiledSource:named: on: replacement as: [:self :record :name | [ControlMachine park]; [Codegen sourceTextOf: record]]]; [Globals at: #SourceRegistry put: replacement]] value]",
        );
        release(t);
        let transferring = text(b"[Module named: #sourceTransfer requires: #[]]");
        send(
            globals,
            b"at:put:",
            &[sel(b"RegistrationSource"), transferring],
        )
        .unwrap();
        let (body, bt) =
            eval("[| [Codegen recordModuleSource: RegistrationSource named: #sourceTransfer]]");
        assert_eq!(send(machine, b"runBlock:", &[body]), Ok(machine));
        assert_eq!(send(machine, b"executionState", &[]), Ok(sel(b"suspended")));
        send(globals, b"at:put:", &[sel(b"SourceRegistry"), registry]).unwrap();
        send(globals, b"at:put:", &[sel(b"RegistrationSource"), nil]).unwrap();
        release(bt);
        collect();
        let result = send(machine, b"resumeWith:", &[nil]).unwrap();
        let result_token = retain(result);
        assert_eq!(
            bytes(result),
            b"[Module named: #sourceTransfer requires: #[]]"
        );
        assert_eq!(revision(), 2);
        assert_eq!(
            send(
                codegen,
                b"recordModuleSource:named:",
                &[result, sel(b"sourceTransfer")]
            ),
            Ok(result)
        );
        assert_eq!(revision(), 3);
        send(globals, b"at:put:", &[sel(b"ControlMachine"), nil]).unwrap();
        release(result_token);
        release(mt);
        release(registry_token);
        collect();
        send(gc, b"automaticCollection:", &[abi::size(0).unwrap()]).unwrap();
        send(gc, b"budget:", &[abi::size(16 << 20).unwrap()]).unwrap();
        println!("SOURCE_REGISTRY_LIVE_SUSPENSION_OK");
    }
}
