//! Batched current-Method facts decline custom lookup without executing it.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use std::ptr;
#[test]
#[ignore = "requires production Oop modules"]
fn facts_are_exact_and_decline_custom_lookup_without_calling_it() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let key = |n: &[u8]| hooks.symbol(n);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let vt = |r| call(h.id.root, b"behaviorOf:", &[r]);
    let bind = |v, s, m| {
        call(v, b"addMethod", &[s, m]);
    };
    let parent = call(h.named(b"ObjectVt").unwrap(), b"delegated", &[]);
    let child = call(parent, b"delegated", &[]);
    let first = call(parent, b"allocate", &[size(0).unwrap()]);
    let last = call(child, b"allocate", &[size(0).unwrap()]);
    let selector = key(b"bindingProofProbe");
    let lookup = key(b"lookup");
    let matches = key(b"nativeBindingsMatch:");
    let answer = hooks.method(b"fixed", size(9).unwrap());
    let different_context = hooks.method(b"fixed", size(10).unwrap());
    let different_code = hooks.method(b"fixedCount", size(9).unwrap());
    bind(parent, selector, answer);
    let query =
        |rows: &[Oop]| as_size(unsafe { h.id.send(h.id.root, matches, rows) }.unwrap()).unwrap();
    let row = [first, selector, answer];
    assert_eq!(query(&row), 1);
    assert_eq!(query(&[first, selector, different_code]), 0);
    assert_eq!(query(&[first, selector, different_context]), 0);
    assert_eq!(query(&[size(3).unwrap(), selector, answer]), 0);
    assert_eq!(query(&[first, size(3).unwrap(), answer]), 0);
    assert_eq!(query(&[first, selector, ptr::null_mut()]), 0);
    assert_eq!(query(&[first]), 0, "incomplete Oop rows decline the proof");
    let captured = call(answer, b"captured", &[]);
    assert_ne!(captured, answer);
    assert_eq!(
        query(&[first, selector, captured]),
        1,
        "capture preserves the selected implementation and context"
    );
    let mut rows = Vec::new();
    for receiver in [first, last].into_iter().cycle().take(32) {
        rows.extend([receiver, selector, answer]);
    }
    assert_eq!(query(&rows), 1);
    let mut wrong = rows.clone();
    wrong[95] = different_context;
    assert_eq!(query(&wrong), 0, "last row's Method context is checked");
    wrong[95] = answer;
    wrong[94] = size(3).unwrap();
    assert_eq!(
        query(&wrong),
        0,
        "last selector must name a current binding"
    );
    bind(child, selector, different_code);
    assert_eq!(
        query(&rows),
        0,
        "inherited selection observes a direct override"
    );
    call(child, b"removeMethod:", &[selector]);
    assert_eq!(query(&rows), 1);
    let calls = hooks.context(&[ptr::null_mut(), size(0).unwrap()]);
    let watched = hooks.method(b"fixedCount", calls);
    let meta = vt(child);
    let custom = call(meta, b"delegated", &[]);
    bind(custom, lookup, watched);
    call(h.id.root, b"reassociateBehaviorOf:with:", &[child, custom]);
    assert_eq!(
        query(&rows),
        0,
        "a later row with custom method lookup declines"
    );
    assert_eq!(hooks.count(calls, 1), 0);
    call(h.id.root, b"reassociateBehaviorOf:with:", &[child, meta]);
    assert_eq!(query(&rows), 1);

    // Keep captured operations on a separate receiver. Ordinary sends to it
    // can inspect/restore the root while the root's own lookup is replaced.
    let root_vt = vt(h.id.root);
    let root_meta = vt(root_vt);
    let matcher = call(root_vt, b"lookup", &[matches]);
    let reassociate = key(b"reassociateBehaviorOf:with:");
    let restore = call(root_vt, b"lookup", &[reassociate]);
    let probe = call(h.named(b"Object").unwrap(), b"clone", &[]);
    let probe_vt = call(h.id.root, b"ensurePrivateVtableFor:", &[probe]);
    let saved_query = key(b"bindingProofSavedQuery");
    let saved_restore = key(b"bindingProofSavedRestore");
    bind(probe_vt, saved_query, matcher);
    bind(probe_vt, saved_restore, restore);
    let direct = |arguments: &[Oop]| unsafe { h.id.send(probe, saved_query, arguments) };
    assert_eq!(as_size(direct(&rows).unwrap()), Some(1));
    call(
        h.id.root,
        b"reassociateBehaviorOf:with:",
        &[root_vt, custom],
    );
    let result = direct(&rows);
    // No Host snapshots or Name construction while root lookup is replaced.
    unsafe { h.id.send(probe, saved_restore, &[root_vt, root_meta]) }.unwrap();
    assert_eq!(as_size(result.unwrap()), Some(0));
    assert_eq!(hooks.count(calls, 1), 0);
    assert_eq!(query(&rows), 1, "restoration is observed by the next query");
    // A captured query also declines when the public proof operation is replaced.
    bind(root_vt, matches, watched);
    let result = direct(&rows);
    bind(root_vt, matches, matcher);
    assert_eq!(as_size(result.unwrap()), Some(0));
    assert_eq!(hooks.count(calls, 1), 0);
    assert_eq!(query(&rows), 1);
}
