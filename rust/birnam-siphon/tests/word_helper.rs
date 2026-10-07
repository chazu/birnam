//! Actual authored helper plans retain current dispatch and capture meanings.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use std::{cell::Cell, ffi::c_void, os::unix::ffi::OsStrExt};
use support::*;
struct Hook {
    entry: NativeEntry,
    calls: Cell<usize>,
}
unsafe extern "C" fn forward(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let h = unsafe { &*c.cast::<Hook>() };
    h.calls.set(h.calls.get() + 1);
    unsafe { (h.entry.method.unwrap())(h.entry.context, r, a, n) }
}
unsafe extern "C" fn receiver(c: *mut c_void, r: Oop, _: *const Operand, _: usize) -> Outcome {
    let h = unsafe { &*c.cast::<Hook>() };
    h.calls.set(h.calls.get() + 1);
    Outcome::loaded(Operand::object(r))
}
unsafe extern "C" fn argument(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    let h = unsafe { &*c.cast::<Hook>() };
    h.calls.set(h.calls.get() + 1);
    Outcome::loaded(unsafe { borrowed_slice(a, n) }.unwrap()[1])
}
struct Grant {
    id: Protocol,
    gc: Oop,
    entry: NativeEntry,
    behavior: Oop,
    hook: *mut Hook,
}
unsafe extern "C" fn grant(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let g = unsafe { &*c.cast::<Grant>() };
    let answer = unsafe { (g.entry.method.unwrap())(g.entry.context, r, a, n) };
    unsafe {
        collect(g.id, g.gc);
        install(
            g.id,
            g.behavior,
            b"value:value:value:",
            argument,
            g.hook.cast(),
        );
    }
    answer
}
#[test]
#[ignore = "production synchronous compiler and Word helper protocols required"]
fn authored_word_helpers_guard_body_invocation_captures_and_primitives() {
    unsafe {
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        let named = |name: &[u8]| {
            object(send(
                id,
                ns,
                b"at:",
                &[Operand::object(id.intern_name(name).unwrap())],
            ))
        };
        let roots = named(b"Roots");
        let gc = named(b"GC");
        let source=br#"[Module named: #wordHelperProbe requires: #[]]; let mask := [Execution wordResultOf: 15 selector: #word]; let probe := [Object clone]; [Reflect defineMethod: #compute:with: on: probe as: [:self :a :b | let x := [a bitXor: b]; let sum := [x wrappingAdd: a]; [[sum bitXor: mask] wrappingSubtract: sum]]]; [Globals at: #WordHelperProbe put: probe]; true"#;
        let text = object(send(
            id,
            named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(source)],
        ));
        let tt = retain(id, roots, text);
        let path =
            std::env::temp_dir().join(format!("birnam-word-helper-{}.dylib", std::process::id()));
        let out = object(send(
            id,
            named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(path.as_os_str().as_bytes())],
        ));
        let ot = retain(id, roots, out);
        let answer = object(send(
            id,
            named(b"Codegen"),
            b"compile:output:from:synchronousBodies:",
            &[
                Operand::object(text),
                Operand::object(out),
                Operand::object(out),
                Operand::object(named(b"true")),
            ],
        ));
        assert_eq!(object(send(id, answer, b"isError", &[])), named(b"false"));
        let loaded = object(send(
            id,
            named(b"Codegen"),
            b"load:",
            &[Operand::object(out)],
        ));
        assert_eq!(object(send(id, loaded, b"isError", &[])), named(b"false"));
        let probe = object(send(
            id,
            named(b"Globals"),
            b"at:",
            &[Operand::object(id.intern_name(b"WordHelperProbe").unwrap())],
        ));
        let pt = retain(id, roots, probe);
        let make = |v| object(send(id, named(b"Word"), b"fromBits:", &[Operand::word(v)]));
        let a = make(10);
        let b = make(6);
        let at = retain(id, roots, a);
        let bt = retain(id, roots, b);
        let run = || {
            let answer = object(send(
                id,
                probe,
                b"compute:with:",
                &[Operand::object(a), Operand::object(b)],
            ));
            send(id, answer, b"bits", &[]).as_word().unwrap()
        };
        let bind = |r, name: &[u8]| {
            send(
                id,
                id.receiver,
                b"bind:to:",
                &[
                    Operand::object(id.intern_name(name).unwrap()),
                    Operand::object(r),
                ],
            )
            .as_entry()
            .unwrap()
        };
        let word = named(b"Word");
        let word_vt = named(b"WordVt");
        let plan_entry = bind(word, b"tryPrimitiveExpressionAtEpoch:");
        let mut plan_hook = Hook {
            entry: plan_entry,
            calls: Cell::new(0),
        };
        install(
            id,
            word_vt,
            b"tryPrimitiveExpressionAtEpoch:",
            forward,
            (&raw mut plan_hook).cast(),
        );
        assert_eq!(run(), 3);
        assert_eq!(
            plan_hook.calls.get(),
            1,
            "the authored body must enter its guarded helper plan"
        );
        let selected = bind(probe, b"compute:with:");
        let implementation = object(send(
            id,
            named(b"Execution"),
            b"implementationOfEntry:",
            &[Operand::entry(selected)],
        ));
        let it = retain(id, roots, implementation);
        let body_vt = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(implementation)],
        ));
        let value_entry = bind(implementation, b"value:value:value:");
        let mut h = Hook {
            entry: value_entry,
            calls: Cell::new(0),
        };
        install(
            id,
            body_vt,
            b"value:value:value:",
            argument,
            (&raw mut h).cast(),
        );
        plan_hook.calls.set(0);
        assert_eq!(run(), 10);
        assert_eq!(h.calls.get(), 1);
        assert_eq!(plan_hook.calls.get(), 0);
        install(
            id,
            body_vt,
            b"value:value:value:",
            value_entry.method.unwrap(),
            value_entry.context,
        );
        let block = named(b"Block");
        let block_vt = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(block)],
        ));
        let capture = bind(block, b"captureWordAt:");
        h.entry = capture;
        h.calls.set(0);
        install(
            id,
            block_vt,
            b"captureWordAt:",
            forward,
            (&raw mut h).cast(),
        );
        plan_hook.calls.set(0);
        assert_eq!(run(), 3);
        assert_eq!(h.calls.get(), 1);
        assert_eq!(plan_hook.calls.get(), 0);
        install(
            id,
            block_vt,
            b"captureWordAt:",
            capture.method.unwrap(),
            capture.context,
        );
        let xor = bind(a, b"bitXor:");
        h.entry = xor;
        h.calls.set(0);
        install(id, word_vt, b"bitXor:", receiver, (&raw mut h).cast());
        assert_eq!(run(), 0);
        assert_eq!(h.calls.get(), 2);
        install(id, word_vt, b"bitXor:", xor.method.unwrap(), xor.context);
        // Mutation after a positive allocation grant must invalidate the earlier
        // helper-body proof, before any optimized result is published.
        let model = named(b"ObjectModel");
        let model_vt = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(model)],
        ));
        let permission = bind(model, b"canOmitAllocationFor:");
        h.calls.set(0);
        let mut g = Grant {
            id,
            gc,
            entry: permission,
            behavior: body_vt,
            hook: &raw mut h,
        };
        install(
            id,
            model_vt,
            b"canOmitAllocationFor:",
            grant,
            (&raw mut g).cast(),
        );
        assert_eq!(run(), 10);
        assert_eq!(
            h.calls.get(),
            1,
            "guard mutation must invoke the new Block body once"
        );
        install(
            id,
            model_vt,
            b"canOmitAllocationFor:",
            permission.method.unwrap(),
            permission.context,
        );
        install(
            id,
            body_vt,
            b"value:value:value:",
            value_entry.method.unwrap(),
            value_entry.context,
        );
        send(id, gc, b"budget:", &[Operand::word(1)]);
        assert_eq!(run(), 3);
        assert!(present(id, a) && present(id, b));
        install(
            id,
            word_vt,
            b"tryPrimitiveExpressionAtEpoch:",
            plan_entry.method.unwrap(),
            plan_entry.context,
        );
        for token in [it, bt, at, pt, ot, tt] {
            release(id, roots, token);
        }
        std::fs::remove_file(path).unwrap();
    }
}
