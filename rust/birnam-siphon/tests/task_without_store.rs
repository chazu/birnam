//! Actual authored initializers on an Oop floor without Store or Relational.
#[path = "support/minimal.rs"]
#[allow(dead_code)]
mod minimal;
use birnam_abi::size;
use minimal::H;
use std::path::{Path, PathBuf};
#[test]
#[ignore = "isolated artifact floor supplied by the corresponding script"]
fn tasks_work_without_store_or_relational() {
    let directory = PathBuf::from(std::env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
    for name in [
        "store",
        "relational",
        "bootstrap",
        "image",
        "alien",
        "isolate",
    ] {
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
        "imageFormat",
        "trace",
        "globals",
        "stdlib",
    ] {
        h.artifact(name);
    }
    let globals = h.named(b"Globals");
    let module = h.call(globals, b"at:", &[h.key(b"Module")]);
    h.call(module, b"markLoaded:", &[h.key(b"stdlib")]);
    h.artifact("stacklessControl");
    for name in [b"Store".as_slice(), b"db", b"Relational"] {
        assert!(h.call(host.namespace, b"at:", &[h.key(name)]).is_null());
    }
    let eval = |source: &[u8]| h.eval(source);
    assert_eq!(
        eval(
            br#"[[|
let first := ["same" , " value"]; let other := ["same" , " value"];
let cell := [Cell with: first];
let refused := [cell replace: other with: nil];
let unchanged := [[cell value] == first];
let accepted := [cell replace: first with: nil]; [System gc];
[[[[first = other] & [[first == other] not]] & [refused == false]] &
 [unchanged & [accepted & [[cell value] isNil]]]]
] value]"#
        ),
        h.named(b"true")
    );
    // Production loads this authored module from source into the startup image.
    // Run its actual initializer; its resource methods still require Alien/Image
    // when called, but owned preparation fields do not require Store.
    h.call(module, b"markLoaded:", &[h.key(b"stacklessControl")]);
    let initialized = eval(include_bytes!("../../../modules/isolate.birnam"));
    assert_eq!(h.call(initialized, b"isError", &[]), h.named(b"false"));
    assert_eq!(
        eval(
            br#"[[|
let owner := [Concurrency clone];
let left := [owner field: #workspace]; let right := [owner field: #workspace];
[left set: #owned]; [System gc];
[[[right value] == #owned] & [[owner workspace] == #owned]]
] value]"#
        ),
        h.named(b"true")
    );

    let initialized = eval(include_bytes!("../../../modules/repl.birnam"));
    assert_eq!(h.call(initialized, b"isError", &[]), h.named(b"false"));
    assert_eq!(
        eval(
            br#"[[|
let calls := [0 asCell]; let buffer := [Object clone];
[Reflect defineMethod: #free on: buffer as: [:self | [calls inc]; [System gc]; nil]];
[Repl installConsoleState: #[#borrowedInput #borrowedError buffer]];
let alias := [Repl clone]; [Repl closeConsoleResources]; [alias closeConsoleResources];
[[[calls value] = 1] & [[[alias consoleInput] isNil] & [[Repl consoleBuffer] isNil]]]
] value]"#
        ),
        h.named(b"true")
    );
    let result = eval(
        br#"[[|
let task := [TaskControl start: [:self | [self wait]; 42]];
let token := [task waitToken];
[System gc];
[task resolve: token with: nil];
[[task pollOutcome] = #[#success 42]]
] value]"#,
    );
    assert_eq!(result, h.named(b"true"));
    let result=eval(br#"[[|
let cleaned := [0 asCell];
let task := [TaskControl start: [:self | [self ensure: [| [self wait]] cleanup: [| [cleaned inc]]]]];
[task cancel: #stop];
[[[task pollOutcome] = #[#cancelled #stop]] & [[cleaned value] = 1]]
] value]"#);
    assert_eq!(result, h.named(b"true"));
    // The retained field view must be the only root of its owner and payload.
    let q = host.scope();
    let library =
        std::env::temp_dir().join(format!("birnam-task-presence-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/allocation-presence.c")
            )
            .arg("-o")
            .arg(&library)
            .status()
            .unwrap()
            .success()
    );
    let fixture = host.load_native(&q, &library, &[]).unwrap();
    std::fs::remove_file(library).unwrap();
    let monitor = q.call(h.named(b"Object"), b"clone", &[]).unwrap();
    let vt = q
        .call(host.id.root, b"ensurePrivateVtableFor:", &[monitor])
        .unwrap();
    for n in [b"allocation:bytes:identity:".as_slice(), b"present:"] {
        let selector = q.keep(host.intern(n).unwrap()).unwrap();
        let method = q.call(fixture, b"methodAt:", &[selector]).unwrap();
        q.call(vt, b"addMethod", &[selector, method]).unwrap();
    }
    let identity = |value| q.call(host.id.root, b"identityOf:", &[value]).unwrap();
    let present = |id| q.call(monitor, b"present:", &[id]).unwrap() == size(1).unwrap();
    let start = h.mark();
    let task_control = h.call(globals, b"at:", &[h.key(b"TaskControl")]);
    let owner = h.call(task_control, b"clone", &[]);
    let payload = h.call(h.named(b"Object"), b"clone", &[]);
    h.call(owner, b"cancelReason:", &[payload]);
    let view = h.call(owner, b"field:", &[h.key(b"cancelReason")]);
    let ids = [identity(owner), identity(payload), identity(view)];
    let view_root = host.send(host.roots, b"retain:", &[view]).unwrap();
    h.clear_from(start);
    h.collect();
    assert!(ids.iter().all(|id| present(*id)));
    assert_eq!(h.call(view, b"value", &[]), payload);
    h.clear_from(start);
    host.send(host.roots, b"release:", &[view_root]).unwrap();
    h.collect();
    assert!(ids.iter().all(|id| !present(*id)));
    h.clear_from(0);
}
