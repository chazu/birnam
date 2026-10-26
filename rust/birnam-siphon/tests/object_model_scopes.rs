//! Slot adapters preserve live dispatch and exact nested owner roots.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use std::path::Path;

#[test]
#[ignore = "requires production Oop modules"]
fn boxed_slots_use_current_conversion_and_raw_access_with_scoped_roots() {
    let h = oop::bootstrap();
    let held = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let model = h.named(b"ObjectModel").unwrap();
    let gc = h.named(b"GC").unwrap();
    let roots = h.named(b"Roots").unwrap();
    let original_integer = held.keep(h.named(b"Integer").unwrap()).unwrap();
    let prototype = h.named(b"Object").unwrap();
    let base = h.named(b"ObjectVt").unwrap();
    held.call(gc, b"precisePayloads:", &[size(1).unwrap()])
        .unwrap();
    let behavior = held.call(base, b"delegated", &[]).unwrap();
    let kinds = held
        .call(h.id.root, b"sequence", &[size(1).unwrap()])
        .unwrap();
    h.send(
        model,
        b"setLayoutOf:to:rest:",
        &[behavior, kinds, size(0).unwrap()],
    )
    .unwrap();
    let target = h
        .send(
            model,
            b"allocate:bytes:",
            &[behavior, size(size_of::<Oop>()).unwrap()],
        )
        .unwrap();
    let target_token = h.send(roots, b"retain:", &[target]).unwrap();
    let edge = h.send(prototype, b"clone", &[]).unwrap();
    let edge_token = h.send(roots, b"retain:", &[edge]).unwrap();
    let index = h.send(prototype, b"clone", &[]).unwrap();
    let index_token = h.send(roots, b"retain:", &[index]).unwrap();
    h.send(model, b"slotAt:in:put:", &[size(0).unwrap(), target, edge])
        .unwrap();
    let integer = held.call(prototype, b"clone", &[]).unwrap();
    let visitor = held.call(prototype, b"clone", &[]).unwrap();
    let path =
        std::env::temp_dir().join(format!("birnam-slot-scopes-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/slot-scopes.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let module = h.load_native(&held, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let declared = |name: &[u8]| {
        held.call(module, b"methodAt:", &[hooks.symbol(name)])
            .unwrap()
    };
    let visitor_vt = held
        .call(h.id.root, b"ensurePrivateVtableFor:", &[visitor])
        .unwrap();
    for name in [b"visit:".as_slice(), b"allocation:bytes:identity:"] {
        held.call(
            visitor_vt,
            b"addMethod",
            &[hooks.symbol(name), declared(name)],
        )
        .unwrap();
    }
    let integer_vt = held
        .call(h.id.root, b"ensurePrivateVtableFor:", &[integer])
        .unwrap();
    held.call(
        integer_vt,
        b"addMethod",
        &[hooks.symbol(b"wordOf:"), declared(b"convert")],
    )
    .unwrap();
    let fault = hooks.fault(7);
    let configure_args = h
        .send(
            h.id.root,
            b"sequence",
            &[model, gc, visitor, target, edge, index, fault],
        )
        .unwrap();
    h.send(
        declared(b"configure"),
        b"invokeOn:arguments:",
        &[module, configure_args],
    )
    .unwrap();
    let integer_name = hooks.symbol(b"Integer");
    let getter = hooks.symbol(b"slot:in:");
    let setter = hooks.symbol(b"slot:in:put:");
    let release = hooks.symbol(b"release:");
    let capture = held
        .call(h.id.root, b"sequence", &[size(0).unwrap()])
        .unwrap();
    let compare = held
        .call(h.id.root, b"sequence", &[size(1).unwrap()])
        .unwrap();
    let no_args = held.call(h.id.root, b"sequence", &[]).unwrap();
    let checkpoint = declared(b"checkpoint");
    let mode = declared(b"mode");
    let stats = declared(b"stats");
    held.call(h.namespace, b"at:put:", &[integer_name, integer])
        .unwrap();
    for token in [target_token, edge_token, index_token] {
        unsafe { h.id.send(roots, release, &[token]) }.unwrap();
    }
    held.call(checkpoint, b"invokeOn:arguments:", &[module, capture])
        .unwrap();
    // Raw sends intentionally do not retain the returned target or edge.
    assert_eq!(
        unsafe { h.id.send(model, setter, &[index, target, edge]) }.unwrap(),
        target
    );
    held.call(checkpoint, b"invokeOn:arguments:", &[module, compare])
        .unwrap();
    assert_eq!(
        unsafe { h.id.send(model, getter, &[index, target]) }.unwrap(),
        edge
    );
    held.call(checkpoint, b"invokeOn:arguments:", &[module, compare])
        .unwrap();
    let values = held
        .call(stats, b"invokeOn:arguments:", &[module, no_args])
        .unwrap();
    assert_eq!(
        as_size(held.call(values, b"at:", &[size(0).unwrap()]).unwrap()),
        Some(4)
    );
    held.call(mode, b"invokeOn:arguments:", &[module, compare])
        .unwrap();
    assert_eq!(
        unsafe { h.id.send(model, setter, &[index, target, edge]) }.unwrap_err(),
        fault
    );
    held.call(checkpoint, b"invokeOn:arguments:", &[module, compare])
        .unwrap();
    held.call(h.namespace, b"at:put:", &[integer_name, original_integer])
        .unwrap();
    let model_vt = held.call(h.id.root, b"behaviorOf:", &[model]).unwrap();
    let raw = hooks.symbol(b"slotAt:in:");
    let original_raw = held.call(model_vt, b"lookup", &[raw]).unwrap();
    let raw_fault = hooks.fault(8);
    let deny = hooks.context(&[raw_fault, size(0).unwrap()]);
    held.call(model_vt, b"addMethod", &[raw, hooks.method(b"deny", deny)])
        .unwrap();
    let zero = held
        .call(original_integer, b"fromWord:", &[size(0).unwrap()])
        .unwrap();
    assert_eq!(
        unsafe { h.id.send(model, getter, &[zero, target]) }.unwrap_err(),
        raw_fault
    );
    assert_eq!(hooks.count(deny, 1), 1);
    held.call(checkpoint, b"invokeOn:arguments:", &[module, compare])
        .unwrap();
    held.call(model_vt, b"addMethod", &[raw, original_raw])
        .unwrap();
    h.send(gc, b"collectPrecisely", &[]).unwrap();
    let values = held
        .call(stats, b"invokeOn:arguments:", &[module, no_args])
        .unwrap();
    assert_eq!(
        as_size(held.call(values, b"at:", &[size(1).unwrap()]).unwrap()),
        Some(0),
        "exited scopes must not retain the target, edge or index"
    );
    assert!(unsafe { h.id.send(model, getter, &[]) }.is_err());
    assert!(
        unsafe {
            h.id.send(
                model,
                setter,
                &[zero, std::ptr::null_mut(), std::ptr::null_mut()],
            )
        }
        .is_err()
    );
}
