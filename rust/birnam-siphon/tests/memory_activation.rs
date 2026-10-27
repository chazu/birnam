//! Collector activation over the existing Id heap, without a language bootstrap.
use birnam_abi::as_size;
use std::{
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};
#[test]
#[ignore = "requires production Oop modules"]
fn activation_requires_complete_representation_owners_without_replacing_the_heap() {
    let directory = PathBuf::from(std::env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
    let fixture = std::env::temp_dir().join(format!(
        "birnam-memory-activation-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/memory-activation.c")
            )
            .arg("-o")
            .arg(&fixture)
            .status()
            .unwrap()
            .success()
    );
    let roots_path = directory.join("libbirnam_root_registry.dylib");
    let gc_path = directory.join("libbirnam_collect.dylib");
    let id = birnam_siphon::load(
        directory.join("libbirnam_id.dylib").as_os_str().as_bytes(),
        &[
            b"sequence",
            roots_path.as_os_str().as_bytes(),
            gc_path.as_os_str().as_bytes(),
            fixture.as_os_str().as_bytes(),
        ],
    )
    .unwrap();
    unsafe {
        let sequence = id.send(id.literal(0).unwrap(), id.intern, &[]).unwrap();
        let empty = id.send(id.root, sequence, &[]).unwrap();
        let roots = id.send(id.literal(1).unwrap(), id.load, &[empty]).unwrap();
        let arguments = id
            .send(id.root, sequence, &[roots, id.literal(2).unwrap()])
            .unwrap();
        let result = id
            .send(id.literal(3).unwrap(), id.load, &[arguments])
            .unwrap();
        assert_eq!(as_size(result), Some(1));
    }
    std::fs::remove_file(fixture).unwrap();
}
