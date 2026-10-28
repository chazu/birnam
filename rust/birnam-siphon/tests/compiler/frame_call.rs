//! Generated calls use current preparation/frame operations and expose real frames.
use super::*;

pub(super) fn check(h: &H<'_>) {
    h.clear();
    h.collect();
    let baseline_units = h.units();
    let baseline_bytes = h.native_bytes();
    let machine = h.named(b"Machine");
    let gc = h.named(b"GC");
    let mvt = h.vt(machine);
    let add = h.key(b"addMethod");
    let run = h.key(b"runBlock:");
    let raw = |r, s, a: &[Oop]| unsafe { h.host.id.send(r, s, a) };
    let argument = h.eval(b"[Object clone]");
    let other = h.eval(b"[Object clone]");
    let callee = h.eval(b"[:a :b :c | b]");
    h.bind(b"FrameArgument", argument);
    h.bind(b"FrameCallee", callee);
    let sources=[
        b"let first := [FrameCallee value: FrameArgument value: FrameArgument value: FrameArgument]; [FrameCallee value: first value: first value: first]".as_slice(),
        b"[| let callee := FrameCallee; let argument := FrameArgument; let one := argument; let two := one; let first := [callee value: two value: one value: argument]; let next := first; let last := next; [callee value: last value: next value: first]]",
    ];
    for (index, source) in sources.into_iter().enumerate() {
        let block = if index == 0 {
            h.block(h.compile(source))
        } else {
            h.eval(source)
        };
        h.host.scope().check_result(block).unwrap();
        let execute = || {
            let target = h.call(machine, b"new", &[]);
            h.keep(raw(target, run, &[block]).unwrap())
        };
        assert_eq!(execute(), argument);
        // Rebinding the callee's ordinary value method affects both calls.
        let cvt = h.call(h.host.id.root, b"ensurePrivateVtableFor:", &[callee]);
        let invoke = h.key(b"value:value:value:");
        let original = h.call(cvt, b"lookup", &[invoke]);
        let called = h.hooks.context(&[other, size(0).unwrap()]);
        raw(cvt, add, &[invoke, h.hooks.method(b"fixedCount", called)]).unwrap();
        let result = execute();
        raw(cvt, add, &[invoke, original]).unwrap();
        assert_eq!(result, other);
        assert_eq!(h.hooks.count(called, 1), 2);
        // Each operation is independently replaced. The first entry collects;
        // forwarding uses an explicit captured Method and cannot bypass lookup.
        for (receiver, selector) in [
            (machine, b"prepareCall:selector:".as_slice()),
            (machine, b"oopAt:"),
            (machine, b"oopAt:put:"),
            (machine, b"callSite:"),
            (machine, b"tailCallSite:"),
            (h.host.namespace, b"at:"),
            (gc, b"prepareAllocation"),
        ] {
            let behavior = h.vt(receiver);
            let selector = h.key(selector);
            let original = h.call(h.host.id.root, b"bind:to:", &[selector, receiver]);
            assert!(!original.is_null());
            let state = h.hooks.context(&[original, gc, size(0).unwrap()]);
            let replacement = h.hooks.method(b"collectOnce", state);
            let target = h.call(machine, b"new", &[]);
            raw(behavior, add, &[selector, replacement]).unwrap();
            let result = raw(target, run, &[block]);
            raw(behavior, add, &[selector, original]).unwrap();
            assert_eq!(result.unwrap(), argument);
            assert_eq!(
                h.hooks.count(state, 2),
                1,
                "current operation must be entered and collect"
            );
        }
        let prepare = h.key(b"prepareCall:selector:");
        let original = h.call(mvt, b"lookup", &[prepare]);
        let failure = h.hooks.fault(71);
        let state = h.hooks.context(&[failure, size(0).unwrap()]);
        let deny = h.hooks.method(b"deny", state);
        let target = h.call(machine, b"new", &[]);
        raw(mvt, add, &[prepare, deny]).unwrap();
        let result = raw(target, run, &[block]);
        raw(mvt, add, &[prepare, original]).unwrap();
        assert_eq!(result, Err(failure));
        assert_eq!(
            h.hooks.count(state, 1),
            1,
            "entered preparation failure must not replay the call"
        );
        assert_eq!(execute(), argument);

        let target = h.call(machine, b"new", &[]);
        let one = h.call(h.named(b"Integer"), b"fromWord:", &[size(1).unwrap()]);
        let mut result = h.call(target, b"runBlock:budget:", &[block, one]);
        let mut observed_argument = false;
        let mut views = Vec::new();
        for _ in 0..128 {
            if result != target {
                break;
            }
            let view = h.call(target, b"currentFrameView", &[]);
            if view != h.named(b"nil") {
                views.push(view);
                let count = h.integer(h.call(view, b"slotCount", &[]));
                for slot in 0..count {
                    let value = h.call(view, b"oopAt:", &[size(slot as usize).unwrap()]);
                    observed_argument |= value == argument;
                }
            }
            h.collect();
            result = h.call(target, b"continueWithBudget:", &[one]);
        }
        assert_eq!(result, argument);
        assert!(!views.is_empty());
        assert!(
            observed_argument,
            "paused frames expose the actual argument identity"
        );
        for view in views {
            let stale = h.call(view, b"programCounter", &[]);
            assert_eq!(h.call(stale, b"kind", &[]), h.key(b"staleFrameView"));
        }
        assert_eq!(h.call(target, b"currentFrameView", &[]), h.named(b"nil"));
    }
    h.bind(b"FrameArgument", h.named(b"nil"));
    h.bind(b"FrameCallee", h.named(b"nil"));
    h.call(h.named(b"Codegen"), b"evictMaterializations", &[]);
    h.clear();
    h.collect();
    assert_eq!(h.units(), baseline_units);
    assert_eq!(h.native_bytes(), baseline_bytes);
}
