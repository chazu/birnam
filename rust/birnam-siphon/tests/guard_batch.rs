//! Current Method facts guard allocation omission without performing allocation.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
#[test]
#[ignore = "requires production Oop allocation owners"]
fn batch_declines_changed_dependencies_and_preserves_guard_errors() {
    let host = oop::bootstrap();
    let q = host.scope();
    let hooks = oop::hooks::Hooks::new(&host);
    let named = |n: &[u8]| host.named(n).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let model = named(b"ObjectModel");
    let gc = named(b"GC");
    let word = named(b"Word");
    let word_vt = named(b"WordVt");
    let root = host.id.root;
    let root_vt = call(root, b"behaviorOf:", &[root]);
    let model_vt = call(root, b"behaviorOf:", &[model]);
    let gc_vt = call(root, b"behaviorOf:", &[gc]);
    let allocation_vt = call(root, b"behaviorOf:", &[word_vt]);
    let matches = key(b"nativeBindingsMatch:");
    let can_omit = key(b"canOmitAllocationFor:");
    let alloc = key(b"allocate:bytes:");
    let add = key(b"addMethod");
    let reassociate = key(b"reassociateBehaviorOf:with:");
    let default_alloc = key(b"usesDefaultAllocation");
    let raw = |r, selector, args: &[Oop]| unsafe { host.id.send(r, selector, args) };
    let set = |vt, selector, m| {
        raw(vt, add, &[selector, m]).unwrap();
    };
    let bind = |r, selector| call(root, b"bind:to:", &[selector, r]);
    let original_alloc = bind(model, alloc);
    let matcher = bind(root, matches);
    let allocation_policy = bind(root, default_alloc);
    let query = |rows: &[Oop]| as_size(raw(root, matches, rows).unwrap()).unwrap();
    let check = || raw(model, can_omit, &[word_vt]);
    let row = [model, alloc, original_alloc];
    assert_eq!(query(&row), 1);
    let wrong_context = call(original_alloc, b"withContext:", &[std::ptr::null_mut()]);
    assert_eq!(
        query(&[model, alloc, wrong_context]),
        0,
        "context participates in the Method fact"
    );
    assert_eq!(as_size(check().unwrap()), Some(1));
    for (receiver, vt, selector) in [
        (model, model_vt, alloc),
        (word_vt, allocation_vt, key(b"allocate")),
        (gc, gc_vt, key(b"prepareAllocation")),
    ] {
        let original = bind(receiver, selector);
        let state = hooks.context(&[original, size(0).unwrap()]);
        let replacement = hooks.method(b"counted", state);
        set(vt, selector, replacement);
        assert_eq!(as_size(check().unwrap()), Some(0));
        assert_eq!(
            hooks.count(state, 1),
            0,
            "permission must not execute the changed allocation operation"
        );
        set(vt, selector, original);
        assert_eq!(as_size(check().unwrap()), Some(1));
    }
    let failure = unsafe {
        host.id.send(
            named(b"Execution"),
            key(b"failNative:detail:"),
            &[size(1901).unwrap(), size(73).unwrap()],
        )
    }
    .unwrap_err();
    q.keep(failure).unwrap();
    let denied_state = hooks.context(&[failure, size(0).unwrap()]);
    let deny = hooks.method(b"deny", denied_state);
    // These are the current permission dependencies, rather than the old
    // native-entry discovery helper's cached lookup/bind services.
    for (selector, original) in [(matches, matcher), (default_alloc, allocation_policy)] {
        hooks.put(denied_state, 1, size(0).unwrap());
        set(root_vt, selector, deny);
        let result = check();
        set(root_vt, selector, original);
        assert_eq!(result.unwrap_err(), failure);
        assert_eq!(hooks.count(denied_state, 1), 1);
    }
    let forwarding = hooks.context(&[matcher, size(0).unwrap()]);
    let forwarded = hooks.method(b"counted", forwarding);
    set(root_vt, matches, forwarded);
    let result = check();
    set(root_vt, matches, matcher);
    assert_eq!(
        as_size(result.unwrap()),
        Some(0),
        "a forwarding replacement cannot make the captured matcher approve its own replacement"
    );
    assert_eq!(hooks.count(forwarding, 1), 1);
    assert_eq!(as_size(check().unwrap()), Some(1));
    let forwarding = hooks.context(&[allocation_policy, size(0).unwrap()]);
    let forwarded = hooks.method(b"counted", forwarding);
    set(root_vt, default_alloc, forwarded);
    let result = check();
    set(root_vt, default_alloc, allocation_policy);
    assert_eq!(as_size(result.unwrap()), Some(0));
    assert_eq!(hooks.count(forwarding, 1), 1);

    // Enter a captured permission Method through an ordinary alias on another
    // receiver while the allocation owner's method lookup is noncanonical.
    let permission = bind(model, can_omit);
    let probe = call(named(b"Object"), b"clone", &[]);
    let probe_vt = call(root, b"ensurePrivateVtableFor:", &[probe]);
    let saved = key(b"savedAllocationPermission:");
    set(probe_vt, saved, permission);
    let meta = call(root, b"behaviorOf:", &[model_vt]);
    let custom = call(meta, b"delegated", &[]);
    set(custom, key(b"lookup"), deny);
    hooks.put(denied_state, 1, size(0).unwrap());
    raw(root, reassociate, &[model_vt, custom]).unwrap();
    let result = raw(probe, saved, &[word_vt]);
    raw(root, reassociate, &[model_vt, meta]).unwrap();
    assert_eq!(as_size(result.unwrap()), Some(0));
    assert_eq!(
        hooks.count(denied_state, 1),
        0,
        "permission must decline without running custom method lookup"
    );
    assert_eq!(query(&row), 1);
    assert_eq!(as_size(check().unwrap()), Some(1));
    let input = call(word, b"fromBits:", &[size(7).unwrap()]);
    assert_eq!(q.word_bits(call(input, b"bits", &[])).unwrap(), 7);
}
