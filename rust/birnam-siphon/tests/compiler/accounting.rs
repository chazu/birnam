//! Cross-check production compiler/execution charges against physical Id records.
//! This oracle does not call GC's accounting helper or Id's charge-size query.
use super::*;
use std::collections::BTreeMap;

unsafe extern "C" fn allocation(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let [_, size, identity] = borrowed_slice(a, n).unwrap() else {
            panic!("allocation inventory");
        };
        let entries = &mut *c.cast::<BTreeMap<usize, usize>>();
        assert!(
            entries
                .insert(identity.as_word().unwrap(), size.as_word().unwrap())
                .is_none()
        );
        Outcome::loaded(Operand::unit())
    }
}
unsafe fn inventory(h: &H) -> BTreeMap<usize, usize> {
    unsafe {
        let mut entries = BTreeMap::new();
        support::inventory(h.id, allocation, ptr::from_mut(&mut entries).cast());
        entries
    }
}
struct ExternalCharge {
    total: Cell<usize>,
    original: NativeEntry,
}
unsafe extern "C" fn external_charge(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    unsafe {
        let observer = &*c.cast::<ExternalCharge>();
        let [bytes] = borrowed_slice(a, n).unwrap() else {
            panic!("external charge")
        };
        let result = observer.original.method.unwrap()(observer.original.context, r, a, n);
        if result.into_result().is_ok() {
            observer
                .total
                .set(observer.total.get() + bytes.as_word().unwrap());
        }
        result
    }
}
pub(super) unsafe fn check(h: &mut H) {
    unsafe {
        h.clear();
        h.collect();
        let gc = h.named(b"GC");
        send(h.id, gc, b"budget:", &[Operand::word(usize::MAX)]);
        let vt = object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(gc)],
        ));
        let selector = h.id.intern_name(b"chargeExternalAllocation:").unwrap();
        let original = send(
            h.id,
            h.id.receiver,
            b"bindingOf:in:",
            &[Operand::object(selector), Operand::object(vt)],
        )
        .as_entry()
        .unwrap();
        let mut external = ExternalCharge {
            total: Cell::new(0),
            original,
        };
        install(
            h.id,
            vt,
            b"chargeExternalAllocation:",
            external_charge,
            (&mut external as *mut ExternalCharge).cast(),
        );
        let before = inventory(h);
        assert_eq!(send(h.id, gc, b"allocationBytes", &[]).as_word(), Some(0));
        let result = h.eval(b"let chargeInput := 35; [[:x | [chargeInput + x]] value: 7]");
        assert_eq!(h.integer(result), 42);
        let after = inventory(h);
        assert!(
            before.keys().all(|id| after.contains_key(id)),
            "no collection during the measured source path"
        );
        // Independent 64-bit oracle: an 8-byte header and 8-byte payload padding.
        let expected: usize = after
            .iter()
            .filter(|(id, _)| !before.contains_key(id))
            .map(|(_, size)| (size + 8 + 7) & !7)
            .sum();
        assert!(expected > 0);
        assert_eq!(
            send(h.id, gc, b"allocationBytes", &[]).as_word(),
            Some(expected + external.total.get()),
            "parsing, generated Method construction and Machine execution charge their actual allocations"
        );
        assert!(
            external.total.get() > 0,
            "source compilation creates external Bytes storage"
        );
        install(
            h.id,
            vt,
            b"chargeExternalAllocation:",
            original.method.unwrap(),
            original.context,
        );
        h.clear();
        h.collect();
        assert_eq!(send(h.id, gc, b"allocationBytes", &[]).as_word(), Some(0));
        assert_eq!(h.units(), 0);
        assert_eq!(h.native_bytes(), 0);
        send(h.id, gc, b"budget:", &[Operand::word(1 << 20)]);
    }
}
