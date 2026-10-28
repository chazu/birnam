//! Alias returns retain input identity, current effects and ordinary frame visibility.
use super::*;
pub(super) fn check(h: &H<'_>) {
    h.clear();
    h.collect();
    let baseline_units = h.units();
    let baseline_bytes = h.native_bytes();
    let method_vt = h.call(h.host.id.root, b"methodBehavior", &[]);
    let attach = h.key(b"withInline:at:owner:");
    let original = h.call(method_vt, b"lookup", &[attach]);
    assert!(!original.is_null());
    let admitted = h.hooks.context(&[original, size(0).unwrap()]);
    h.call(
        method_vt,
        b"addMethod",
        &[attach, h.hooks.method(b"counted", admitted)],
    );
    let body = h.eval(b"[:x | let alias := x; let another := alias; another]");
    h.call(method_vt, b"addMethod", &[attach, original]);
    h.host.scope().check_result(body).unwrap();
    eprintln!(
        "automatic alias-return candidates={}",
        h.hooks.count(admitted, 1)
    );
    let other_body = h.eval(b"[:x | let alias := x; let another := alias; another]");
    let argument = h.eval(b"[Object clone]");
    h.bind(b"InlineArgument", argument);
    h.bind(b"InlineReturn", body);
    // The source literal, rather than obsolete boxed routing indices, supplies
    // the actual language constructor effect being compared.
    let block = h.block(h.compile(
        b"let inlineMarker := 35; let answer := [InlineReturn value: InlineArgument]; answer",
    ));
    let machine = h.named(b"Machine");
    let gc = h.named(b"GC");
    let raw = |r, s, a: &[Oop]| unsafe { h.host.id.send(r, s, a) };
    let run_key = h.key(b"runBlock:");
    let add = h.key(b"addMethod");
    let run = || h.call(h.call(machine, b"new", &[]), b"runBlock:", &[block]);
    for callee in [body, other_body] {
        h.bind(b"InlineReturn", callee);
        assert_eq!(run(), argument);
    }
    for (receiver, selector) in [
        (h.named(b"Integer"), b"fromWord:".as_slice()),
        (machine, b"oopAt:"),
    ] {
        let vt = h.vt(receiver);
        let selector = h.key(selector);
        let original = h.call(h.host.id.root, b"bind:to:", &[selector, receiver]);
        let state = h.hooks.context(&[original, size(0).unwrap()]);
        let replacement = h.hooks.method(b"counted", state);
        let mut counts = [0; 2];
        for (i, callee) in [body, other_body].into_iter().enumerate() {
            h.bind(b"InlineReturn", callee);
            h.hooks.put(state, 1, size(0).unwrap());
            let target = h.call(machine, b"new", &[]);
            raw(vt, add, &[selector, replacement]).unwrap();
            let result = raw(target, run_key, &[block]);
            raw(vt, add, &[selector, original]).unwrap();
            assert_eq!(result.unwrap(), argument);
            counts[i] = h.hooks.count(state, 1);
        }
        assert!(counts[0] > 0);
        assert_eq!(counts[0], counts[1]);
        let state = h.hooks.context(&[original, gc, size(0).unwrap()]);
        let replacement = h.hooks.method(b"collectOnce", state);
        let target = h.call(machine, b"new", &[]);
        raw(vt, add, &[selector, replacement]).unwrap();
        let result = raw(target, run_key, &[block]);
        raw(vt, add, &[selector, original]).unwrap();
        assert_eq!(result.unwrap(), argument);
        assert_eq!(h.hooks.count(state, 2), 1);
    }
    h.bind(b"InlineReturn", body);
    let replaced = h.eval(b"[Object clone]");
    let state = h.hooks.context(&[replaced, size(0).unwrap()]);
    let vt = h.call(h.host.id.root, b"ensurePrivateVtableFor:", &[body]);
    let value = h.key(b"value:");
    let original = h.call(vt, b"lookup", &[value]);
    raw(vt, add, &[value, h.hooks.method(b"fixedCount", state)]).unwrap();
    let result = run();
    raw(vt, add, &[value, original]).unwrap();
    assert_eq!(result, replaced);
    assert_eq!(h.hooks.count(state, 1), 1);
    assert_eq!(run(), argument);
    let target = h.call(machine, b"new", &[]);
    let budget = h.eval(b"1");
    let mut result = h.call(target, b"runBlock:budget:", &[block, budget]);
    let mut observed = false;
    for _ in 0..128 {
        if result != target {
            break;
        }
        let view = h.call(target, b"currentFrameView", &[]);
        if view != h.named(b"nil") {
            let count = h.integer(h.call(view, b"slotCount", &[]));
            for index in 0..count {
                observed |= h.call(view, b"oopAt:", &[size(index as usize).unwrap()]) == argument;
            }
        }
        h.collect();
        result = h.call(target, b"continueWithBudget:", &[budget]);
    }
    assert!(observed);
    assert_eq!(result, argument);
    for name in [
        b"InlineReturn".as_slice(),
        b"InlineArgument",
        b"inlineMarker",
        b"answer",
    ] {
        h.bind(name, h.named(b"nil"));
    }
    h.call(h.named(b"Codegen"), b"evictMaterializations", &[]);
    h.clear();
    h.collect();
    assert_eq!(h.units(), baseline_units);
    assert_eq!(h.native_bytes(), baseline_bytes);
}
