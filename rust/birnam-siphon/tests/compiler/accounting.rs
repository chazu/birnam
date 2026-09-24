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
        send(
            h.id,
            h.id.receiver,
            b"allocationsDo:",
            &[Operand::entry(NativeEntry::new(
                allocation,
                ptr::from_mut(&mut entries).cast(),
            ))],
        );
        entries
    }
}
pub(super) unsafe fn check(h: &mut H) {
    unsafe {
        h.clear();
        h.collect();
        let gc = h.named(b"GC");
        send(h.id, gc, b"budget:", &[Operand::word(usize::MAX)]);
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
            Some(expected),
            "parsing, generated Method/candidate construction and Machine execution charge their actual allocations"
        );
        h.clear();
        h.collect();
        assert_eq!(send(h.id, gc, b"allocationBytes", &[]).as_word(), Some(0));
        assert_eq!(h.units(), 0);
        assert_eq!(h.native_bytes(), 0);
        send(h.id, gc, b"budget:", &[Operand::word(1 << 20)]);
    }
}
