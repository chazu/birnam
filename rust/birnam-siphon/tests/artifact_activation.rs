//! Durable artifact activation through current Oop owners, without recompilation.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
#[test]
#[ignore = "requires production Oop modules with TCC metrics"]
fn durable_artifacts_use_owned_code_and_current_messages() {
    let host = oop::bootstrap();
    let q = host.scope();
    let hooks = oop::hooks::Hooks::new(&host);
    let named = |n: &[u8]| host.named(n).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let text = |b: &[u8]| q.string(b).unwrap();
    let bytes = |v| q.read_bytes(call(v, b"bytes", &[])).unwrap();
    let gc = named(b"GC");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let reflect = named(b"Reflect");
    assert_eq!(
        bytes(call(reflect, b"nameOf:", &[key(b"artifactName")])),
        b"artifactName"
    );
    let anonymous = call(named(b"Object"), b"clone", &[]);
    for value in [anonymous, named(b"Object"), text(b"not a Symbol")] {
        let e = call(reflect, b"nameOf:", &[value]);
        assert_eq!(call(e, b"kind", &[]), key(b"notASymbol"));
        assert_eq!(bytes(call(e, b"message", &[])), b"nameOf: expects a symbol");
    }
    let cg = named(b"Codegen");
    let tcc = named(b"TCC");
    let temp = std::env::temp_dir().join(format!("birnam-artifact-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let file = temp.join("module.birnam");
    let artifact = temp.join("libbirnam_durable.dylib");
    let payload = b"owned\0\xff";
    std::fs::write(temp.join("asset"), payload).unwrap();
    let source = b"[| [System embedFile: \"asset\"]]";
    std::fs::write(&file, source).unwrap();
    let path = text(artifact.as_os_str().as_encoded_bytes());
    let compiled = call(
        cg,
        b"compile:output:from:",
        &[
            text(source),
            path,
            text(file.as_os_str().as_encoded_bytes()),
        ],
    );
    assert_eq!(compiled, path);
    assert!(std::fs::read(&artifact).unwrap().starts_with(b"\x7fELF"));
    std::fs::remove_file(file).unwrap();
    std::fs::remove_file(temp.join("asset")).unwrap();
    assert_ne!(
        call(named(b"Artifact"), b"inspect:", &[path]),
        named(b"nil")
    );
    let binding = |vt, n: &[u8]| call(vt, b"lookup", &[key(n)]);
    let set = |vt, n: &[u8], m| {
        call(vt, b"addMethod", &[key(n), m]);
    };
    let tcc_vt = call(host.id.root, b"behaviorOf:", &[tcc]);
    let lang_vt = call(host.id.root, b"behaviorOf:", &[named(b"Lang")]);
    let failure = unsafe {
        host.id.send(
            named(b"Execution"),
            key(b"failNative:detail:"),
            &[size(909).unwrap(), size(73).unwrap()],
        )
    }
    .unwrap_err();
    q.keep(failure).unwrap();
    let forbidden = hooks.context(&[failure, size(0).unwrap()]);
    let deny = hooks.method(b"deny", forbidden);
    let mut displaced = vec![];
    for (vt, name) in [
        (tcc_vt, b"compile:".as_slice()),
        (tcc_vt, b"compileObject:"),
        (lang_vt, b"parse:"),
    ] {
        displaced.push((vt, name, binding(vt, name)));
        set(vt, name, deny);
    }
    let mut collectors = vec![];
    for (vt, name) in [
        (
            call(host.id.root, b"behaviorOf:", &[named(b"Artifact")]),
            b"inspectBytes:from:".as_slice(),
        ),
        (tcc_vt, b"loadObject:"),
    ] {
        let original = binding(vt, name);
        let state = hooks.context(&[original, gc, size(0).unwrap()]);
        set(vt, name, hooks.method(b"collectEvery", state));
        collectors.push((vt, name, original, state));
    }
    let collect = || {
        host.send(gc, b"collectPrecisely", &[]).unwrap();
        host.send(tcc, b"checkpoint", &[]).unwrap();
    };
    let units = || as_size(host.send(tcc, b"liveNativeUnits", &[]).unwrap()).unwrap();
    collect();
    let before = units();
    let loaded = host.scope();
    let block = loaded.call(cg, b"load:", &[path]).unwrap();
    assert_ne!(block, named(b"nil"));
    loaded.check_result(block).unwrap();
    collect();
    let value = loaded.call(block, b"value", &[]).unwrap();
    assert_eq!(loaded.read_bytes(value).unwrap(), payload);
    assert_eq!(units(), before + 1);
    for (vt, name, original, state) in collectors {
        assert!(hooks.count(state, 2) > 0);
        set(vt, name, original);
    }
    for (vt, name, original) in displaced {
        set(vt, name, original);
    }
    assert_eq!(hooks.count(forbidden, 1), 0);
    let data_pin = host.scope();
    data_pin.keep(value).unwrap();
    let block_pin = host.scope();
    block_pin.keep(block).unwrap();
    drop(loaded);
    collect();
    {
        let check = host.scope();
        let value = check.call(block, b"value", &[]).unwrap();
        assert_eq!(check.read_bytes(value).unwrap(), payload);
    }
    drop(block_pin);
    collect();
    assert_eq!(units(), before);
    assert_eq!(data_pin.read_bytes(value).unwrap(), payload);
    drop(data_pin);
    let original = binding(tcc_vt, b"loadObject:");
    set(tcc_vt, b"loadObject:", deny);
    assert_eq!(
        unsafe { host.id.send(cg, key(b"load:"), &[path]) }.unwrap_err(),
        failure
    );
    set(tcc_vt, b"loadObject:", original);
    assert_eq!(hooks.count(forbidden, 1), 1);
    let old = std::fs::read(&artifact).unwrap();
    assert_eq!(
        call(cg, b"compile:output:", &[text(b"[bad"), path]),
        named(b"nil")
    );
    assert_eq!(std::fs::read(&artifact).unwrap(), old);
    let mut corrupt = old.clone();
    let magic = b"BIRNAM-ARTIFACT-DESCRIPTOR-V1";
    let start = corrupt
        .windows(magic.len())
        .position(|v| v == magic)
        .unwrap()
        + magic.len();
    let mut offset = start + 4;
    for _ in 0..4 {
        let n = u32::from_le_bytes(corrupt[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4 + n;
    }
    offset += 4;
    corrupt[offset] = if corrupt[offset] == b'0' { b'1' } else { b'0' };
    std::fs::write(&artifact, corrupt).unwrap();
    assert_eq!(call(cg, b"load:", &[path]), named(b"nil"));
    assert!(String::from_utf8_lossy(&bytes(call(cg, b"lastError", &[]))).contains("digest"));
    std::fs::write(&artifact, old).unwrap();
    let restored = call(cg, b"load:", &[path]);
    q.check_result(restored).unwrap();
    assert_ne!(restored, named(b"nil"));
    std::fs::remove_dir_all(temp).unwrap();
}
