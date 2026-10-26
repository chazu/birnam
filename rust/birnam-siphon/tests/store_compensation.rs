//! Candidate-owned inverse capacity and exact one-key compensation through Oops.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{as_size, size};

#[test]
#[ignore = "requires production Oop modules"]
fn reserved_compensation_survives_capacity_exhaustion_and_preserves_other_facts() {
    // This executable has one runtime/test. Store reads this immutable limit
    // when its optional native module is loaded below.
    unsafe {
        std::env::set_var("BIRNAM_STORE_FACT_BYTES_LIMIT", "16384");
    }
    let h = oop::bootstrap();
    let q = h.scope();
    let db = h
        .load_native(
            &q,
            &h.resources.lib_dir.join("native/libbirnam_store.dylib"),
            &[h.namespace],
        )
        .unwrap();
    q.check_result(db).unwrap();
    let named = |name: &[u8]| h.named(name).unwrap();
    let sym = |name: &[u8]| q.keep(h.intern(name).unwrap()).unwrap();
    let call = |r, name: &[u8], args: &[_]| q.call(r, name, args).unwrap();
    let integer = |n| call(named(b"Integer"), b"fromWord:", &[size(n).unwrap()]);
    let number = |o| as_size(call(named(b"Integer"), b"wordOf:", &[o])).unwrap();
    let yes = named(b"true");
    let no = named(b"false");
    let basis = call(db, b"basis", &[]);
    assert_eq!(call(db, b"retainSince:", &[basis]), db);
    let owner_scope = h.scope();
    let owner = owner_scope.call(named(b"Object"), b"clone", &[]).unwrap();
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
    drop(owner_scope);
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
    call(named(b"GC"), b"collectPrecisely", &[]);
    assert_eq!(
        as_size(call(h.id.root, b"allocationSizeOf:", &[owner])),
        Some(0)
    );
    q.keep(owner).unwrap(); // Reused after consuming its reservation.
    let restore = b"restoreValue:attr:expected:previous:reservation:";
    let rejected = call(db, restore, &[entity, attr, old, old, owner]);
    assert_eq!(call(rejected, b"kind", &[]), sym(b"compensationConflict"));
    assert_eq!(number(call(db, b"reservedCompensationFactCount", &[])), 1);
    assert_eq!(call(db, restore, &[entity, attr, new, old, owner]), db);
    assert_eq!(call(db, b"currentValue:attr:", &[entity, attr]), old);
    assert_eq!(call(db, b"currentValue:attr:", &[entity, fill]), last);
    assert_eq!(number(call(db, b"reservedCompensationFactCount", &[])), 0);
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
    // The null Oop denotes absence; language nil is an actual stored value.
    let nullable = sym(b"nullable");
    let nil = named(b"nil");
    call(db, b"assert:attr:value:", &[entity, nullable, nil]);
    assert_eq!(
        call(
            db,
            restore,
            &[entity, nullable, nil, std::ptr::null_mut(), other]
        ),
        db
    );
    assert!(call(db, b"rawCurrentValue:attr:", &[entity, nullable]).is_null());
    assert_eq!(call(db, b"hasValue:attr:", &[entity, nullable]), no);
    assert_eq!(number(call(db, b"reservedCompensationFactCount", &[])), 0);
}
