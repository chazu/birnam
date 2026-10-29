//! Actual authored initializers on an Oop floor without Store or Relational.
#[path = "support/minimal.rs"]
#[allow(dead_code)]
mod minimal;
use minimal::H;
use std::path::PathBuf;
#[test]
#[ignore = "isolated artifact floor supplied by the corresponding script"]
fn alien_wraps_without_store_or_relational() {
    let directory = PathBuf::from(std::env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
    for name in ["store", "relational", "bootstrap", "image", "isolate"] {
        assert!(!directory.join(format!("libbirnam_{name}.dylib")).exists());
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
    ] {
        h.artifact(name);
    }
    h.mount("alien");
    let eval = |source: &[u8]| h.eval(source);
    for name in [b"Store".as_slice(), b"db", b"Relational"] {
        assert!(h.call(host.namespace, b"at:", &[h.key(name)]).is_null());
    }
    assert_eq!(
        eval(
            br#"[[|
let before := [AlienRecipes objectFor: "labs" signature: "l l"];
let wrapped := [Alien wrap: "labs" signature: "l l" as: #floorAbsolute on: Integer];
let code := [AlienRecipes objectFor: "labs" signature: "l l"];
[System gc];
[[[before isNil] & [wrapped == Integer]] & [[[-7 floorAbsolute] = 7] & [code notNil]]]
] value]"#
        ),
        h.named(b"true")
    );
    assert_eq!(eval(br#"[[|
let code := [AlienRecipes objectFor: "labs" signature: "l l"];
[Alien wrap: "labs" signature: "l l" as: #floorAbsolute on: Integer];
let reused := [code == [AlienRecipes objectFor: "labs" signature: "l l"]];
let active := [[AlienRecipes snapshotRegistrations] size];
[Reflect defineMethod: #floorAbsolute on: Integer as: [:self | 99]];
[System gc];
[reused & [[active = 1] & [[[-7 floorAbsolute] = 99] & [[[AlienRecipes snapshotRegistrations] size] = 0]]]]
] value]"#),h.named(b"true"));
    h.clear_from(0);
    h.collect();
}
