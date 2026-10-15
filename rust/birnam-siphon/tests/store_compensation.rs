//! Candidate-owned inverse capacity and exact one-key compensation.
mod support;
use birnam_abi::{
    Oop,
    id_protocol::{Operand, UNIT},
};
use std::os::unix::ffi::OsStrExt;
use support::*;
#[test]
#[ignore = "scripts/native-message-test.sh builds production native modules"]
fn reserved_compensation_survives_capacity_exhaustion_and_preserves_other_facts() {
    unsafe {
        // This test executable has one VM and one test; configure before loading
        // Store, which reads its immutable resource limit at construction.
        std::env::set_var("BIRNAM_STORE_FACT_BYTES_LIMIT", "16384");
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
        let keep = |o: Oop| {
            retain(id, named(b"Roots"), o);
            o
        };
        let sym = |name: &[u8]| keep(id.intern_name(name).unwrap());
        let call = |r: Oop, name: &[u8], args: &[Oop]| {
            keep(object(send(
                id,
                r,
                name,
                &args
                    .iter()
                    .copied()
                    .map(Operand::object)
                    .collect::<Vec<_>>(),
            )))
        };
        let integer = |n| {
            keep(object(send(
                id,
                named(b"Integer"),
                b"fromWord:",
                &[Operand::word(n)],
            )))
        };
        let number = |o| {
            send(id, named(b"Integer"), b"wordOf:", &[Operand::object(o)])
                .as_word()
                .unwrap()
        };
        let db = named(b"db");
        // Bootstrap may have compacted seed facts. Retain from the current
        // available basis rather than requesting already-discarded history.
        let basis = call(db, b"basis", &[]);
        assert_eq!(call(db, b"retainSince:", &[basis]), db);
        let yes = named(b"true");
        let no = named(b"false");
        let owner = object(send(id, named(b"Object"), b"clone", &[]));
        let owner_root = retain(id, named(b"Roots"), owner);
        let entity = sym(b"candidateOwned");
        let attr = sym(b"value");
        let fill = sym(b"unrelatedHistory");
        let old = integer(10);
        let new = integer(20);
        call(db, b"assert:attr:value:", &[attr, sym(b"history"), yes]);
        call(db, b"assert:attr:value:", &[fill, sym(b"history"), yes]);
        call(db, b"assert:attr:value:", &[entity, attr, old]);
        assert_eq!(
            call(db, b"reserveCompensationFor:count:", &[owner, integer(1)]),
            db
        );
        release(id, named(b"Roots"), owner_root);
        assert_eq!(call(db, b"assert:attr:value:", &[entity, attr, new]), db);
        let mut last = old;
        let mut exhausted = false;
        for i in 0..2000 {
            let value = integer(i);
            let result = call(db, b"assert:attr:value:", &[entity, fill, value]);
            if call(result, b"isError", &[]) == yes {
                assert_eq!(call(result, b"kind", &[]), sym(b"historyResourceLimit"));
                exhausted = true;
                break;
            }
            last = value;
        }
        assert!(exhausted);
        assert_eq!(number(call(db, b"reservedCompensationFactCount", &[])), 1);
        collect(id, named(b"GC"));
        assert_eq!(
            send(
                id,
                id.receiver,
                b"allocationSizeOf:",
                &[Operand::object(owner)]
            )
            .as_word(),
            Some(0)
        );
        keep(owner); // Further checks reuse the owner after its reservation is consumed.

        // Stale compare does not restore or spend the inverse reservation.
        let args = [
            Operand::object(entity),
            Operand::object(attr),
            Operand::object(old),
            Operand::object(old),
            Operand::object(owner),
        ];
        let rejected = keep(object(send(
            id,
            db,
            b"restoreValue:attr:expected:previous:reservation:",
            &args,
        )));
        assert_eq!(call(rejected, b"kind", &[]), sym(b"compensationConflict"));
        assert_eq!(number(call(db, b"reservedCompensationFactCount", &[])), 1);
        let args = [
            Operand::object(entity),
            Operand::object(attr),
            Operand::object(new),
            Operand::object(old),
            Operand::object(owner),
        ];
        assert_eq!(
            object(send(
                id,
                db,
                b"restoreValue:attr:expected:previous:reservation:",
                &args
            )),
            db
        );
        assert_eq!(call(db, b"currentValue:attr:", &[entity, attr]), old);
        assert_eq!(call(db, b"currentValue:attr:", &[entity, fill]), last);
        assert_eq!(number(call(db, b"reservedCompensationFactCount", &[])), 0);
        // Release is idempotent and affects only this owner's reservation.
        call(db, b"retainCurrentOnly", &[]);
        call(db, b"compact", &[]);
        let other = call(named(b"Object"), b"clone", &[]);
        assert_eq!(
            call(db, b"reserveCompensationFor:count:", &[owner, integer(2)]),
            db
        );
        assert_eq!(
            call(db, b"reserveCompensationFor:count:", &[other, integer(1)]),
            db
        );
        call(db, b"releaseCompensationFor:", &[owner]);
        call(db, b"releaseCompensationFor:", &[owner]);
        assert_eq!(number(call(db, b"reservedCompensationFactCount", &[])), 1);
        // Unit is absent; nil is a present language value, and must remain
        // distinguishable when an added Global is undone.
        let nullable = sym(b"nullable");
        let nil = named(b"nil");
        call(db, b"assert:attr:value:", &[entity, nullable, nil]);
        let args = [
            Operand::object(entity),
            Operand::object(nullable),
            Operand::object(nil),
            Operand::unit(),
            Operand::object(other),
        ];
        assert_eq!(
            object(send(
                id,
                db,
                b"restoreValue:attr:expected:previous:reservation:",
                &args
            )),
            db
        );
        assert_eq!(
            send(
                id,
                db,
                b"rawCurrentValue:attr:",
                &[Operand::object(entity), Operand::object(nullable)]
            )
            .kind(),
            UNIT
        );
        assert_eq!(call(db, b"hasValue:attr:", &[entity, nullable]), no);
        assert_eq!(number(call(db, b"reservedCompensationFactCount", &[])), 0);
    }
}
