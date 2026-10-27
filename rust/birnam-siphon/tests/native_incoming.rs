//! Incoming Oop normalization, snapshot order and primitive-only initialization.
#[path = "../../birnam-host/src/test_hooks.rs"]
#[allow(dead_code)]
mod hooks;
use birnam_abi::{Oop, as_size, size};
use birnam_host_support::{Context, Resources};
use std::{
    env,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

#[test]
#[ignore = "requires production Oop modules"]
fn incoming_signatures_preserve_values_and_early_bootstrap() {
    let lib = PathBuf::from(env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
    let prefix = lib.parent().unwrap().to_path_buf();
    let resources = Resources {
        id_dylib: lib.join("libbirnam_id.dylib"),
        lib_dir: lib.clone(),
        rust_workspace_dir: prefix.join("share/birnam/src"),
        guide_dir: prefix.join("share/birnam/docs/guide"),
        prefix,
    };
    let fixture = env::temp_dir().join(format!(
        "birnam-native-incoming-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/native-incoming.c")
            )
            .arg("-o")
            .arg(&fixture)
            .status()
            .unwrap()
            .success()
    );
    let bootstrap = lib.join("libbirnam_bootstrap.dylib");
    let portable = lib.join("libbirnam_portable_code.dylib");
    let id = birnam_siphon::load(
        resources.id_dylib.as_os_str().as_bytes(),
        &[
            fixture.as_os_str().as_bytes(),
            bootstrap.as_os_str().as_bytes(),
            lib.as_os_str().as_bytes(),
            b"nameWithBytes",
            b"sequence",
            portable.as_os_str().as_bytes(),
        ],
    )
    .unwrap();
    let (h, module) = unsafe {
        let constructor = id.send(id.literal(3).unwrap(), id.intern, &[]).unwrap();
        let sequence = id.send(id.literal(4).unwrap(), id.intern, &[]).unwrap();
        let args = id
            .send(
                id.root,
                sequence,
                &[id.literal(1).unwrap(), id.literal(5).unwrap()],
            )
            .unwrap();
        let module = id.send(id.literal(0).unwrap(), id.load, &[args]).unwrap();
        let args = id
            .send(id.root, sequence, &[id.literal(2).unwrap()])
            .unwrap();
        let ns = id.send(id.literal(1).unwrap(), id.load, &[args]).unwrap();
        (
            Context::from_namespace(id, ns, resources, constructor).unwrap(),
            module,
        )
    };
    let q = h.scope();
    q.keep(module).unwrap();
    let hooks = hooks::Hooks::new(&h);
    let key = |name: &[u8]| hooks.symbol(name);
    let named = |name: &[u8]| h.named(name).unwrap();
    let call = |r, name: &[u8], args: &[Oop]| q.call(r, name, args).unwrap();
    let declared = |name: &[u8]| call(module, b"methodAt:", &[key(name)]);
    assert_eq!(
        as_size(call(declared(b"restore"), b"invokeOn:", &[module])),
        Some(3)
    );
    std::fs::remove_file(fixture).unwrap();
    let gc = named(b"GC");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let source=q.string(br#"
      let IncomingProbe := [Object clone];
      let echo := [Execution block: [:self :bits :data | [System gc]; data] withNativeSignature: "bwb"];
      [Reflect defineMethod: #echoWord:bytes: on: IncomingProbe as: echo];
      let wordEcho := [Execution block: [:self :bits | bits] withNativeSignature: "ww"];
      [Reflect defineMethod: #word: on: IncomingProbe as: wordEcho];
      let optional := [Execution block: [:self :value | nil] withNativeSignature: "Wo"];
      [Reflect defineMethod: #optional: on: IncomingProbe as: optional];
      let pair := [Execution block: [:self :first :second | [System gc]; second] withNativeSignature: "bbb"];
      [Reflect defineMethod: #firstBytes:secondBytes: on: IncomingProbe as: pair];
      let required := [Execution block: [:self :bits :value | value] withNativeSignature: "owO"];
      [Reflect defineMethod: #requiredWord:object: on: IncomingProbe as: required];
      let code := [wordEcho codeObject];
      let signature := [code nativeSignature];
      [signature appendBytes: "ignored"];
      #[IncomingProbe code signature]
    "#).unwrap();
    let values = call(named(b"Codegen"), b"eval:", &[source]);
    q.check_result(values).unwrap();
    let probe = q.vector_at(values, 0).unwrap();
    let code = q.vector_at(values, 1).unwrap();
    assert_eq!(
        q.read_bytes(q.vector_at(values, 2).unwrap()).unwrap(),
        b"wwignored"
    );
    let word = named(b"Word");
    let behavior = call(h.id.root, b"behaviorOf:", &[word]);
    let selector = key(b"fromBits:");
    let original = call(behavior, b"lookup", &[selector]);
    let input = call(
        named(b"Bytes"),
        b"fromBytes:",
        &[q.byte_values(b"original").unwrap()],
    );
    let full = q.word(usize::MAX).unwrap();
    let context = hooks.context(&[original, gc, input, size(0).unwrap()]);
    let replacement = call(declared(b"mutate"), b"withContext:", &[context]);
    call(behavior, b"addMethod", &[selector, replacement]);
    let result = call(probe, b"echoWord:bytes:", &[full, input]);
    assert_eq!(
        q.read_bytes(result).unwrap(),
        b"original",
        "byte input must be snapshotted before the Word factory runs"
    );
    assert_eq!(q.read_bytes(input).unwrap(), b"originalzzzzzzzz");
    assert!(hooks.count(context, 3) >= 1);
    let before = hooks.count(context, 3);
    assert!(
        h.send(
            probe,
            b"requiredWord:object:",
            &[full, std::ptr::null_mut()]
        )
        .is_err()
    );
    assert_eq!(
        hooks.count(context, 3),
        before,
        "a malformed non-null argument must be rejected before conversion callbacks"
    );
    call(behavior, b"addMethod", &[selector, original]);
    let bytes = named(b"Bytes");
    let bytes_vt = call(h.id.root, b"behaviorOf:", &[bytes]);
    let from_bytes = key(b"fromBytes:");
    let original_bytes = call(bytes_vt, b"lookup", &[from_bytes]);
    let first = call(bytes, b"fromBytes:", &[q.byte_values(b"first").unwrap()]);
    let second = call(bytes, b"fromBytes:", &[q.byte_values(b"second").unwrap()]);
    let state = hooks.context(&[original_bytes, gc, second, size(0).unwrap()]);
    let replacement = call(declared(b"mutate"), b"withContext:", &[state]);
    call(bytes_vt, b"addMethod", &[from_bytes, replacement]);
    let result = call(probe, b"firstBytes:secondBytes:", &[first, second]);
    call(bytes_vt, b"addMethod", &[from_bytes, original_bytes]);
    assert_eq!(q.read_bytes(result).unwrap(), b"second");
    assert!(q.read_bytes(second).unwrap().len() > b"second".len());
    assert!(
        hooks.count(state, 3) >= 2,
        "both input factories must have run"
    );

    for bits in [0, 1, 1usize << 63, usize::MAX] {
        let result = call(probe, b"word:", &[q.word(bits).unwrap()]);
        assert_eq!(q.word_bits(result).unwrap(), bits);
    }
    assert!(h.send(probe, b"word:", &[named(b"nil")]).is_err());
    assert!(h.send(probe, b"word:", &[]).is_err());
    assert!(call(probe, b"optional:", &[std::ptr::null_mut()]).is_null());
    let method = call(probe, b"methodFor:", &[key(b"word:")]);
    let vt = call(h.id.root, b"ensurePrivateVtableFor:", &[probe]);
    call(vt, b"addMethod:as:", &[method, key(b"copiedWord:")]);
    call(gc, b"collectPrecisely", &[]);
    assert_eq!(
        q.word_bits(call(probe, b"copiedWord:", &[q.word(99).unwrap()]))
            .unwrap(),
        99
    );
    assert_eq!(
        q.read_bytes(call(code, b"nativeSignature", &[])).unwrap(),
        b"ww"
    );
}
