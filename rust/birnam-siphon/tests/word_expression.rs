//! Allocation witnesses and guard refusal for emitted nonescaping Word trees.
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
    word_behavior: Oop,
}
struct Admission {
    id: Protocol,
    gc: Oop,
    entry: NativeEntry,
    vt: Oop,
    replacement: *mut Hook,
    change: Cell<bool>,
    allowed: Cell<bool>,
}
unsafe extern "C" fn admission(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let s = unsafe { &*c.cast::<Admission>() };
    let answer = unsafe { (s.entry.method.unwrap())(s.entry.context, r, a, n) };
    unsafe {
        collect(s.id, s.gc);
    }
    if s.change.get() {
        unsafe {
            install(s.id, s.vt, b"bitXor:", xor_hook, s.replacement.cast());
        }
    }
    if s.allowed.get() { answer } else { Outcome::loaded(Operand::word(0)) }
}
struct Words {
    id: Protocol,
    behavior: Oop,
    count: usize,
}
unsafe extern "C" fn count_word(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    let s = unsafe { &mut *c.cast::<Words>() };
    let a = unsafe { borrowed_slice(a, n) }.unwrap();
    if unsafe { send(s.id, s.id.receiver, b"behaviorOf:", &[a[0]]) }.as_object() == Some(s.behavior)
    {
        s.count += 1;
    }
    Outcome::loaded(Operand::unit())
}
unsafe fn words(id: Protocol, behavior: Oop) -> usize {
    let mut w = Words {
        id,
        behavior,
        count: 0,
    };
    unsafe {
        inventory(id, count_word, (&raw mut w).cast());
    }
    w.count
}
unsafe extern "C" fn allocation_hook(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    let s = unsafe { &*c.cast::<Hook>() };
    if unsafe { borrowed_slice(a, n) }.unwrap()[0].as_object() == Some(s.word_behavior) {
        s.calls.set(s.calls.get() + 1);
    }
    unsafe { (s.entry.method.unwrap())(s.entry.context, r, a, n) }
}
unsafe extern "C" fn xor_hook(c: *mut c_void, r: Oop, _: *const Operand, _: usize) -> Outcome {
    let s = unsafe { &*c.cast::<Hook>() };
    s.calls.set(s.calls.get() + 1);
    Outcome::loaded(Operand::object(r))
}
#[test]
#[ignore = "production Word expression and compiler modules required"]
fn word_temporaries_elide_only_with_current_owner_permissions() {
    unsafe {
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        let named = |v: &[u8]| {
            object(send(
                id,
                ns,
                b"at:",
                &[Operand::object(id.intern_name(v).unwrap())],
            ))
        };
        let roots = named(b"Roots");
        let gc = named(b"GC");
        let code=br#"let Probe := [Object clone]; [Reflect defineMethod: #compute:with: on: Probe as: [Execution block: [:self :a :b | let xor := [a bitXor: b]; let sum := [xor wrappingAdd: a]; let masked := [sum bitAnd: b]; [masked wrappingAdd: a]] withNativeSignature: "oOO"]]; Probe"#;
        let text = object(send(
            id,
            named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(code)],
        ));
        let tt = retain(id, roots, text);
        let probe = object(send(
            id,
            named(b"Codegen"),
            b"eval:",
            &[Operand::object(text)],
        ));
        let pt = retain(id, roots, probe);
        release(id, roots, tt);
        assert_eq!(object(send(id, probe, b"isError", &[])), named(b"false"));
        let make = |x| object(send(id, named(b"Word"), b"fromBits:", &[Operand::word(x)]));
        let a = make(10);
        let b = make(6);
        let at = retain(id, roots, a);
        let bt = retain(id, roots, b);
        let run = || {
            object(send(
                id,
                probe,
                b"compute:with:",
                &[Operand::object(a), Operand::object(b)],
            ))
        };
        let read = |v| send(id, v, b"bits", &[]).as_word().unwrap();
        run();
        collect(id, gc);
        send(id, gc, b"budget:", &[Operand::word(usize::MAX)]);
        let before = words(id, named(b"WordVt"));
        let result = run();
        let result_pin = retain(id, roots, result);
        assert_eq!(read(result), 16);
        let after = words(id, named(b"WordVt"));
        assert_eq!(
            after - before,
            1,
            "only the final escaping Word should exist"
        );
        let vt = named(b"WordVt");
        let old = send(
            id,
            id.receiver,
            b"bind:to:",
            &[
                Operand::object(id.intern_name(b"bitXor:").unwrap()),
                Operand::object(a),
            ],
        )
        .as_entry()
        .unwrap();
        let mut hook = Hook {
            entry: old,
            calls: Cell::new(0),
            word_behavior: vt,
        };
        install(id, vt, b"bitXor:", xor_hook, (&raw mut hook).cast());
        assert_eq!(read(run()), 14);
        assert_eq!(
            hook.calls.get(),
            1,
            "replaced primitive must execute exactly once"
        );
        install(id, vt, b"bitXor:", old.method.unwrap(), old.context);
        let model = named(b"ObjectModel");
        let mvt = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(model)],
        ));
        let old = send(
            id,
            id.receiver,
            b"bind:to:",
            &[
                Operand::object(id.intern_name(b"allocate:bytes:").unwrap()),
                Operand::object(model),
            ],
        )
        .as_entry()
        .unwrap();
        hook.entry = old;
        hook.calls.set(0);
        install(
            id,
            mvt,
            b"allocate:bytes:",
            allocation_hook,
            (&raw mut hook).cast(),
        );
        assert_eq!(read(run()), 16);
        assert_eq!(
            hook.calls.get(),
            4,
            "allocation replacement must restore all original Word constructions"
        );
        install(
            id,
            mvt,
            b"allocate:bytes:",
            old.method.unwrap(),
            old.context,
        );
        // Restoring default meaning must permit the next invocation again.
        collect(id, gc);
        send(id, gc, b"budget:", &[Operand::word(usize::MAX)]);
        let before = words(id, vt);
        assert_eq!(read(run()), 16);
        assert_eq!(words(id, vt) - before, 1);
        let permit_name = id.intern_name(b"canOmitAllocationFor:").unwrap();
        let permit = send(
            id,
            id.receiver,
            b"bind:to:",
            &[Operand::object(permit_name), Operand::object(model)],
        )
        .as_entry()
        .unwrap();
        let xor = send(
            id,
            id.receiver,
            b"bind:to:",
            &[
                Operand::object(id.intern_name(b"bitXor:").unwrap()),
                Operand::object(a),
            ],
        )
        .as_entry()
        .unwrap();
        let mut grant = Admission {
            id,
            gc,
            entry: permit,
            vt,
            replacement: &raw mut hook,
            change: Cell::new(false),
            allowed: Cell::new(true),
        };
        install(
            id,
            mvt,
            b"canOmitAllocationFor:",
            admission,
            (&raw mut grant).cast(),
        );
        let answer = run();
        let pin = retain(id, roots, answer);
        assert_eq!(read(answer), 16);
        assert!(
            present(id, a) && present(id, b),
            "collecting admission must preserve inputs"
        );
        release(id, roots, pin);
        // Stateful permission may revoke without any method/epoch mutation.
        // Query directly: inventory visitors would themselves rebind methods.
        let expression = [1, 2, 1, 2, 6, 0, 1];
        let evaluate = || send(id, named(b"Word"), b"tryPrimitiveExpression:", &[
            Operand::bytes(&expression), Operand::object(a), Operand::object(b),
        ]);
        let epoch = send(id, id.receiver, b"behaviorEpoch", &[]).as_word();
        grant.allowed.set(false);
        assert_eq!(evaluate().kind(), birnam_abi::id_protocol::UNIT);
        assert_eq!(send(id, id.receiver, b"behaviorEpoch", &[]).as_word(), epoch);
        grant.allowed.set(true);
        assert_eq!(read(object(evaluate())), 16);
        assert_eq!(send(id, id.receiver, b"behaviorEpoch", &[]).as_word(), epoch);
        grant.change.set(true);
        hook.calls.set(0);
        assert_eq!(read(run()), 14);
        assert_eq!(
            hook.calls.get(),
            1,
            "epoch change during admission must execute the replaced primitive once"
        );
        install(
            id,
            mvt,
            b"canOmitAllocationFor:",
            permit.method.unwrap(),
            permit.context,
        );
        install(id, vt, b"bitXor:", xor.method.unwrap(), xor.context);
        // Exercise all fixed-buffer limits: 32 inputs, 32 operations, 33 roots.
        let mut program = vec![1, 32, 32, 63];
        let mut arguments = vec![Operand::unit()];
        let mut pins = vec![];
        for i in 0..32 {
            let value = make(i);
            pins.push(retain(id, roots, value));
            arguments.push(Operand::object(value));
            program.extend_from_slice(&[6, if i == 0 { 0 } else { (31 + i) as u8 }, i as u8]);
        }
        arguments[0] = Operand::bytes(&program);
        let maximum = object(send(
            id,
            named(b"Word"),
            b"tryPrimitiveExpression:",
            &arguments,
        ));
        assert_eq!(read(maximum), 496);
        for token in pins {
            release(id, roots, token);
        }
        for token in [result_pin, bt, at, pt] {
            release(id, roots, token);
        }
    }
}
