//! Exercise scheduler initialization after restoring the published startup image.
#[allow(dead_code)]
#[path = "support/oop.rs"]
mod oop;
use std::os::unix::ffi::OsStrExt;
#[test]
#[ignore = "requires an installed production prefix"]
fn installed_scheduler_resets_nil_and_resumes_a_waiter() {
    let h = oop::bootstrap();
    let q = h.scope();
    let r = &h.resources;
    let mut args = vec![h.namespace];
    for path in [
        &r.prefix,
        &r.lib_dir,
        &r.rust_workspace_dir,
        &r.guide_dir,
        &r.id_dylib,
    ] {
        args.push(q.string(path.as_os_str().as_bytes()).unwrap());
    }
    h.load_native(&q, &r.lib_dir.join("libbirnam_host_entry.dylib"), &args)
        .unwrap();
    h.owner_call(
        &q,
        b"HostEntry",
        b"bootstrap:tools:",
        &[h.named(b"false").unwrap(), h.named(b"false").unwrap()],
    )
    .unwrap();
    let source = br#"
 [TaskExecutor queue: nil];
 let cleared := [[TaskExecutor queue] isNil];
 [TaskExecutor initialize];
 let empty := [[TaskExecutor queue] = #[]];
 let ran := [false asCell];
 let task := [TaskControl start: [:self | [self wait]; [ran set: true]; 37]];
 let waiting := [[task pollOutcome] isNil];
 let token := [task waitToken];
 let woke := [task resolve: token with: nil];
 #[cleared empty waiting woke [[task pollOutcome] = #[#success 37]] [ran value]]
 "#;
    let result = q
        .call(
            h.named(b"Codegen").unwrap(),
            b"eval:",
            &[q.string(source).unwrap()],
        )
        .unwrap();
    q.check_result(result).unwrap();
    assert_eq!(q.vector_size(result).unwrap(), 6);
    for i in 0..6 {
        assert_eq!(
            q.vector_at(result, i).unwrap(),
            h.named(b"true").unwrap(),
            "installed scheduler check {i}"
        );
    }
}
