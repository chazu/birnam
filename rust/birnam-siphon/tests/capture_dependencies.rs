//! Compiled capture access uses existing selectors, independently of name services.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
#[test]
#[ignore = "requires production Oop compiler and Execution"]
fn unrelated_name_services_are_not_capture_dependencies() {
    let host = oop::bootstrap();
    let q = host.scope();
    let hooks = oop::hooks::Hooks::new(&host);
    let named = |n: &[u8]| host.named(n).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let call = |r, n: &[u8], args: &[Oop]| q.call(r, n, args).unwrap();
    let text = q
        .string(b"let marker := [Object clone]; [| marker]")
        .unwrap();
    let block = call(named(b"Codegen"), b"eval:", &[text]);
    q.check_result(block).unwrap();
    let count = call(block, b"captureCount", &[]);
    assert_eq!(
        q.word_bits(call(named(b"Integer"), b"wordOf:", &[count]))
            .unwrap(),
        1
    );
    let index = call(named(b"Integer"), b"fromWord:", &[size(0).unwrap()]);
    let capture = key(b"captureAt:");
    let value = key(b"value");
    let put = key(b"at:put:");
    let at = key(b"at:");
    let add = key(b"addMethod");
    let marker = call(block, b"captureAt:", &[index]);
    assert_eq!(call(block, b"value", &[]), marker);
    assert!(
        call(
            host.id.root,
            b"bind:to:",
            &[key(b"tryBorrowCaptureOperands:"), block]
        )
        .is_null(),
        "the raw capture-carrier query is not restored by the Oop migration"
    );
    let names = call(host.id.root, b"nameBehavior", &[]);
    let existing = key(b"existingSymbol");
    let intern = host.id.intern;
    let original_intern = call(names, b"lookup", &[intern]);
    let original_existing = call(names, b"lookup", &[existing]);
    let bytes: Vec<_> = b"capture-dependency-probe"
        .iter()
        .map(|&v| size(v as usize).unwrap())
        .collect();
    let name = call(host.id.root, b"nameWithBytes", &bytes);
    let failure = hooks.fault(73);
    let state = hooks.context(&[failure, size(0).unwrap()]);
    let deny = hooks.method(b"deny", state);
    let raw = |receiver, selector, args: &[Oop]| unsafe { host.id.send(receiver, selector, args) };
    raw(names, add, &[intern, deny]).unwrap();
    raw(names, add, &[existing, deny]).unwrap();
    // Verify both replacements are effective, then observe only capture work.
    for selector in [intern, existing] {
        assert_eq!(raw(name, selector, &[]).unwrap_err(), failure);
    }
    assert_eq!(
        raw(state, at, &[size(1).unwrap()]).unwrap(),
        size(2).unwrap()
    );
    raw(state, put, &[size(1).unwrap(), size(0).unwrap()]).unwrap();
    assert_eq!(raw(block, capture, &[index]).unwrap(), marker);
    assert_eq!(raw(block, value, &[]).unwrap(), marker);
    assert_eq!(
        as_size(raw(state, at, &[size(1).unwrap()]).unwrap()),
        Some(0),
        "capture access and compiled invocation must not consult unrelated name services"
    );
    raw(names, add, &[intern, original_intern]).unwrap();
    raw(names, add, &[existing, original_existing]).unwrap();
}
