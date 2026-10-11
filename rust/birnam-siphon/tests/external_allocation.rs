//! Native Bytes capacity participates in the fixed GC budget.
mod support;
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use core::{cell::Cell, ffi::c_void};
use std::os::unix::ffi::OsStrExt;
use support::*;

struct Charge {
    id: Protocol,
    gc: Oop,
    original: NativeEntry,
    calls: Cell<usize>,
    charged: Cell<usize>,
}
unsafe extern "C" fn collecting_charge(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    (|| unsafe {
        let s = &*c.cast::<Charge>();
        let [amount] = borrowed_slice(a, n).unwrap() else {
            panic!("charge arity")
        };
        s.calls.set(s.calls.get() + 1);
        s.charged.set(s.charged.get() + amount.as_word().unwrap());
        // Bytes must have installed ownership/finalization and rooted the
        // result before invoking this replaceable accounting message.
        s.id.send_named(s.gc, b"collectPrecisely", &[])?;
        s.original.method.unwrap()(s.original.context, r, a, n).into_result()
    })()
    .map_or_else(|e| e, Outcome::loaded)
}

#[test]
#[ignore = "requires production native modules"]
fn bytes_charge_capacity_and_survive_replaced_accounting() {
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
        let gc = named(b"GC");
        let roots = named(b"Roots");
        let bytes = named(b"Bytes");
        for name in [
            b"chargeExternalAllocation:".as_slice(),
            b"allocationBytes",
            b"prepareAllocation",
            b"bytes",
            b"appendBytes:",
            b"fromBytes:",
        ] {
            id.intern_name(name).unwrap();
        }
        let charged = || send(id, gc, b"allocationBytes", &[]).as_word().unwrap();
        for _ in 0..2 {
            collect(id, gc);
            let value = object(send(
                id,
                bytes,
                b"fromBytes:",
                &[Operand::bytes(&vec![7; 16 << 20])],
            ));
            let pin = retain(id, roots, value);
            assert!(
                charged() >= 16 << 20,
                "external buffer capacity missing from allocation budget"
            );
            send(id, gc, b"prepareAllocation", &[]);
            assert_eq!(
                charged(),
                0,
                "fixed 16 MiB budget must trigger at next checkpoint"
            );
            assert!(present(id, value));
            release(id, roots, pin);
        }
        // Automatic collection remains explicitly suppressible.
        send(id, gc, b"automaticCollection:", &[Operand::word(0)]);
        collect(id, gc);
        send(
            id,
            gc,
            b"chargeExternalAllocation:",
            &[Operand::word(16 << 20)],
        );
        send(id, gc, b"prepareAllocation", &[]);
        assert_eq!(charged(), 16 << 20);
        send(id, gc, b"automaticCollection:", &[Operand::word(1)]);
        send(id, gc, b"prepareAllocation", &[]);
        assert_eq!(charged(), 0);
        assert!(
            id.send_named(gc, b"chargeExternalAllocation:", &[Operand::unit()])
                .is_err()
        );
        send(id, gc, b"chargeExternalAllocation:", &[Operand::word(0)]);
        assert_eq!(charged(), 0);

        let vt = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(gc)],
        ));
        let selector = id.intern_name(b"chargeExternalAllocation:").unwrap();
        let original = send(
            id,
            id.receiver,
            b"bindingOf:in:",
            &[Operand::object(selector), Operand::object(vt)],
        )
        .as_entry()
        .unwrap();
        let mut hook = Charge {
            id,
            gc,
            original,
            calls: Cell::new(0),
            charged: Cell::new(0),
        };
        install(
            id,
            vt,
            b"chargeExternalAllocation:",
            collecting_charge,
            (&mut hook as *mut Charge).cast(),
        );
        let value = object(send(id, bytes, b"fromBytes:", &[Operand::bytes(b"a")]));
        let pin = retain(id, roots, value);
        assert_eq!(hook.charged.get(), 1);
        let suffix = vec![42; 4096];
        send(id, value, b"appendBytes:", &[Operand::bytes(&suffix)]);
        let got = send(id, value, b"bytes", &[]).as_bytes().unwrap();
        let mut expected = vec![b'a'];
        expected.extend_from_slice(&suffix);
        assert_eq!(borrowed_bytes(got.pointer, got.length).unwrap(), expected);
        assert!(hook.charged.get() >= expected.len());
        let before = hook.charged.get();
        send(id, value, b"appendBytes:", &[Operand::bytes(b"")]);
        assert_eq!(
            hook.charged.get(),
            before,
            "unused capacity is not charged twice"
        );
        assert!(
            hook.calls.get() >= 2,
            "factory and append use current accounting"
        );
        install(
            id,
            vt,
            b"chargeExternalAllocation:",
            original.method.unwrap(),
            original.context,
        );
        let calls = hook.calls.get();
        send(id, bytes, b"fromBytes:", &[Operand::bytes(b"restored")]);
        assert_eq!(
            hook.calls.get(),
            calls,
            "accounting replacement must not be cached"
        );
        release(id, roots, pin);
        collect(id, gc);
    }
}
