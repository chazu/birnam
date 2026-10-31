//! Currently admitted Method data can reside in mutable ObjectModel slots.
//! This exercises that admission, not a language promise about Method layout.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi as abi;
#[test]
#[ignore = "requires production Id and ObjectModel"]
fn mutable_query_method_fields_are_rechecked_without_dictionary_edits() {
    let h = oop::bootstrap();
    let q = h.scope();
    let model = h.named(b"ObjectModel").unwrap();
    let behavior = h.named(b"ObjectVt").unwrap();
    q.call(
        h.named(b"GC").unwrap(),
        b"automaticCollection:",
        &[abi::size(0).unwrap()],
    )
    .unwrap();
    let query = q.keep(h.intern(b"allocationSizeOf:").unwrap()).unwrap();
    let match_selector = q.keep(h.intern(b"representationsMatch").unwrap()).unwrap();
    let add = q.keep(h.intern(b"addMethod").unwrap()).unwrap();
    let slot_put = q.keep(h.intern(b"slotAt:in:put:").unwrap()).unwrap();
    let original = q.call(h.id.root, b"bind:to:", &[query, h.id.root]).unwrap();
    let fake = q
        .call(
            model,
            b"allocate:bytes:",
            &[
                behavior,
                abi::size(3 * std::mem::size_of::<abi::Oop>()).unwrap(),
            ],
        )
        .unwrap();
    let put = |i, v| {
        q.call(model, b"slotAt:in:put:", &[abi::size(i).unwrap(), fake, v])
            .unwrap()
    };
    put(1, h.id.root);
    put(2, h.id.root);
    let index = (0..256)
        .find(|&i| {
            put(0, abi::size(i).unwrap());
            q.call(h.id.root, b"sameNativeMethod:as:", &[fake, original])
                .unwrap()
                == abi::size(1).unwrap()
        })
        .expect("an immediate first field matching the current extent Method");
    eprintln!("MUTABLE_QUERY_ADMITTED index={index}");
    let vt = q.call(h.id.root, b"behaviorOf:", &[h.id.root]).unwrap();
    q.call(vt, b"addMethod", &[query, fake]).unwrap();
    let left = q
        .call(
            model,
            b"allocate:bytes:",
            &[behavior, abi::size(8).unwrap()],
        )
        .unwrap();
    let batch = [behavior, abi::size(8).unwrap(), left, left];
    assert_eq!(
        q.call(h.id.root, b"representationsMatch", &batch).unwrap(),
        abi::size(1).unwrap()
    );
    put(1, left);
    let got = unsafe { h.id.send(h.id.root, match_selector, &batch) };
    // Restore the context through its owner, without editing the dictionary.
    unsafe { h.id.send(model, slot_put, &[abi::size(1).unwrap(), fake, h.id.root]) }.unwrap();
    let restored = unsafe { h.id.send(h.id.root, match_selector, &batch) };
    unsafe { h.id.send(vt, add, &[query, original]) }.unwrap();
    let got = got.unwrap();
    assert!(
        got.is_null(),
        "query must decline after mutable Method context changes; got {got:p}"
    );
    assert_eq!(restored.unwrap(), abi::size(1).unwrap());
    println!("MUTABLE_QUERY_CONTEXT_OBSERVED");
}
