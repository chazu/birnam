//! Independent Oop witnesses for GC geometry, thresholds, failures and overflow.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use std::{os::unix::process::ExitStatusExt, path::Path, ptr};
#[test]
#[ignore = "requires production Oop modules"]
fn production_allocation_accounting() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let gc = h.named(b"GC").unwrap();
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let path = std::env::temp_dir().join(format!(
        "birnam-allocation-accounting-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/allocation-accounting.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = h.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let behavior = call(h.named(b"ObjectVt").unwrap(), b"delegated", &[]);
    let owner = call(h.named(b"Object").unwrap(), b"clone", &[]);
    let owner_vt = call(h.id.root, b"ensurePrivateVtableFor:", &[owner]);
    let root_vt = call(h.id.root, b"behaviorOf:", &[h.id.root]);
    let original = call(root_vt, b"lookup", &[key(b"allocationChargeFor:")]);
    let state = hooks.context(&[
        gc,
        h.roots,
        behavior,
        root_vt,
        owner,
        ptr::null_mut(),
        hooks.fault(83),
        original,
    ]);
    let method = |n: &[u8]| {
        call(
            call(fixture, b"methodAt:", &[key(n)]),
            b"withContext:",
            &[state],
        )
    };
    hooks.put(state, 5, method(b"geometry"));
    for name in [b"referencesOf:do:".as_slice(), b"run", b"overflow"] {
        call(owner_vt, b"addMethod", &[key(name), method(name)]);
    }
    let alias = key(b"accountingOriginalGeometry:");
    call(root_vt, b"addMethod", &[alias, original]);
    call(gc, b"trace:with:", &[behavior, owner]);
    if std::env::var_os("BIRNAM_ACCOUNTING_OVERFLOW_CHILD").is_some() {
        call(owner, b"overflow", &[]);
        panic!("committed charge overflow returned");
    }
    assert_eq!(as_size(call(owner, b"run", &[])), Some(1));
    call(root_vt, b"removeMethod:", &[alias]);
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "production_allocation_accounting",
            "--include-ignored",
            "--nocapture",
        ])
        .env("BIRNAM_ACCOUNTING_OVERFLOW_CHILD", "1")
        .output()
        .unwrap();
    assert_eq!(output.status.signal(), Some(6));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "birnam invariant failed: GC allocation byte counter overflow\n"
    );
}
