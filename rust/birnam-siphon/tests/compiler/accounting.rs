//! Compare compiler/execution charges with independent physical allocation records.
use super::*;
use std::{collections::BTreeMap, path::Path};
pub(super) fn check(h: &H<'_>) {
    h.reclaimed();
    let q = h.host.scope();
    let gc = h.named(b"GC");
    let library = std::env::temp_dir().join(format!(
        "birnam-compiler-accounting-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/compiler-accounting.c")
            )
            .arg("-o")
            .arg(&library)
            .status()
            .unwrap()
            .success()
    );
    let fixture = h.host.load_native(&q, &library, &[]).unwrap();
    std::fs::remove_file(library).unwrap();
    let source = q
        .string(b"let chargeInput := 35; [[:x | [chargeInput + x]] value: 7]")
        .unwrap();
    let context = h.hooks.context(&[gc, h.named(b"Codegen"), source]);
    let method = |n: &[u8]| {
        q.call(
            q.call(fixture, b"methodAt:", &[h.key(n)]).unwrap(),
            b"withContext:",
            &[context],
        )
        .unwrap()
    };
    let probe = q.call(h.named(b"Object"), b"clone", &[]).unwrap();
    let vt = q
        .call(h.host.id.root, b"ensurePrivateVtableFor:", &[probe])
        .unwrap();
    for n in [
        b"run".as_slice(),
        b"allocation:bytes:identity:",
        b"recordCount:",
        b"identityAt:in:",
        b"bytesAt:in:",
        b"measuredCharge",
        b"measuredExternal",
        b"clear",
    ] {
        q.call(vt, b"addMethod", &[h.key(n), method(n)]).unwrap();
    }
    let gc_vt = h.vt(gc);
    let charge = h.key(b"chargeExternalAllocation:");
    let alias = h.key(b"compilerOriginalCharge:");
    let original = q.call(gc_vt, b"lookup", &[charge]).unwrap();
    q.call(gc_vt, b"addMethod", &[alias, original]).unwrap();
    q.call(
        gc_vt,
        b"addMethod",
        &[charge, method(b"chargeExternalAllocation:")],
    )
    .unwrap();
    q.call(gc, b"budget:", &[size(usize::MAX >> 1).unwrap()])
        .unwrap();
    let result = q.call(probe, b"run", &[]).unwrap();
    q.call(gc_vt, b"addMethod", &[charge, original]).unwrap();
    q.call(gc_vt, b"removeMethod:", &[alias]).unwrap();
    assert_eq!(h.integer(result), 42);
    let count = h.key(b"recordCount:");
    let identity = h.key(b"identityAt:in:");
    let bytes = h.key(b"bytesAt:in:");
    let read = |selector, args: &[Oop]| {
        as_size(unsafe { h.host.id.send(probe, selector, args) }.unwrap()).unwrap()
    };
    let inventory = |which| {
        let which = size(which).unwrap();
        let mut entries = BTreeMap::new();
        for i in 0..read(count, &[which]) {
            let args = [size(i).unwrap(), which];
            assert!(
                entries
                    .insert(read(identity, &args), read(bytes, &args))
                    .is_none()
            );
        }
        entries
    };
    let before = inventory(0);
    let after = inventory(1);
    assert!(
        before.keys().all(|id| after.contains_key(id)),
        "no collection during measured compilation and execution"
    );
    // Independent 64-bit storage calculation: 8-byte header, 8-byte payload padding.
    let expected: usize = after
        .iter()
        .filter(|(id, _)| !before.contains_key(id))
        .map(|(_, size)| (size + 8 + 7) & !7)
        .sum();
    let external = as_size(q.call(probe, b"measuredExternal", &[]).unwrap()).unwrap();
    assert!(expected > 0);
    assert!(external > 0);
    assert_eq!(
        as_size(q.call(probe, b"measuredCharge", &[]).unwrap()),
        Some(expected + external),
        "source parsing, Method construction and execution charge their actual allocations"
    );
    q.call(probe, b"clear", &[]).unwrap();
    q.call(gc, b"budget:", &[size(1 << 20).unwrap()]).unwrap();
    drop(q);
    h.reclaimed();
    // Read without Host argument snapshots or fresh Name allocation after collection.
    let collected = h.key(b"collectPrecisely");
    let charged = h.key(b"allocationBytes");
    unsafe { h.host.id.send(gc, collected, &[]) }.unwrap();
    assert_eq!(
        as_size(unsafe { h.host.id.send(gc, charged, &[]) }.unwrap()),
        Some(0)
    );
}
