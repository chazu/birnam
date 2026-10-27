//! Tracing, finalization, phase boundaries and current dispatch through Oops.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use std::{path::Path, ptr};
#[test]
#[ignore = "requires production Oop modules"]
fn bootstrap_memory_preserves_tracing_finalization_and_live_dispatch() {
    let child = std::env::var_os("BIRNAM_GC_RELEASE_FAILURE_CHILD").is_some();
    if !child {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--include-ignored",
                "--exact",
                "bootstrap_memory_preserves_tracing_finalization_and_live_dispatch",
                "--nocapture",
            ])
            .env("BIRNAM_GC_RELEASE_FAILURE_CHILD", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let gc = h.named(b"GC").unwrap();
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    assert_eq!(
        h.load_native(
            &q,
            &h.resources.lib_dir.join("libbirnam_collect.dylib"),
            &[h.roots]
        )
        .unwrap(),
        gc
    );
    let path = std::env::temp_dir().join(format!("birnam-memory-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/memory.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = h.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let nodes = call(h.named(b"ObjectVt").unwrap(), b"delegated", &[]);
    let owner_a = call(h.named(b"Object").unwrap(), b"clone", &[]);
    let owner_b = call(h.named(b"Object").unwrap(), b"clone", &[]);
    let a_vt = call(h.id.root, b"ensurePrivateVtableFor:", &[owner_a]);
    let b_vt = call(h.id.root, b"ensurePrivateVtableFor:", &[owner_b]);
    let name = |bytes: &[u8]| {
        call(
            h.id.root,
            b"nameWithBytes",
            &bytes
                .iter()
                .map(|&b| size(b as usize).unwrap())
                .collect::<Vec<_>>(),
        )
    };
    let state = hooks.context(&[
        gc,
        h.roots,
        nodes,
        owner_a,
        owner_b,
        name(b"rootsDo:"),
        name(b"a-name-that-must-not-be-created-during-gc"),
        name(b"weak-gc-symbol"),
        hooks.fault(917),
        fixture,
        call(h.id.root, b"behaviorOf:", &[gc]),
        call(h.id.root, b"behaviorOf:", &[h.id.root]),
        ptr::null_mut(),
        ptr::null_mut(),
        ptr::null_mut(),
        ptr::null_mut(),
    ]);
    let method = |n: &[u8], c| {
        call(
            call(fixture, b"methodAt:", &[key(n)]),
            b"withContext:",
            &[c],
        )
    };
    for (i, n) in [
        (12, b"firstVisit".as_slice()),
        (13, b"nextVisit"),
        (14, b"refuse"),
        (15, b"countedReferences"),
    ] {
        hooks.put(state, i, method(n, state));
    }
    for (tag, vt) in [(0, a_vt), (1, b_vt)] {
        let context = hooks.context(&[state, size(tag).unwrap()]);
        for (public, implementation) in [
            (b"referencesOf:do:".as_slice(), b"trace".as_slice()),
            (b"finalize:", b"finalize"),
        ] {
            call(
                vt,
                b"addMethod",
                &[key(public), method(implementation, context)],
            );
        }
    }
    for n in [
        b"allocation:bytes:identity:".as_slice(),
        b"run",
        b"memorySavedRelease:",
    ] {
        call(a_vt, b"addMethod", &[key(n), method(n, state)]);
    }
    let result = call(
        owner_a,
        if child {
            b"memorySavedRelease:"
        } else {
            b"run"
        },
        &[],
    );
    assert_eq!(as_size(result), Some(1));
}
