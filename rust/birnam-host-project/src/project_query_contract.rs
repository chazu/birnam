//! Permanent queries retain their owner, use current policy, and never publish
//! a temporary Project global or a callback borrowing a stack Context.
use crate::{
    StartupPolicy,
    test_support::{HostRuntime, create_check_temp_directory},
};
use birnam_abi::id_protocol::Operand;
use std::{
    env, fs,
    path::{Path, PathBuf},
};
#[test]
#[ignore = "requires installed modules; scripts/project-query-test.py"]
fn project_queries_are_rooted_and_do_not_install_a_global_bridge() {
    let lib = PathBuf::from(env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
    let resources = crate::resolve_from_exe_dir(
        Path::new("/unused/bin"),
        crate::Overrides {
            lib_dir: Some(&lib),
            ..Default::default()
        },
    )
    .unwrap();
    let host = HostRuntime::start(
        &resources,
        StartupPolicy {
            skip_stdlib: false,
            precise_gc: true,
            source_bootstrap: false,
            tool_bootstrap: false,
        },
    )
    .unwrap();
    let q = host.scope();
    let original = host.global(&q, b"Project").unwrap();
    let project = host.named(b"HostProject").unwrap();
    let uri = q
        .string(b"file:///tmp/birnam%20query/example.birnam")
        .unwrap();
    host.send(host.named(b"GC").unwrap(), b"collectPrecisely", &[])
        .unwrap();
    let path = q
        .call(project, b"pathForUri:", &[Operand::object(uri)])
        .unwrap();
    assert_eq!(
        q.string_bytes(path).unwrap(),
        b"/tmp/birnam query/example.birnam"
    );
    let malformed = q.string(b"https://example.test/not-a-file").unwrap();
    let rejected = q
        .call(project, b"pathForUri:", &[Operand::object(malformed)])
        .unwrap();
    let key = q.string(b"ok").unwrap();
    assert_eq!(
        q.call(rejected, b"at:", &[Operand::object(key)]).unwrap(),
        host.named(b"false").unwrap()
    );
    let wrong = q.call(project, b"pathForUri:", &[]).unwrap();
    assert!(q.check_result(wrong).is_err());
    let temp = create_check_temp_directory().unwrap();
    fs::create_dir(temp.0.join("src")).unwrap();
    fs::write(
        temp.0.join("birnam.toml"),
        b"name=\"query\"\nload=[\"example\"]\n",
    )
    .unwrap();
    fs::write(
        temp.0.join("src/example.birnam"),
        b"[Module named: #example requires: #[]]; let queryMustNotExecute := 42;",
    )
    .unwrap();
    let uri = q
        .string(format!("file://{}", temp.0.join("src/example.birnam").display()).as_bytes())
        .unwrap();
    let snapshot = q
        .call(project, b"snapshotForUri:", &[Operand::object(uri)])
        .unwrap();
    assert_eq!(
        q.call(snapshot, b"at:", &[Operand::object(key)]).unwrap(),
        host.named(b"true").unwrap()
    );
    assert_eq!(
        host.global(&q, b"queryMustNotExecute").unwrap(),
        host.named(b"nil").unwrap()
    );
    assert_eq!(host.global(&q, b"Project").unwrap(), original);
    // Retain the receiver beyond a query scope, then collect and send again. There
    // is no closed callback state: the completed native owner has process lifetime.
    host.send(host.named(b"GC").unwrap(), b"collectPrecisely", &[])
        .unwrap();
    let repeated = q
        .call(project, b"snapshotForUri:", &[Operand::object(uri)])
        .unwrap();
    assert_eq!(
        q.call(repeated, b"at:", &[Operand::object(key)]).unwrap(),
        host.named(b"true").unwrap()
    );
    assert!(!temp.0.join(".birnam/build").exists());
    assert!(!temp.0.join(".birnam/deps").exists());
}
