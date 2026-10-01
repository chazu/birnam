//! Callback rooting, nested ownership and revocation through the real Id runtime.
use super::*;
use std::path::PathBuf;

#[test]
#[ignore = "requires installed modules; scripts/host-cli-test.sh"]
fn scoped_project_bridge_revokes_retained_callbacks() {
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
    let mut retained = None;
    host.with_project_bridge(|| {
        let project = host.global(&q, b"Project")?;
        retained = Some(project);
        let uri = q.string(b"file:///tmp/birnam%20bridge/example.birnam")?;
        host.send(host.named(b"GC")?, b"collectPrecisely", &[])?;
        let decoded = q.call(project, b"pathForUri:", &[Operand::object(uri)])?;
        assert_eq!(
            q.string_bytes(decoded)?,
            b"/tmp/birnam bridge/example.birnam"
        );
        let malformed = q.string(b"https://example.test/not-a-file")?;
        let rejected = q.call(project, b"pathForUri:", &[Operand::object(malformed)])?;
        let key = q.string(b"ok")?;
        assert_eq!(
            q.call(rejected, b"at:", &[Operand::object(key)])?,
            host.named(b"false")?
        );
        assert!(
            q.call(project, b"pathForUri:", &[])
                .unwrap_err()
                .0
                .contains("detail=411")
        );
        let nested: Result<(), HostError> = host.with_project_bridge(|| {
            assert_ne!(host.global(&q, b"Project")?, project);
            Err(HostError("fixture error".into()))
        });
        assert_eq!(nested.unwrap_err().0, "fixture error");
        assert_eq!(host.global(&q, b"Project")?, project);
        Ok(())
    })
    .unwrap();
    assert_eq!(host.global(&q, b"Project").unwrap(), original);
    let uri = q.string(b"file:///tmp/test").unwrap();
    assert!(
        q.call(retained.unwrap(), b"pathForUri:", &[Operand::object(uri)])
            .unwrap_err()
            .0
            .contains("detail=410")
    );
    let mut unwound = None;
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _: Result<(), HostError> = host.with_project_bridge(|| {
            unwound = Some(host.global(&q, b"Project")?);
            panic!("test bridge unwinding")
        });
    }));
    assert!(panic.is_err());
    assert_eq!(host.global(&q, b"Project").unwrap(), original);
    assert!(
        q.call(
            unwound.unwrap(),
            b"snapshotForUri:",
            &[Operand::object(uri)]
        )
        .unwrap_err()
        .0
        .contains("detail=410")
    );
}
