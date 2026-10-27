//! Explicit native failure policy survives mapped and rebuilt images.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{self as abi, Oop, size};

#[test]
#[ignore = "requires production Oop modules"]
fn native_failure_channel_is_preserved_across_images() {
    let h = oop::bootstrap();
    let q = h.scope();
    let named = |n: &[u8]| h.named(n).unwrap();
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let gc = named(b"GC");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let dir = std::env::temp_dir().join(format!("birnam-failure-image-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let artifact = dir.join("policy.dylib");
    let image = dir.join("policy.image");
    let output = q.string(artifact.as_os_str().as_encoded_bytes()).unwrap();
    let image_path = q.string(image.as_os_str().as_encoded_bytes()).unwrap();
    let source = q
        .string(include_bytes!(
            "../../../tests/fixtures/publication-failure-image.birnam"
        ))
        .unwrap();
    let compiled = call(
        named(b"Codegen"),
        b"compile:output:from:synchronousBodies:",
        &[source, output, output, named(b"true")],
    );
    q.check_result(compiled).unwrap();
    assert_ne!(compiled, named(b"nil"));
    q.check_result(call(named(b"Codegen"), b"load:", &[output]))
        .unwrap();
    for mode in ["fresh", "mapped", "rebuilt"] {
        if mode == "fresh" {
            assert_eq!(
                call(named(b"System"), b"saveImage:", &[image_path]),
                named(b"true")
            );
            std::fs::remove_file(&artifact).unwrap();
        } else {
            let loaded = call(
                named(b"System"),
                b"loadImage:rebuild:",
                &[
                    image_path,
                    named(if mode == "rebuilt" { b"true" } else { b"false" }),
                ],
            );
            q.check_result(loaded).unwrap();
            assert_eq!(
                loaded,
                named(if mode == "rebuilt" { b"false" } else { b"true" })
            );
        }
        call(gc, b"collectPrecisely", &[]);
        let owner = h.global(&q, b"PublicationFailureImageOwner").unwrap();
        let failed = unsafe { h.id.send(owner, h.intern(b"publish").unwrap(), &[]) }.unwrap_err();
        assert_eq!(
            failed,
            abi::object_reference(h.id.invalid),
            "native policy failure changed after {mode} restoration"
        );
        eprintln!("image failure boundary {mode}: exact native invalidRequest preserved");
    }
    std::fs::remove_dir_all(dir).unwrap();
}
