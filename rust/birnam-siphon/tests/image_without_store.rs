//! Real compiled owners and source-free Image recovery on an isolated Oop floor.
#[path = "support/minimal.rs"]
#[allow(dead_code)]
mod minimal;
use minimal::H;
use std::path::PathBuf;
#[test]
#[ignore = "scripts/image-without-store-test.py supplies the isolated artifact floor"]
fn image_roundtrips_without_store_or_relational() {
    let directory = PathBuf::from(std::env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
    let mode = std::env::var("BIRNAM_IMAGE_FLOOR_MODE").unwrap();
    for name in ["relational", "bootstrap", "isolate"] {
        assert!(!directory.join(format!("libbirnam_{name}.dylib")).exists());
    }
    if !mode.starts_with("optional-") {
        assert!(!directory.join("libbirnam_store.dylib").exists());
    }
    let host = minimal::service_floor();
    let h = H::new(&host);
    for name in [
        "error",
        "globalsOwner",
        "sourceRegistry",
        "cell",
        "modules",
        "lang",
        "nativeValues",
        "booleanValues",
        "integerValues",
        "blockLoopControl",
        "blockLoop",
        "range",
        "systemGc",
        "printing",
        "sourceLoader",
        "nativeLoader",
        "alienCompiler",
        "alienRecipes",
        "imageFormat",
        "trace",
        "globals",
        "stdlib",
        "cli",
    ] {
        h.artifact(name);
    }
    h.mount("alien");
    let eval = |source: &[u8]| h.eval(source);
    let module = h.call(h.named(b"Globals"), b"at:", &[h.key(b"Module")]);
    h.call(
        module,
        b"libDir:",
        &[h.text(directory.as_os_str().as_encoded_bytes())],
    );
    h.mount("image");
    for name in [b"Store".as_slice(), b"db", b"Relational"] {
        assert!(h.call(host.namespace, b"at:", &[h.key(name)]).is_null());
    }
    assert_eq!(
        h.call(h.named(b"System"), b"imageStore", &[]),
        h.named(b"nil")
    );
    let reflect = h.named(b"Reflect");
    let reflect_behavior = h.call(host.id.root, b"behaviorOf:", &[reflect]);
    assert_eq!(
        h.call(
            h.named(b"Execution"),
            b"directMethodFor:in:",
            &[h.key(b"factsOf:"), reflect_behavior]
        ),
        h.named(b"nil")
    );
    assert_eq!(
        eval(include_bytes!(
            "../../../tests/fixtures/documentation-hooks.birnam"
        )),
        h.named(b"true")
    );
    assert_eq!(
        eval(include_bytes!(
            "../../../tests/fixtures/cli-lsp-boundary.birnam"
        )),
        h.named(b"true")
    );
    h.collect();

    let path = std::env::var("BIRNAM_IMAGE_FLOOR_PATH").unwrap();
    let path = h.text(path.as_bytes());
    // The only added library in the positive controls is Store.
    if mode.starts_with("optional-") {
        h.mount("store");
        assert_eq!(eval(br#"[Module kernel: #store]; [Module field: #imageProvider of: #store put: true]; true"#),h.named(b"true"));
    }
    if mode == "optional-save" {
        assert_eq!(
            eval(
                br#"let optionalImageStore := Store;
[Reflect defineMethod: #imageStore on: System as: [:self | optionalImageStore]];
[Store assert: #optionalFloor attr: #value value: 7]; true"#
            ),
            h.named(b"true")
        );
        assert_eq!(
            h.call(h.named(b"System"), b"saveImage:", &[path]),
            h.named(b"true")
        );
    } else if mode == "save" {
        assert_eq!(
            eval(
                br#"let coreImageCell := [Cell with: #before];
let coreImageAlias := coreImageCell;
[Reflect defineMethod: #coreImageAnswer on: Integer as: [:self | 42]];
[Alien wrap: "labs" signature: "l l" as: #coreImageAbs on: Integer]; true"#
            ),
            h.named(b"true")
        );
        assert_eq!(
            h.call(h.named(b"System"), b"saveImage:", &[path]),
            h.named(b"true")
        );
    } else {
        let rebuild = mode.ends_with("rebuild");
        let answer = h.call(
            h.named(b"System"),
            b"loadImage:rebuild:",
            &[path, h.named(if rebuild { b"true" } else { b"false" })],
        );
        if mode == "missing" {
            assert!(
                answer == h.named(b"nil") || h.call(answer, b"isError", &[]) == h.named(b"true")
            );
            let report = h.call(h.named(b"System"), b"imageSaveReport", &[]);
            assert_eq!(h.call(report, b"status", &[]), h.key(b"rejected"));
        } else if mode.starts_with("optional-") {
            assert_eq!(answer, h.named(if rebuild { b"false" } else { b"true" }));
            assert_eq!(eval(br#"[[[Store currentValue: #optionalFloor attr: #value] = 7] & [[System imageStore] == Store]]"#),h.named(b"true"));
        } else {
            assert_eq!(answer, h.named(if rebuild { b"false" } else { b"true" }));
            assert_eq!(
                eval(
                    br#"[[|
[System gc];
let alias := [coreImageCell == coreImageAlias];
let before := [[coreImageCell value] == #before];
[coreImageAlias set: #changed]; [System gc];
let changed := [[coreImageCell value] == #changed];
[coreImageCell set: #before];
[[[alias & before] & changed] & [[[1 coreImageAnswer] = 42] & [[-9 coreImageAbs] = 9]]]
] value]"#
                ),
                h.named(b"true")
            );
            if mode == "resave" {
                let output = std::env::var("BIRNAM_IMAGE_FLOOR_OUTPUT").unwrap();
                assert_eq!(
                    h.call(
                        h.named(b"System"),
                        b"saveImage:",
                        &[h.text(output.as_bytes())]
                    ),
                    h.named(b"true")
                );
            }
        }
    }
    h.clear_from(0);
    h.collect();
}
