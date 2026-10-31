//! Process-local native state refuses persistence through its current owner policy.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{object_reference, size};

#[test]
#[ignore = "requires authored image policy and native Image"]
fn machine_refusal_precedes_storage_queries_and_preserves_owner_failures() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let system = h.named(b"System").unwrap();
    let nil = h.named(b"nil").unwrap();
    let globals = h.named(b"Globals").unwrap();
    let key = hooks.symbol(b"ImageUnsupportedProbe");
    let machine = q.call(h.named(b"Machine").unwrap(), b"new", &[]).unwrap();
    let path = std::env::temp_dir().join(format!("birnam-machine-refusal-{}.image", std::process::id()));
    let filename = q.string(path.as_os_str().as_encoded_bytes()).unwrap();
    let sentinel = b"leave the existing destination untouched";
    std::fs::write(&path, sentinel).unwrap();
    for private in [false, true] {
        if private {
            q.call(h.named(b"ObjectModel").unwrap(), b"ensurePrivateVtableFor:", &[machine]).unwrap();
        }
        q.call(globals, b"at:put:", &[key, machine]).unwrap();
        assert!(h.send(h.named(b"ObjectModel").unwrap(), b"imageSizeOf:", &[machine]).is_err());
        assert_eq!(q.call(system, b"saveImage:", &[filename]).unwrap(), nil);
        assert_eq!(std::fs::read(&path).unwrap(), sentinel);
    }

    // A current owner failure is not an unsupported-format refusal. Do not
    // fix admission by swallowing arbitrary errors from imagePayload.
    let behavior = q.call(h.id.root, b"behaviorOf:", &[machine]).unwrap();
    let selector = hooks.symbol(b"imagePayload");
    let original = hooks.capture(behavior, selector);
    let failure = hooks.fault(919);
    let state = hooks.context(&[failure, size(0).unwrap()]);
    q.call(behavior, b"addMethod", &[selector, hooks.method(b"deny", state)]).unwrap();
    let save = hooks.symbol(b"saveImage:");
    assert_eq!(unsafe { h.id.send(system, save, &[filename]) }, Err(failure));
    assert_eq!(std::fs::read(&path).unwrap(), sentinel);
    q.call(behavior, b"removeMethod:", &[selector]).unwrap();
    assert_eq!(hooks.capture(behavior, selector), original);
    q.call(globals, b"at:put:", &[key, nil]).unwrap();

    // An invalidRequest from an applicable storage getter must still propagate;
    // the admission fix must not translate that error category wholesale to nil.
    let model = h.named(b"ObjectModel").unwrap();
    let model_behavior = q.call(h.id.root, b"ensurePrivateVtableFor:", &[model]).unwrap();
    let size_selector = hooks.symbol(b"imageSizeOf:");
    let size_method = hooks.capture(model_behavior, size_selector);
    let invalid = object_reference(h.id.invalid);
    let state = hooks.context(&[invalid, size(0).unwrap()]);
    q.call(model_behavior, b"addMethod", &[size_selector, hooks.method(b"deny", state)]).unwrap();
    assert_eq!(unsafe { h.id.send(system, save, &[filename]) }, Err(invalid));
    assert_eq!(std::fs::read(&path).unwrap(), sentinel);
    q.call(model_behavior, b"addMethod", &[size_selector, size_method]).unwrap();
    std::fs::remove_file(path).unwrap();
}
