//! Collecting construction and extension installation preserve current bindings.
use birnam_abi::{as_size, size};
use birnam_host_support::{Context, Resources};
use std::{
    env,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

#[test]
#[ignore = "requires production Oop modules"]
fn complete_extensions_preserve_replacements_during_collecting_bootstrap() {
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
        "birnam-numbers-loading-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/numbers-loading.c")
            )
            .arg("-o")
            .arg(&fixture)
            .status()
            .unwrap()
            .success()
    );
    let bootstrap = lib.join("libbirnam_bootstrap.dylib");
    let id = birnam_siphon::load(
        resources.id_dylib.as_os_str().as_bytes(),
        &[
            fixture.as_os_str().as_bytes(),
            bootstrap.as_os_str().as_bytes(),
            lib.as_os_str().as_bytes(),
            b"nameWithBytes",
            b"sequence",
        ],
    )
    .unwrap();
    let (h, module) = unsafe {
        let constructor = id.send(id.literal(3).unwrap(), id.intern, &[]).unwrap();
        let sequence = id.send(id.literal(4).unwrap(), id.intern, &[]).unwrap();
        let args = id
            .send(id.root, sequence, &[id.literal(1).unwrap()])
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
    let restore = q
        .call(module, b"methodAt:", &[h.intern(b"restore").unwrap()])
        .unwrap();
    assert_eq!(
        as_size(q.call(restore, b"invokeOn:", &[module]).unwrap()),
        Some(63),
        "pressure window and all four replacements must be observed"
    );
    std::fs::remove_file(fixture).unwrap();
    let module = h.global(&q, b"Module").unwrap();
    let policy = q
        .string(
            lib.join("libbirnam_integerValues.dylib")
                .as_os_str()
                .as_bytes(),
        )
        .unwrap();
    assert_eq!(
        q.call(h.named(b"Codegen").unwrap(), b"load:", &[policy])
            .unwrap(),
        module
    );
    let left = q
        .call(
            h.named(b"Integer").unwrap(),
            b"fromWord:",
            &[size(3).unwrap()],
        )
        .unwrap();
    let right = q
        .call(
            h.named(b"Float").unwrap(),
            b"fromBits:",
            &[q.word(0.5f64.to_bits() as usize).unwrap()],
        )
        .unwrap();
    assert_eq!(q.call(left, b"+", &[right]).unwrap(), left);
    assert_eq!(q.call(left, b"to:", &[left]).unwrap(), left);
    let difference = q.call(left, b"-", &[right]).unwrap();
    assert_eq!(
        q.word_bits(q.call(difference, b"bits", &[]).unwrap())
            .unwrap(),
        2.5f64.to_bits() as usize
    );
    let text = q.string(b"123").unwrap();
    assert_eq!(q.call(text, b"size", &[]).unwrap(), text);
    assert_eq!(q.call(text, b"split:", &[text]).unwrap(), text);
    let parsed = q.call(text, b"asInteger", &[]).unwrap();
    assert_eq!(
        q.word_bits(q.call(parsed, b"word", &[]).unwrap()).unwrap(),
        123
    );
    h.send(h.named(b"GC").unwrap(), b"collectPrecisely", &[])
        .unwrap();
}
