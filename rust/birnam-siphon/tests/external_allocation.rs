//! Bytes capacity participates in GC pressure through current Oop owner messages.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use std::{path::Path, ptr};
#[test]
#[ignore = "requires production Oop modules"]
fn bytes_charge_capacity_and_survive_replaced_accounting() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let gc = h.named(b"GC").unwrap();
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let path = std::env::temp_dir().join(format!(
        "birnam-external-allocation-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/external-allocation.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = h.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let provider = call(h.named(b"Object").unwrap(), b"clone", &[]);
    let vt = call(h.id.root, b"ensurePrivateVtableFor:", &[provider]);
    let gc_vt = call(h.id.root, b"behaviorOf:", &[gc]);
    let original = call(gc_vt, b"lookup", &[key(b"chargeExternalAllocation:")]);
    let state = hooks.context(&[
        gc,
        h.roots,
        h.named(b"Bytes").unwrap(),
        provider,
        gc_vt,
        original,
        ptr::null_mut(),
        q.byte_values(b"a").unwrap(),
        q.byte_values(&vec![42; 4096]).unwrap(),
        q.byte_values(b"").unwrap(),
        q.byte_values(b"restored").unwrap(),
    ]);
    let method = |n: &[u8]| {
        call(
            call(fixture, b"methodAt:", &[key(n)]),
            b"withContext:",
            &[state],
        )
    };
    hooks.put(state, 6, method(b"collectingCharge"));
    for (selector, implementation) in [
        (b"sizeWord".as_slice(), b"largeSize".as_slice()),
        (b"copyByteValuesFrom:count:to:", b"largeByte"),
        (b"run", b"run"),
    ] {
        call(vt, b"addMethod", &[key(selector), method(implementation)]);
    }
    let alias = key(b"externalOriginalCharge:");
    call(gc_vt, b"addMethod", &[alias, original]);
    assert_eq!(as_size(call(provider, b"run", &[])), Some(1));
    call(gc_vt, b"removeMethod:", &[alias]);
}
