//! ObjectModel ownership, layouts and construction through production Oop messages.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use std::{path::Path, ptr};
#[test]
#[ignore = "requires production Oop modules"]
fn ordinary_objects_private_behaviors_and_retirement_use_live_messages() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let call = |r, n: &[u8], a: &[Oop]| h.send(r, n, a).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let gc = h.named(b"GC").unwrap();
    let model = h.named(b"ObjectModel").unwrap();
    let prototype = h.named(b"Object").unwrap();
    let base = h.named(b"ObjectVt").unwrap();
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    assert_eq!(
        h.load_native(
            &q,
            &h.resources.lib_dir.join("libbirnam_object_model.dylib"),
            &[h.namespace]
        )
        .unwrap(),
        model
    );
    let retain = key(b"retain:");
    let release = key(b"release:");
    let pin = |v| unsafe { h.id.send(h.roots, retain, &[v]) }.unwrap();
    let unpin = |t| {
        unsafe { h.id.send(h.roots, release, &[t]) }.unwrap();
    };
    let collect = || {
        call(gc, b"collectPrecisely", &[]);
    };
    let vt = |v| call(h.id.root, b"behaviorOf:", &[v]);
    let private = |v| call(h.id.root, b"ensurePrivateVtableFor:", &[v]);
    let allocated = |b, n| call(model, b"allocate:bytes:", &[b, size(n).unwrap()]);
    let get = |v, i| call(model, b"slotAt:in:", &[size(i).unwrap(), v]);
    let put = |v, i, x| {
        call(model, b"slotAt:in:put:", &[size(i).unwrap(), v, x]);
    };
    let layout = |b, kinds: &[u8], rest| {
        call(
            model,
            b"setLayoutOf:to:rest:",
            &[b, q.byte_values(kinds).unwrap(), size(rest).unwrap()],
        );
    };
    let set = |v, n: &[u8], m| {
        call(v, b"addMethod", &[key(n), m]);
    };
    let path =
        std::env::temp_dir().join(format!("birnam-object-model-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/object-model.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = h.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let probe = q.call(prototype, b"clone", &[]).unwrap();
    let probe_vt = q
        .call(h.id.root, b"ensurePrivateVtableFor:", &[probe])
        .unwrap();
    let context = hooks.context(&[gc, model, base, probe]);
    let method = |n: &[u8], c| {
        q.call(
            q.call(fixture, b"methodAt:", &[key(n)]).unwrap(),
            b"withContext:",
            &[c],
        )
        .unwrap()
    };
    for n in [
        b"watch".as_slice(),
        b"allocation:bytes:identity:",
        b"mask",
        b"behaviorCount",
        b"binding:value:weak:",
        b"binding",
        b"watchFinal",
        b"finalize:",
        b"finalCount",
        b"fillRaw",
        b"checkRaw",
    ] {
        set(probe_vt, n, method(n, context));
    }
    let watch = |values: &[Oop]| {
        call(probe, b"watch", values);
    };
    let mask = || as_size(call(probe, b"mask", &[])).unwrap();
    assert!(call(h.id.root, b"ownerOf:", &[ptr::null_mut()]).is_null());
    let model_vt = vt(model);
    let retire = call(model_vt, b"lookup", &[key(b"retire:")]);
    set(model_vt, b"modelOriginalRetire:", retire);
    let retirement = hooks.context(&[key(b"modelOriginalRetire:"), size(0).unwrap()]);
    set(model_vt, b"retire:", method(b"forward", retirement));
    collect();
    assert_eq!(hooks.count(retirement, 1), 1);
    set(model_vt, b"retire:", retire);
    call(model_vt, b"removeMethod:", &[key(b"modelOriginalRetire:")]);
    assert_eq!(private(prototype), base);
    assert_eq!(private(base), base);
    for value in [
        h.named(b"true").unwrap(),
        h.named(b"nil").unwrap(),
        h.namespace,
    ] {
        assert_eq!(private(value), vt(value));
    }
    let original = call(prototype, b"clone", &[]);
    let original_pin = pin(original);
    let sibling = call(prototype, b"new", &[]);
    let sibling_pin = pin(sibling);
    assert_ne!(original, sibling);
    let personal = private(original);
    assert_eq!(private(original), personal);
    assert_eq!(call(h.id.root, b"parentOf:", &[personal]), base);
    assert_eq!(call(h.id.root, b"ownerOf:", &[personal]), original);
    assert_eq!(vt(sibling), base);
    let answer = key(b"objectModelAnswer");
    call(
        base,
        b"addMethod",
        &[answer, hooks.method(b"fixed", size(1).unwrap())],
    );
    call(
        personal,
        b"addMethod",
        &[answer, hooks.method(b"fixed", size(2).unwrap())],
    );
    assert_eq!(as_size(call(original, b"objectModelAnswer", &[])), Some(2));
    assert_eq!(as_size(call(sibling, b"objectModelAnswer", &[])), Some(1));
    let copy = call(original, b"clone", &[]);
    let copy_pin = pin(copy);
    assert_eq!(vt(copy), personal);
    let copy_behavior = private(copy);
    assert_ne!(copy_behavior, personal);
    assert_eq!(call(h.id.root, b"parentOf:", &[copy_behavior]), personal);
    mask();
    assert!(as_size(call(probe, b"behaviorCount", &[])).unwrap() > 0);
    call(personal, b"bindingsDo:", &[probe]);
    assert_eq!(call(probe, b"binding", &[]), answer);
    let root_vt = vt(h.id.root);
    let owner = call(root_vt, b"lookup", &[key(b"ownerOf:")]);
    set(root_vt, b"modelOriginalOwner:", owner);
    let counted = hooks.context(&[key(b"modelOriginalOwner:"), size(0).unwrap()]);
    set(root_vt, b"ownerOf:", method(b"forward", counted));
    assert_eq!(private(original), personal);
    assert!(hooks.count(counted, 1) > 0);
    set(root_vt, b"ownerOf:", owner);
    call(root_vt, b"removeMethod:", &[key(b"modelOriginalOwner:")]);
    watch(&[original, copy, personal, copy_behavior]);
    let behavior_pin = pin(personal);
    unpin(original_pin);
    unpin(copy_pin);
    collect();
    assert_eq!(mask(), 5);
    unpin(behavior_pin);
    collect();
    assert_eq!(mask(), 0);
    unpin(sibling_pin);
    call(base, b"removeMethod:", &[answer]);
    let b = call(base, b"delegated", &[]);
    let b_pin = pin(b);
    let kinds = q.byte_values(&[1, 9]).unwrap();
    call(
        model,
        b"setLayoutOf:to:rest:",
        &[b, kinds, size(1).unwrap()],
    );
    for i in 0..2 {
        call(kinds, b"at:put:", &[size(i).unwrap(), size(0).unwrap()]);
    }
    assert_eq!(
        as_size(call(model, b"slotKindOf:at:", &[b, size(1).unwrap()])),
        Some(9)
    );
    assert_eq!(
        as_size(call(
            model,
            b"slotKindOf:at:",
            &[b, q.word(usize::MAX).unwrap()]
        )),
        Some(1)
    );
    assert_eq!(
        as_size(call(
            model,
            b"allOopLayoutOf:words:",
            &[b, size(2).unwrap()]
        )),
        Some(0)
    );
    let cb = call(b, b"delegated", &[]);
    let cb_pin = pin(cb);
    assert_eq!(
        as_size(call(model, b"slotKindOf:at:", &[cb, size(0).unwrap()])),
        Some(0)
    );
    layout(b, &[], 1);
    let source = allocated(b, 8);
    let source_pin = pin(source);
    let child = call(prototype, b"clone", &[]);
    put(source, 0, child);
    let cloned = call(source, b"clone", &[]);
    let cloned_pin = pin(cloned);
    assert_ne!(source, cloned);
    assert_eq!(get(cloned, 0), child);
    watch(&[child]);
    put(source, 0, ptr::null_mut());
    collect();
    assert_eq!(mask(), 1);
    unpin(cloned_pin);
    collect();
    assert_eq!(mask(), 0);
    assert!(
        h.send(model, b"slotAt:in:", &[q.word(usize::MAX).unwrap(), source])
            .is_err()
    );
    layout(b, &[], 9);
    let raw = allocated(b, 9);
    let raw_pin = pin(raw);
    call(probe, b"fillRaw", &[raw]);
    let raw_copy = call(raw, b"clone", &[]);
    let raw_copy_pin = pin(raw_copy);
    assert_eq!(
        as_size(call(h.id.root, b"allocationSizeOf:", &[raw_copy])),
        Some(16)
    );
    assert_eq!(as_size(call(probe, b"checkRaw", &[raw_copy])), Some(1));
    let foreign = call(b, b"allocate", &[size(8).unwrap()]);
    assert!(h.send(foreign, b"clone", &[]).is_err());
    unpin(raw_pin);
    unpin(raw_copy_pin);
    layout(b, &[], 1);
    let snapshot_child = call(prototype, b"clone", &[]);
    put(source, 0, snapshot_child);
    let snapshot_identity = call(h.id.root, b"identityOf:", &[snapshot_child]);
    let meta = vt(b);
    let allocate = call(h.id.root, b"bind:to:", &[key(b"allocate"), b]);
    set(meta, b"modelOriginalAllocate:", allocate);
    let reentry = hooks.context(&[
        gc,
        model,
        source,
        key(b"modelOriginalAllocate:"),
        size(0).unwrap(),
    ]);
    set(meta, b"allocate", method(b"reenter", reentry));
    let saved = call(source, b"clone", &[]);
    let saved_pin = pin(saved);
    set(meta, b"allocate", allocate);
    call(meta, b"removeMethod:", &[key(b"modelOriginalAllocate:")]);
    assert_eq!(hooks.count(reentry, 4), 1);
    assert_eq!(
        call(h.id.root, b"identityOf:", &[snapshot_child]),
        snapshot_identity
    );
    assert_eq!(get(saved, 0), snapshot_child);
    assert!(get(source, 0).is_null());
    watch(&[snapshot_child]);
    hooks.put(reentry, 2, ptr::null_mut());
    unpin(saved_pin);
    unpin(source_pin);
    unpin(b_pin);
    unpin(cb_pin);
    collect();
    assert_eq!(mask(), 0);
    for _ in 0..24 {
        let value = call(prototype, b"clone", &[]);
        let behavior = private(value);
        call(probe, b"watchFinal", &[value, behavior]);
        call(gc, b"finalize:with:", &[value, probe]);
        let duplicate = call(value, b"clone", &[]);
        watch(&[value, duplicate, behavior]);
        collect();
        assert_eq!(mask(), 0);
    }
    assert_eq!(as_size(call(probe, b"finalCount", &[])), Some(24));
    call(gc, b"budget:", &[size(1).unwrap()]);
    let fresh = call(prototype, b"clone", &[]);
    let fresh_pin = pin(fresh);
    let behavior = private(fresh);
    assert_eq!(private(fresh), behavior);
    watch(&[fresh, behavior]);
    assert_eq!(mask(), 3);
    unpin(fresh_pin);
    call(gc, b"budget:", &[size(1 << 20).unwrap()]);
    collect();
}
