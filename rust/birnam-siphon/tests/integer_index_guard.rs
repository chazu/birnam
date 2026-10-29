//! Integer index omission retains live Method facts and allocation policy.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{as_size, size, Oop};
#[test]
#[ignore = "requires production Oop modules"]
fn integer_index_admission_preserves_replacement_policy_and_epoch_fence() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let named = |n: &[u8]| h.named(n).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let root = h.id.root;
    let integer = named(b"Integer");
    let vt = named(b"IntegerVt");
    let root_vt = call(root, b"behaviorOf:", &[root]);
    let model = named(b"ObjectModel");
    let model_vt = call(root, b"behaviorOf:", &[model]);
    let bind = |r, n: &[u8]| call(root, b"bind:to:", &[key(n), r]);
    let set = |vt, n: &[u8], m| {
        call(vt, b"addMethod", &[key(n), m]);
    };
    let selector = key(b"canOmitIndexing");
    let check = || unsafe { h.id.send(integer, selector, &[]) };
    assert_eq!(as_size(check().unwrap()), Some(1));
    let word = bind(integer, b"wordOf:");
    let from = bind(integer, b"fromWord:");
    for (name, original) in [(b"fromWord:".as_slice(), from), (b"wordOf:", word)] {
        let changed = call(original, b"withContext:", &[std::ptr::null_mut()]);
        set(vt, name, changed);
        assert_eq!(as_size(check().unwrap()), Some(0));
        set(vt, name, original);
        assert_eq!(as_size(check().unwrap()), Some(1));
    }
    let lookup = bind(root, b"usesDefaultLookup:");
    let counted = hooks.context(&[lookup, size(0).unwrap()]);
    set(
        root_vt,
        b"usesDefaultLookup:",
        hooks.method(b"counted", counted),
    );
    assert_eq!(as_size(check().unwrap()), Some(1));
    assert!(
        hooks.count(counted, 1) > 0,
        "the current fact query must execute"
    );
    set(root_vt, b"usesDefaultLookup:", lookup);
    let binder = bind(root, b"bind:to:");
    let failure = hooks.fault(1901);
    set(
        root_vt,
        b"bind:to:",
        hooks.method(b"deny", hooks.context(&[failure, size(0).unwrap()])),
    );
    assert_eq!(check().unwrap_err(), failure);
    set(root_vt, b"bind:to:", binder);

    let policy = bind(model, b"canOmitAllocationFor:");
    let state = hooks.context(&[size(1).unwrap(), size(0).unwrap()]);
    set(
        model_vt,
        b"canOmitAllocationFor:",
        hooks.method(b"fixedCount", state),
    );
    let epoch = call(root, b"behaviorEpoch", &[]);
    for allowed in [1, 0, 1] {
        hooks.put(state, 0, size(allowed).unwrap());
        assert_eq!(as_size(check().unwrap()), Some(allowed));
    }
    assert_eq!(call(root, b"behaviorEpoch", &[]), epoch);
    assert_eq!(hooks.count(state, 1), 3);
    set(model_vt, b"canOmitAllocationFor:", policy);

    let factory = call(named(b"Codegen"), b"eval:", &[q.string(b"[:target :replacement | [Execution method: [:self :class | [target addMethod: replacement as: #wordOf:]; [1 word]] withNativeSignature: \"wO\"]]").unwrap()]);
    q.check_result(factory).unwrap();
    let wrong = call(word, b"withContext:", &[std::ptr::null_mut()]);
    let changing = call(factory, b"value:value:", &[vt, wrong]);
    set(model_vt, b"canOmitAllocationFor:", changing);
    assert_eq!(
        as_size(check().unwrap()),
        Some(0),
        "mutation during permission evaluation must decline"
    );
    set(vt, b"wordOf:", word);
    set(model_vt, b"canOmitAllocationFor:", policy);
    assert_eq!(as_size(check().unwrap()), Some(1));
    let value = call(integer, b"fromWord:", &[size(41).unwrap()]);
    assert_eq!(
        q.word_bits(call(integer, b"wordOf:", &[value])).unwrap(),
        41
    );
}
