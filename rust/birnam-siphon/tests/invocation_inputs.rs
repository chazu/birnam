//! Execution scope covers generated inputs and exact input aliases through GC.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use std::{cell::Cell, ffi::c_void, os::unix::ffi::OsStrExt};
use support::*;
struct Checkpoint {
    id: Protocol,
    gc: Oop,
    roots: Oop,
    token: Cell<usize>,
    calls: Cell<usize>,
}
unsafe extern "C" fn checkpoint(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    let s = unsafe { &*c.cast::<Checkpoint>() };
    let args = unsafe { borrowed_slice(a, n) }.unwrap();
    assert_eq!(args.len(), 2);
    let t = s.token.replace(0);
    if t != 0 {
        unsafe { release(s.id, s.roots, t) }
    }
    unsafe {
        collect(s.id, s.gc);
        for arg in args {
            assert!(
                present(s.id, object(*arg)),
                "invocation must cover input through callback"
            );
        }
    }
    s.calls.set(s.calls.get() + 1);
    Outcome::loaded(args[1])
}
unsafe extern "C" fn fail(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe { checkpoint(c, r, a, n) };
    Outcome::failed(811, 97)
}
#[test]
#[ignore = "production synchronous compiler and Execution scope required"]
fn generated_inputs_survive_reentry_collection_failure_aliases_and_new_results() {
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
        let b = behavior(id);
        let h = allocate(id, b, 0);
        let ht = retain(id, roots, h);
        let mut check = Checkpoint {
            id,
            gc,
            roots,
            token: Cell::new(0),
            calls: Cell::new(0),
        };
        install(id, b, b"check:with:", checkpoint, (&raw mut check).cast());
        install(id, b, b"fail:with:", fail, (&raw mut check).cast());
        send(
            id,
            named(b"Globals"),
            b"at:put:",
            &[
                Operand::object(id.intern_name(b"InputCheckpoint").unwrap()),
                Operand::object(h),
            ],
        );
        let source=br#"let probe := [Object clone];
      [Reflect defineMethod: #echo: on: probe as: [Execution block: [:self :value | [InputCheckpoint check: self with: value]; value] withNativeSignature: "oO"]];
      [Reflect defineMethod: #nested: on: probe as: [Execution block: [:self :value | [self echo: value]] withNativeSignature: "oO"]];
      [Reflect defineMethod: #fresh: on: probe as: [Execution block: [:self :value | let result := [Execution wordResultOf: value selector: #word]; [InputCheckpoint check: self with: result]; result] withNativeSignature: "oO"]];
      [Reflect defineMethod: #failure: on: probe as: [Execution block: [:self :value | [InputCheckpoint fail: self with: value]] withNativeSignature: "oO"]];
      [Globals at: #InputScopeProbe put: probe]; true"#;
        let text = object(send(
            id,
            named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(source)],
        ));
        let tt = retain(id, roots, text);
        let path =
            std::env::temp_dir().join(format!("birnam-input-scope-{}.dylib", std::process::id()));
        let output = object(send(
            id,
            named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(path.as_os_str().as_bytes())],
        ));
        let ot = retain(id, roots, output);
        let compile = object(send(
            id,
            named(b"Codegen"),
            b"compile:output:from:synchronousBodies:",
            &[
                Operand::object(text),
                Operand::object(output),
                Operand::object(output),
                Operand::object(named(b"true")),
            ],
        ));
        assert_eq!(object(send(id, compile, b"isError", &[])), named(b"false"));
        send(id, named(b"Codegen"), b"load:", &[Operand::object(output)]);
        let probe = object(send(
            id,
            named(b"Globals"),
            b"at:",
            &[Operand::object(id.intern_name(b"InputScopeProbe").unwrap())],
        ));
        let pt = retain(id, roots, probe);
        for selector in [b"echo:".as_slice(), b"nested:", b"fresh:", b"failure:"] {
            let value = object(send(
                id,
                named(b"Integer"),
                b"fromWord:",
                &[Operand::word(123)],
            ));
            check.token.set(retain(id, roots, value));
            let result = id.send_named(probe, selector, &[Operand::object(value)]);
            assert_eq!(
                check.token.get(),
                0,
                "callback relinquishes external token while Execution protects input"
            );
            if selector == b"failure:" {
                let error = result.unwrap_err();
                assert_eq!((error.status, error.detail), (811, 97));
            } else {
                let result = object(result.unwrap());
                let rt = retain(id, roots, result);
                if selector == b"fresh:" {
                    assert_eq!(send(id, result, b"bits", &[]).as_word(), Some(123));
                } else {
                    assert_eq!(result, value);
                }
                collect(id, gc);
                assert!(present(id, result));
                release(id, roots, rt);
            }
        }
        // Captured exact Method keeps the old implementation/code after replacement.
        let original = send(
            id,
            id.receiver,
            b"bind:to:",
            &[
                Operand::object(id.intern_name(b"echo:").unwrap()),
                Operand::object(probe),
            ],
        )
        .as_entry()
        .unwrap();
        let method = object(send(
            id,
            named(b"Execution"),
            b"methodOfEntry:",
            &[Operand::entry(original)],
        ));
        let mt = retain(id, roots, method);
        let replacement = send(
            id,
            id.receiver,
            b"bind:to:",
            &[
                Operand::object(id.intern_name(b"failure:").unwrap()),
                Operand::object(probe),
            ],
        )
        .as_entry()
        .unwrap();
        let pb = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(probe)],
        ));
        install(
            id,
            pb,
            b"echo:",
            replacement.method.unwrap(),
            replacement.context,
        );
        let value = object(send(
            id,
            named(b"Integer"),
            b"fromWord:",
            &[Operand::word(55)],
        ));
        let vt = retain(id, roots, value);
        let vector = object(send(id, named(b"Vector"), b"new", &[]));
        let vector = object(send(id, vector, b"add:", &[Operand::object(value)]));
        let at = retain(id, roots, vector);
        let alias = object(send(
            id,
            method,
            b"invokeOn:arguments:",
            &[Operand::object(probe), Operand::object(vector)],
        ));
        assert_eq!(alias, value);
        let error = id
            .send_named(probe, b"echo:", &[Operand::object(value)])
            .unwrap_err();
        assert_eq!(error.status, 811);
        // Required incoming type rejection happens before a checkpoint/body call.
        let calls = check.calls.get();
        assert!(
            id.send_named(probe, b"nested:", &[Operand::word(1)])
                .is_err()
        );
        assert_eq!(check.calls.get(), calls);
        for token in [at, vt, mt, pt, ot, tt, ht] {
            release(id, roots, token);
        }
        std::fs::remove_file(path).unwrap();
    }
}
