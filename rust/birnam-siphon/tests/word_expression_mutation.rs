//! Expression guards must observe mutable Method data, not only dictionary edits.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, size};

#[test]
#[ignore = "requires production Word, ObjectModel and Codegen"]
fn warmed_word_expression_rechecks_mutable_primitive_methods() {
    let h = oop::bootstrap();
    let q = h.scope();
    let named = |name: &[u8]| h.named(name).unwrap();
    let call = |r, name: &[u8], args: &[Oop]| q.call(r, name, args).unwrap();
    let key = |name: &[u8]| q.keep(h.intern(name).unwrap()).unwrap();
    let word = named(b"Word");
    let vt = named(b"WordVt");
    let model = named(b"ObjectModel");
    call(named(b"GC"), b"automaticCollection:", &[size(0).unwrap()]);
    let xor = key(b"bitXor:");
    let original = call(vt, b"lookup", &[xor]);
    // Obtain the Method's references through its owner protocol, without
    // reading another object's payload or requiring a new metadata accessor.
    let references = call(
        h.global(&q, b"Cell").unwrap(),
        b"with:",
        &[call(named(b"Vector"), b"new", &[])],
    );
    let factory = call(
        named(b"Codegen"),
        b"eval:",
        &[q.string(
            br#"[:references |
      let visitor := [Object clone];
      [Reflect defineMethod: #visit: on: visitor as: [:self :value |
        [references set: [[references value] add: value]]; self
      ]];
      visitor
    ]"#,
        )
        .unwrap()],
    );
    q.check_result(factory).unwrap();
    let visitor = call(factory, b"value:", &[references]);
    call(h.id.root, b"methodReferences", &[original, visitor]);
    let refs = call(references, b"value", &[]);
    let integer = named(b"Integer");
    let index = |i| call(integer, b"fromWord:", &[size(i).unwrap()]);
    let context = call(refs, b"at:", &[index(0)]);
    let owner = call(refs, b"at:", &[index(1)]);
    assert_eq!(context, word);
    // Exercise the currently admitted three-field representation. This is not
    // a language promise about Method layout or declaration ordinals.
    let mutable = call(
        model,
        b"allocate:bytes:",
        &[named(b"ObjectVt"), size(3 * size_of::<Oop>()).unwrap()],
    );
    let put = |i, value| {
        call(
            model,
            b"slotAt:in:put:",
            &[size(i).unwrap(), mutable, value],
        )
    };
    put(1, context);
    put(2, owner);
    (0..256)
        .find(|&i| {
            put(0, size(i).unwrap());
            call(h.id.root, b"sameNativeMethod:as:", &[mutable, original]) == size(1).unwrap()
        })
        .expect("mutable slots representing the current XOR Method");
    let a = call(word, b"fromBits:", &[size(10).unwrap()]);
    let b = call(word, b"fromBits:", &[size(6).unwrap()]);
    let program = call(
        named(b"Bytes"),
        b"fromBytes:",
        &[q.byte_values(&[1, 2, 1, 2, 13, 0, 1]).unwrap()],
    );
    let expression = key(b"tryPrimitiveExpression:");
    let epoch_expression = key(b"tryPrimitiveExpressionAtEpoch:");
    let epoch = key(b"behaviorEpoch");
    let add = key(b"addMethod");
    let raw = |r, selector, args: &[Oop]| unsafe { h.id.send(r, selector, args) };
    raw(vt, add, &[xor, mutable]).unwrap();
    let stamp = raw(h.id.root, epoch, &[]).unwrap();
    let args = [program, a, b];
    for _ in 0..2 {
        let result = q.keep(raw(word, expression, &args).unwrap()).unwrap();
        assert_eq!(q.word_bits(result).unwrap(), 12);
    }
    put(1, a);
    assert_eq!(
        raw(h.id.root, epoch, &[]).unwrap(),
        stamp,
        "this probe must exercise a data edit without a dictionary edit"
    );
    assert!(
        raw(a, xor, &[b]).is_err(),
        "the changed primitive is no longer the admitted implementation"
    );
    let changed = raw(word, expression, &args).unwrap();
    let stamped = raw(word, epoch_expression, &[stamp, program, a, b]).unwrap();
    put(1, context);
    let restored = q.keep(raw(word, expression, &args).unwrap()).unwrap();
    raw(vt, add, &[xor, original]).unwrap();
    assert!(
        changed.is_null(),
        "a warmed expression must decline after the primitive's context changes"
    );
    assert!(
        stamped.is_null(),
        "an unchanged epoch is not proof of unchanged Method data"
    );
    assert_eq!(q.word_bits(restored).unwrap(), 12);

    // A different operation's admission callback can change a previously
    // checked Method. A per-call bitset would still be stale on the final XOR.
    let path =
        std::env::temp_dir().join(format!("birnam-word-mutation-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/word-expression.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let module = h.load_native(&q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let guard_context = call(h.id.root, b"sequence", &[model, mutable, a]);
    let guard = call(
        call(module, b"methodAt:", &[key(b"mutableMatch")]),
        b"withContext:",
        &[guard_context],
    );
    let guard_key = key(b"nativeBindingsMatch:");
    let guard_original = call(h.id.root, b"bind:to:", &[guard_key, h.id.root]);
    let root_vt = call(h.id.root, b"behaviorOf:", &[h.id.root]);
    let mixed = call(
        named(b"Bytes"),
        b"fromBytes:",
        &[q.byte_values(&[1, 2, 3, 4, 13, 0, 1, 6, 2, 0, 13, 3, 1])
            .unwrap()],
    );
    raw(vt, add, &[xor, mutable]).unwrap();
    raw(root_vt, add, &[guard_key, guard]).unwrap();
    let stamp = raw(h.id.root, epoch, &[]).unwrap();
    let result = raw(word, expression, &[mixed, a, b]);
    let after = raw(h.id.root, epoch, &[]).unwrap();
    raw(root_vt, add, &[guard_key, guard_original]).unwrap();
    raw(vt, add, &[xor, original]).unwrap();
    assert_eq!(after, stamp);
    assert_eq!(
        call(model, b"slotAt:in:", &[size(1).unwrap(), mutable]),
        a,
        "the admission callback must actually have changed the earlier Method"
    );
    assert!(
        result.unwrap().is_null(),
        "a Method proof must not survive an intervening admission callback"
    );
    // Allocation permission must also remain current after guard callbacks.
    // A comparison result makes no output Word, exposing omitted constructor
    // effects rather than relying on a final allocation to reveal the change.
    let model_vt = call(h.id.root, b"behaviorOf:", &[model]);
    let allocate = key(b"allocate:bytes:");
    let allocator_original = call(model_vt, b"lookup", &[allocate]);
    call(references, b"set:", &[call(named(b"Vector"), b"new", &[])]);
    call(
        h.id.root,
        b"methodReferences",
        &[allocator_original, visitor],
    );
    let allocator_refs = call(references, b"value", &[]);
    let allocator_context = call(allocator_refs, b"at:", &[index(0)]);
    let allocator_owner = call(allocator_refs, b"at:", &[index(1)]);
    let allocator = call(
        model,
        b"allocate:bytes:",
        &[named(b"ObjectVt"), size(3 * size_of::<Oop>()).unwrap()],
    );
    let edit = |i, value| {
        call(
            model,
            b"slotAt:in:put:",
            &[size(i).unwrap(), allocator, value],
        )
    };
    edit(1, allocator_context);
    edit(2, allocator_owner);
    let original_index = (0..256)
        .find(|&i| {
            edit(0, size(i).unwrap());
            call(
                h.id.root,
                b"sameNativeMethod:as:",
                &[allocator, allocator_original],
            ) == size(1).unwrap()
        })
        .unwrap();
    let allocation_state = call(
        h.id.root,
        b"sequence",
        &[
            vt,
            named(b"GC"),
            size(1).unwrap(),
            size(0).unwrap(),
            original,
            size(0).unwrap(),
            size(0).unwrap(),
        ],
    );
    let observing = call(
        call(module, b"methodAt:", &[key(b"allocation")]),
        b"withContext:",
        &[allocation_state],
    );
    call(references, b"set:", &[call(named(b"Vector"), b"new", &[])]);
    call(h.id.root, b"methodReferences", &[observing, visitor]);
    let observing_refs = call(references, b"value", &[]);
    let observing_owner = call(observing_refs, b"at:", &[index(1)]);
    edit(1, allocation_state);
    edit(2, observing_owner);
    let observing_index = (0..256)
        .find(|&i| {
            edit(0, size(i).unwrap());
            call(h.id.root, b"sameNativeMethod:as:", &[allocator, observing]) == size(1).unwrap()
        })
        .unwrap();
    edit(0, size(original_index).unwrap());
    edit(1, allocator_context);
    edit(2, allocator_owner);
    let allocation_guard_context = call(
        h.id.root,
        b"sequence",
        &[
            model,
            allocator,
            size(observing_index).unwrap(),
            allocation_state,
            observing_owner,
        ],
    );
    let allocation_guard = call(
        call(module, b"methodAt:", &[key(b"allocatorMatch")]),
        b"withContext:",
        &[allocation_guard_context],
    );
    let alias = key(b"expressionOriginalAllocation:bytes:");
    raw(model_vt, add, &[alias, allocator_original]).unwrap();
    let comparison = call(
        named(b"Bytes"),
        b"fromBytes:",
        &[q.byte_values(&[1, 2, 2, 3, 6, 0, 1, 16, 2, 1]).unwrap()],
    );
    let permit = key(b"canOmitAllocationFor:");
    raw(model_vt, add, &[allocate, allocator]).unwrap();
    raw(root_vt, add, &[guard_key, allocation_guard]).unwrap();
    assert_eq!(raw(model, permit, &[vt]).unwrap(), size(1).unwrap());
    let stamp = raw(h.id.root, epoch, &[]).unwrap();
    let result = raw(word, expression, &[comparison, a, b]);
    let after = raw(h.id.root, epoch, &[]).unwrap();
    let allowed = raw(model, permit, &[vt]);
    raw(root_vt, add, &[guard_key, guard_original]).unwrap();
    let fallback = q.keep(raw(a, key(b"wrappingAdd:"), &[b]).unwrap()).unwrap();
    let effects = call(allocation_state, b"at:", &[size(6).unwrap()]);
    raw(model_vt, add, &[allocate, allocator_original]).unwrap();
    assert_eq!(after, stamp);
    assert_eq!(
        allowed.unwrap(),
        size(0).unwrap(),
        "the callback must revoke allocation omission"
    );
    assert!(
        result.unwrap().is_null(),
        "allocation omission must be rechecked after admission callbacks"
    );
    assert_eq!(q.word_bits(fallback).unwrap(), 16);
    assert_ne!(
        effects,
        size(0).unwrap(),
        "ordinary fallback must observe the changed constructor"
    );
}
