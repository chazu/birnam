//! Owned slot descriptions, live layout changes and collection during queries.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use birnam_host_support::{Context, Scope};
use std::path::Path;

fn indices(h: &Context, q: &Scope<'_>, vector: Oop) -> Vec<usize> {
    (0..q.vector_size(vector).unwrap())
        .map(|i| {
            let value = q.vector_at(vector, i).unwrap();
            q.word_bits(
                q.call(h.named(b"Integer").unwrap(), b"wordOf:", &[value])
                    .unwrap(),
            )
            .unwrap()
        })
        .collect()
}

#[test]
#[ignore = "requires production Oop modules"]
fn reference_inventory_uses_owned_metadata_and_roots_its_snapshot() {
    let h = oop::bootstrap();
    let held = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let model = h.named(b"ObjectModel").unwrap();
    let roots = h.named(b"Roots").unwrap();
    let gc = h.named(b"GC").unwrap();
    let integer = h.named(b"Integer").unwrap();
    let nil = h.named(b"nil").unwrap();
    held.call(gc, b"precisePayloads:", &[size(1).unwrap()])
        .unwrap();
    let base = h.named(b"ObjectVt").unwrap();
    let behavior = held.call(base, b"delegated", &[]).unwrap();
    let layout = held
        .call(
            h.id.root,
            b"sequence",
            &[size(1).unwrap(), size(0).unwrap(), size(1).unwrap()],
        )
        .unwrap();
    let zero_layout = held
        .call(h.id.root, b"sequence", &[size(0).unwrap(); 3])
        .unwrap();
    h.send(
        model,
        b"setLayoutOf:to:rest:",
        &[behavior, layout, size(0).unwrap()],
    )
    .unwrap();
    // Explicit tokens prevent the test's result scope from accidentally
    // retaining objects whose lifetime the query must protect.
    let target = h
        .send(
            model,
            b"allocate:bytes:",
            &[behavior, size(3 * size_of::<Oop>()).unwrap()],
        )
        .unwrap();
    let target_token = h.send(roots, b"retain:", &[target]).unwrap();
    let inventory = held.call(model, b"referenceSlotsOf:", &[target]).unwrap();
    assert_eq!(indices(&h, &held, inventory), vec![0, 2]);
    let private = h
        .send(h.id.root, b"ensurePrivateVtableFor:", &[target])
        .unwrap();
    let private_token = h.send(roots, b"retain:", &[private]).unwrap();
    assert_eq!(
        indices(
            &h,
            &held,
            held.call(model, b"referenceSlotsOf:", &[target]).unwrap()
        ),
        vec![0, 2]
    );
    for foreign in [held.string(b"opaque").unwrap(), private] {
        assert_eq!(
            held.call(model, b"referenceSlotsOf:", &[foreign]).unwrap(),
            nil,
            "foreign storage does not acquire an ObjectModel description"
        );
    }
    let zero = held
        .call(integer, b"fromWord:", &[size(0).unwrap()])
        .unwrap();
    assert!(
        indices(
            &h,
            &held,
            held.call(model, b"referenceSlotsOf:", &[zero]).unwrap()
        )
        .is_empty(),
        "ObjectModel-allocated numeric storage contains no reference slots"
    );
    let empty = held
        .call(h.named(b"Object").unwrap(), b"clone", &[])
        .unwrap();
    assert!(
        indices(
            &h,
            &held,
            held.call(model, b"referenceSlotsOf:", &[empty]).unwrap()
        )
        .is_empty()
    );
    let untyped = held
        .call(
            model,
            b"allocate:bytes:",
            &[base, size(size_of::<Oop>()).unwrap()],
        )
        .unwrap();
    assert_eq!(
        held.call(model, b"referenceSlotsOf:", &[untyped]).unwrap(),
        nil
    );
    let model_vt = held.call(h.id.root, b"behaviorOf:", &[model]).unwrap();
    let raw = hooks.symbol(b"slotAt:in:");
    let read = held.call(model_vt, b"lookup", &[raw]).unwrap();
    let deny_state = hooks.context(&[hooks.fault(4), size(0).unwrap()]);
    h.send(
        model_vt,
        b"addMethod",
        &[raw, hooks.method(b"deny", deny_state)],
    )
    .unwrap();
    assert_eq!(
        indices(
            &h,
            &held,
            held.call(model, b"referenceSlotsOf:", &[target]).unwrap()
        ),
        vec![0, 2]
    );
    assert_eq!(
        hooks.count(deny_state, 1),
        0,
        "describing storage does not read its values"
    );
    h.send(model_vt, b"addMethod", &[raw, read]).unwrap();

    let path = std::env::temp_dir().join(format!(
        "birnam-reference-slots-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/reference-slots.c")
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
    let query = hooks.symbol(b"referenceSlotsOf:");
    let retain = hooks.symbol(b"retain:");
    let release = hooks.symbol(b"release:");
    let add = hooks.symbol(b"addMethod");
    let reassociate = hooks.symbol(b"reassociateBehaviorOf:with:");
    let parent_sel = hooks.symbol(b"parentOf:");
    let root_vt = held.call(h.id.root, b"behaviorOf:", &[h.id.root]).unwrap();
    let parent = held.call(root_vt, b"lookup", &[parent_sel]).unwrap();
    // Collector metadata callbacks cannot allocate an argument Sequence.
    // Forward through an explicit alias of the captured original Method.
    let parent_alias = hooks.symbol(b"referenceParentOriginal:");
    h.send(root_vt, b"addMethod", &[parent_alias, parent])
        .unwrap();
    let integer_vt = held.call(h.id.root, b"behaviorOf:", &[integer]).unwrap();
    let from_word = hooks.symbol(b"fromWord:");
    let original = held.call(integer_vt, b"lookup", &[from_word]).unwrap();
    let visitor_vt = held.call(base, b"delegated", &[]).unwrap();
    held.call(
        visitor_vt,
        b"addMethod",
        &[
            hooks.symbol(b"allocation:bytes:identity:"),
            declared(b"allocation:bytes:identity:"),
        ],
    )
    .unwrap();
    let visitor = held
        .call(visitor_vt, b"allocate", &[size(0).unwrap()])
        .unwrap();
    let fault = hooks.fault(3);
    // Configuration is weak C state. It cannot keep the observed objects alive.
    let configuration = h
        .send(
            h.id.root,
            b"sequence",
            &[
                model,
                gc,
                roots,
                target,
                private,
                base,
                original,
                zero_layout,
                fault,
                visitor,
            ],
        )
        .unwrap();
    h.send(
        declared(b"configure"),
        b"invokeOn:arguments:",
        &[module, configuration],
    )
    .unwrap();
    let empty_args = held.call(h.id.root, b"sequence", &[]).unwrap();
    let stats_method = declared(b"stats");
    let stats = || {
        let q = h.scope();
        let values = q
            .call(stats_method, b"invokeOn:arguments:", &[module, empty_args])
            .unwrap();
        (0..5)
            .map(|i| q.call(values, b"at:", &[size(i).unwrap()]).unwrap())
            .collect::<Vec<_>>()
    };
    let parent_hook = declared(b"parent");
    let convert_hook = declared(b"convert");
    unsafe { h.id.send(root_vt, add, &[parent_sel, parent_hook]) }.unwrap();
    unsafe { h.id.send(roots, release, &[private_token]) }.unwrap();
    let observed = held
        .keep(unsafe { h.id.send(model, query, &[target]) }.unwrap())
        .unwrap();
    unsafe { h.id.send(root_vt, add, &[parent_sel, parent]) }.unwrap();
    h.send(root_vt, b"removeMethod:", &[parent_alias]).unwrap();
    let state = stats();
    assert_eq!(as_size(state[0]), Some(1));
    let private_token = state[1];
    assert!(as_size(private_token).unwrap() > 0);
    assert_eq!(indices(&h, &held, observed), vec![0, 2]);
    unsafe { h.id.send(h.id.root, reassociate, &[target, private]) }.unwrap();
    unsafe { h.id.send(integer_vt, add, &[from_word, convert_hook]) }.unwrap();
    unsafe { h.id.send(roots, release, &[target_token]) }.unwrap();
    unsafe { h.id.send(roots, release, &[private_token]) }.unwrap();
    let snapshot = held
        .keep(unsafe { h.id.send(model, query, &[target]) }.unwrap())
        .unwrap();
    let target_again = unsafe { h.id.send(roots, retain, &[target]) }.unwrap();
    unsafe { h.id.send(integer_vt, add, &[from_word, original]) }.unwrap();
    let state = stats();
    assert_eq!(as_size(state[2]), Some(2));
    assert_eq!(as_size(state[4]), Some(1));
    assert_eq!(indices(&h, &held, snapshot), vec![0, 2]);
    assert!(
        indices(
            &h,
            &held,
            held.call(model, b"referenceSlotsOf:", &[target]).unwrap()
        )
        .is_empty()
    );
    assert_eq!(
        indices(&h, &held, inventory),
        vec![0, 2],
        "old inventory remains a snapshot"
    );
    h.send(
        model,
        b"setLayoutOf:to:rest:",
        &[private, layout, size(0).unwrap()],
    )
    .unwrap();
    let failing_args = held
        .call(h.id.root, b"sequence", &[size(1).unwrap()])
        .unwrap();
    held.call(
        declared(b"mode"),
        b"invokeOn:arguments:",
        &[module, failing_args],
    )
    .unwrap();
    unsafe { h.id.send(integer_vt, add, &[from_word, convert_hook]) }.unwrap();
    unsafe { h.id.send(roots, release, &[target_again]) }.unwrap();
    let error = unsafe { h.id.send(model, query, &[target]) }.unwrap_err();
    assert_eq!(error, fault);
    unsafe { h.id.send(integer_vt, add, &[from_word, original]) }.unwrap();
    h.send(gc, b"collectPrecisely", &[]).unwrap();
    assert_eq!(
        as_size(stats()[3]),
        Some(0),
        "queries and failure must not retain target or private vtable"
    );
    assert_eq!(indices(&h, &held, snapshot), vec![0, 2]);
    assert!(unsafe { h.id.send(model, query, &[]) }.is_err());
    assert!(unsafe { h.id.send(model, query, &[std::ptr::null_mut()]) }.is_err());

    // A live parent callback can report a cycle. Stop before revisiting its
    // layout, while a directly described object needs no parent callback.
    let undescribed = held.call(base, b"delegated", &[]).unwrap();
    let allocate = |b| held.call(model, b"allocate:bytes:",
        &[b, size(size_of::<Oop>()).unwrap()]).unwrap();
    let cyclic = allocate(undescribed);
    let direct = allocate(behavior);
    let cycle = hooks.context(&[undescribed, size(0).unwrap()]);
    let parent_hook = hooks.method(b"fixedCount", cycle);
    h.send(root_vt, b"addMethod", &[parent_sel, parent_hook]).unwrap();
    assert!(held.call(model, b"slotAt:in:", &[size(0).unwrap(), cyclic]).unwrap().is_null());
    assert_eq!(hooks.count(cycle, 1), 1, "a reported cycle must terminate");
    assert!(held.call(model, b"slotAt:in:", &[size(0).unwrap(), direct]).unwrap().is_null());
    assert_eq!(hooks.count(cycle, 1), 1, "a direct layout must not query its parent");
    h.send(root_vt, b"addMethod", &[parent_sel, parent]).unwrap();
}
