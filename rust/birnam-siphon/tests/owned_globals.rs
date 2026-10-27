//! Owned binding roots, conditional slots and partial image application via Oops.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use std::{os::unix::ffi::OsStrExt, path::Path};

#[test]
#[ignore = "requires production Oop modules"]
fn conditional_slots_and_image_failure_preserve_binding_owners() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let named = |n: &[u8]| h.named(n).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let int = |n| call(named(b"Integer"), b"fromWord:", &[size(n).unwrap()]);
    let text = |b: &[u8]| q.string(b).unwrap();
    let vector = |values: &[Oop]| {
        let mut v = call(named(b"Vector"), b"new", &[]);
        for &value in values {
            v = call(v, b"add:", &[value]);
        }
        v
    };
    let module = h.global(&q, b"Module").unwrap();
    call(
        module,
        b"libDir:",
        &[text(h.resources.lib_dir.as_os_str().as_bytes())],
    );
    let store_path = text(
        h.resources
            .lib_dir
            .join("libbirnam_store.dylib")
            .as_os_str()
            .as_bytes(),
    );
    q.check_result(call(named(b"Codegen"), b"load:", &[store_path]))
        .unwrap();
    call(module, b"markLoaded:", &[key(b"store")]);
    let model = named(b"ObjectModel");
    let gc = named(b"GC");
    let globals = named(b"Globals");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let old = call(named(b"Object"), b"clone", &[]);
    let replacement = call(old, b"clone", &[]);
    let foreign = call(old, b"clone", &[]);
    let target = call(
        model,
        b"newSlots:parent:",
        &[vector(&[old]), named(b"ObjectVt")],
    );
    assert_eq!(
        call(
            model,
            b"replaceSlot:in:expected:with:",
            &[int(0), target, old, replacement]
        ),
        named(b"true")
    );
    assert_eq!(
        call(
            model,
            b"replaceSlot:in:expected:with:",
            &[int(0), target, old, foreign]
        ),
        named(b"false")
    );
    let library =
        std::env::temp_dir().join(format!("birnam-owned-globals-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/owned-globals.c")
            )
            .arg("-o")
            .arg(&library)
            .status()
            .unwrap()
            .success()
    );
    let fixture = h.load_native(&q, &library, &[]).unwrap();
    let declared = |n: &[u8]| call(fixture, b"methodAt:", &[key(n)]);
    let method = |n: &[u8], c| call(declared(n), b"withContext:", &[c]);
    let entry = |r, n: &[u8]| call(h.id.root, b"bind:to:", &[key(n), r]);
    let restore = |r, n: &[u8], m| {
        let vt = call(h.id.root, b"behaviorOf:", &[r]);
        call(vt, b"addMethod", &[key(n), m]);
    };
    let index = int(0);
    let original = entry(gc, b"prepareAllocation");
    let state = hooks.context(&[original, gc, model, target, foreign, size(1).unwrap()]);
    restore(gc, b"prepareAllocation", method(b"cas", state));
    assert_eq!(
        call(
            model,
            b"replaceSlot:in:expected:with:",
            &[index, target, replacement, old]
        ),
        named(b"false")
    );
    restore(gc, b"prepareAllocation", original);
    assert_eq!(call(model, b"slot:in:", &[int(0), target]), foreign);
    assert!(
        q.call(
            model,
            b"replaceSlot:in:expected:with:",
            &[int(0), text(b"foreign"), old, replacement]
        )
        .is_err()
    );
    assert!(
        q.call(
            model,
            b"replaceSlot:in:expected:with:",
            &[int(2), target, old, replacement]
        )
        .is_err()
    );

    let binding = |n: &[u8]| call(globals, b"at:", &[key(n)]);
    let put = |n: &[u8], v| assert_eq!(call(globals, b"at:put:", &[key(n), v]), v);
    let name_a = text(b"same name");
    let name_b = text(b"same name");
    assert_ne!(name_a, name_b);
    call(globals, b"at:put:", &[name_a, key(b"left")]);
    call(globals, b"at:put:", &[name_b, key(b"right")]);
    put(b"ImageNameA", name_a);
    put(b"ImageNameB", name_b);
    put(b"ImageOwnedMarker", key(b"saved"));
    put(b"ImageNil", named(b"nil"));
    put(b"ImageAliasA", old);
    put(b"ImageAliasB", old);
    put(b"ImageStorageAlias", call(globals, b"bindingOwner", &[]));
    for selector in [
        b"reserveBindingCompensationFor:count:".as_slice(),
        b"restoreBinding:expected:previous:reservation:",
        b"imageBindingCheckpoint",
        b"restoreImageBindings:",
        b"releaseImageReservations",
    ] {
        assert!(entry(globals, selector).is_null(), "obsolete Globals API");
    }
    let store = named(b"Store");
    call(
        store,
        b"assert:attr:value:",
        &[
            key(b"__globals__"),
            key(b"ImageOwnedMarker"),
            key(b"described"),
        ],
    );
    let path =
        std::env::temp_dir().join(format!("birnam-owned-globals-{}.image", std::process::id()));
    let filename = text(path.as_os_str().as_bytes());
    let system = named(b"System");
    assert_eq!(call(system, b"saveImage:", &[filename]), named(b"true"));
    put(b"ImageOwnedMarker", key(b"resident"));
    q.check_result(call(
        named(b"Codegen"),
        b"eval:",
        &[text(
            b"[Reflect defineMethod: #ownedImageTransient on: System as: [:self | #transient]]",
        )],
    ))
    .unwrap();
    let before = call(globals, b"bindingState", &[]);
    // The Id API receiver is not the vtable metatable. Intercept the actual
    // dictionary operation used for System's current vtable.
    let system_behavior = call(h.id.root, b"behaviorOf:", &[system]);
    let original = entry(system_behavior, b"removeMethod:");
    assert!(!original.is_null());
    let fault = hooks.fault(73);
    let failure = hooks.context(&[
        original,
        globals,
        key(b"ImageOwnedMarker"),
        key(b"callback"),
        key(b"ownedImageTransient"),
        size(1).unwrap(),
        fault,
    ]);
    restore(
        system_behavior,
        b"removeMethod:",
        method(b"removal", failure),
    );
    let error = unsafe { h.id.send(system, key(b"loadImage:"), &[filename]) }.unwrap_err();
    assert_eq!(error, fault);
    assert_eq!(hooks.count(failure, 5), 0);
    restore(system_behavior, b"removeMethod:", original);
    assert_eq!(binding(b"ImageOwnedMarker"), key(b"callback"));
    assert_ne!(
        call(globals, b"bindingState", &[]),
        before,
        "failed live load retains the callback's state"
    );
    let system_behavior = call(h.id.root, b"behaviorOf:", &[system]);
    assert_eq!(
        call(
            named(b"Execution"),
            b"directMethodFor:in:",
            &[key(b"ownedImageTransient"), system_behavior]
        ),
        named(b"nil")
    );
    assert_eq!(
        call(call(system, b"imageSaveReport", &[]), b"status", &[]),
        key(b"partialLoad")
    );
    for rebuild in [false, true] {
        q.check_result(call(
            system,
            b"loadImage:rebuild:",
            &[filename, named(if rebuild { b"true" } else { b"false" })],
        ))
        .unwrap();
        assert_eq!(binding(b"ImageOwnedMarker"), key(b"saved"));
        assert_eq!(binding(b"ImageAliasA"), binding(b"ImageAliasB"));
        assert_eq!(
            binding(b"ImageStorageAlias"),
            call(globals, b"bindingOwner", &[])
        );
        assert_eq!(
            call(globals, b"includes:", &[key(b"ImageNil")]),
            named(b"true")
        );
        assert_eq!(
            call(globals, b"at:", &[binding(b"ImageNameA")]),
            key(b"left")
        );
        assert_eq!(
            call(globals, b"at:", &[binding(b"ImageNameB")]),
            key(b"right")
        );
        assert_eq!(
            q.vector_size(call(globals, b"bindingState", &[])).unwrap(),
            4
        );
        assert_eq!(
            call(
                store,
                b"currentValue:attr:",
                &[key(b"__globals__"), key(b"ImageOwnedMarker")]
            ),
            key(b"described")
        );
    }
    let visitor_type = call(named(b"ObjectVt"), b"delegated", &[]);
    call(
        visitor_type,
        b"addMethod",
        &[
            key(b"allocation:bytes:identity:"),
            declared(b"allocation:bytes:identity:"),
        ],
    );
    let visitor = call(visitor_type, b"allocate", &[size(0).unwrap()]);
    let count = method(b"count", visitor);
    let empty = call(h.id.root, b"sequence", &[]);
    let remaining = || as_size(call(count, b"invokeOn:arguments:", &[fixture, empty])).unwrap();
    let object_key = {
        let local = h.scope();
        let object_key = local.call(named(b"Object"), b"clone", &[]).unwrap();
        let value = local.call(named(b"Object"), b"clone", &[]).unwrap();
        local
            .call(globals, b"at:put:", &[object_key, value])
            .unwrap();
        let args = local
            .call(h.id.root, b"sequence", &[object_key, value])
            .unwrap();
        local
            .call(declared(b"watch"), b"invokeOn:arguments:", &[fixture, args])
            .unwrap();
        object_key
    };
    h.send(gc, b"collectPrecisely", &[]).unwrap();
    assert_eq!(remaining(), 2);
    h.send(globals, b"remove:", &[object_key]).unwrap();
    h.send(gc, b"collectPrecisely", &[]).unwrap();
    assert_eq!(
        remaining(),
        0,
        "removed bindings must release both identities"
    );
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(library).unwrap();
}
