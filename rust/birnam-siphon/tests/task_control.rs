//! Authored task scheduling over Store and Execution, with actual native barriers.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{Operand, Outcome, Protocol},
};
use core::ffi::c_void;
use std::os::unix::ffi::OsStrExt;
use support::*;

struct Observer {
    id: Protocol,
    cleanup: Oop,
    reason: Oop,
}
unsafe extern "C" fn observe(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Observer>();
        let [target] = borrowed_slice(a, n).unwrap() else {
            panic!("observe args")
        };
        match p.id.send_named(object(*target), b"outcome", &[]) {
            Ok(value) => Outcome::loaded(value),
            Err(error) => error,
        }
    }
}
unsafe extern "C" fn fail(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Observer>();
        let [target] = borrowed_slice(a, n).unwrap() else {
            panic!("fail args")
        };
        match p
            .id
            .send_named(object(*target), b"fail:", &[Operand::object(p.reason)])
        {
            Ok(value) => Outcome::loaded(value),
            Err(error) => error,
        }
    }
}
unsafe extern "C" fn ensure(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Observer>();
        let [target, body] = borrowed_slice(a, n).unwrap() else {
            panic!("ensure args")
        };
        match p.id.send_named(
            object(*target),
            b"ensure:cleanup:",
            &[*body, Operand::object(p.cleanup)],
        ) {
            Ok(value) => Outcome::loaded(value),
            Err(error) => error,
        }
    }
}

#[test]
#[ignore = "scripts/host-cli-test.sh --group tasks supplies installed release modules"]
fn authored_task_scheduling_preserves_native_boundaries() {
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
        let keep = |o: Oop| {
            retain(id, roots, o);
            o
        };
        let text = |b: &[u8]| {
            keep(object(send(
                id,
                named(b"String"),
                b"fromBytes:",
                &[Operand::bytes(b)],
            )))
        };
        let cg = named(b"Codegen");
        let eval = |source: &[u8]| {
            keep(object(send(
                id,
                cg,
                b"evalPersisting:",
                &[Operand::object(text(source))],
            )))
        };
        eval(include_bytes!("../../../src/id/modules.birnam"));
        eval(include_bytes!("../../../src/id/stdlib.birnam"));
        eval(include_bytes!("../../../src/id/stackless-control.birnam"));
        let p = Box::new(Observer {
            id,
            cleanup: eval(b"[| nil]"),
            reason: keep(id.intern_name(b"mustNotRecord").unwrap()),
        });
        let vt = keep(behavior(id));
        let observer = keep(allocate(id, vt, 0));
        let c = (&*p as *const Observer).cast_mut().cast();
        install(id, vt, b"observe:", observe, c);
        install(id, vt, b"failTask:", fail, c);
        install(id, vt, b"ensureTask:body:", ensure, c);
        send(
            id,
            named(b"Globals"),
            b"at:put:",
            &[
                Operand::object(id.intern_name(b"NativeTaskObserver").unwrap()),
                Operand::object(observer),
            ],
        );
        eval(include_bytes!("../../../scripts/task-control-smoke.birnam"));
        eval(b"[TaskCheck run]");
        let failures = eval(b"[TaskCheckFailures value]");
        assert_eq!(
            send(
                id,
                named(b"Integer"),
                b"wordOf:",
                &[Operand::object(failures)]
            )
            .as_word(),
            Some(0)
        );
        let checks = eval(b"[TaskChecks value]");
        assert_eq!(
            send(
                id,
                named(b"Integer"),
                b"wordOf:",
                &[Operand::object(checks)]
            )
            .as_word(),
            Some(171)
        );
        // The native contexts remain pinned until all entered fixture calls finish.
        core::mem::forget(p);
    }
}
