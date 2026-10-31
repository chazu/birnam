//! A Unit's native reference entry preserves values and propagates trace errors.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::size;

#[test]
#[ignore = "requires production TCC and GC"]
fn native_unit_reference_entries_are_checked_and_keep_values_alive() {
    let h = oop::bootstrap();
    let q = h.scope();
    let named = |name: &[u8]| h.named(name).unwrap();
    let key = |name: &[u8]| q.keep(h.intern(name).unwrap()).unwrap();
    let call = |r, name: &[u8], args: &[_]| q.call(r, name, args).unwrap();
    let source = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/oop-continuation/tcc-context.c"),
    )
    .unwrap();
    let unit = call(named(b"TCC"), b"compile:", &[q.string(&source).unwrap()]);
    q.check_result(unit).unwrap();
    assert!(
        h.send(unit, b"traceRootsWithEntry:", &[size(5).unwrap()])
            .is_err()
    );
    assert_eq!(call(unit, b"initialize", &[]), unit);
    assert!(
        h.send(unit, b"traceRootsWithEntry:", &[size(2).unwrap()])
            .is_err(),
        "continuation entries cannot report native roots"
    );
    assert!(
        h.send(unit, b"traceRootsWithEntry:", &[size(7).unwrap()])
            .is_err()
    );
    let epoch = call(h.id.root, b"bindingEpoch", &[]);
    assert_eq!(
        call(unit, b"traceRootsWithEntry:", &[size(5).unwrap()]),
        unit
    );
    assert_eq!(call(h.id.root, b"bindingEpoch", &[]), epoch);
    let object = call(named(b"Object"), b"clone", &[]);
    let vt = call(h.id.root, b"ensurePrivateVtableFor:", &[object]);
    for (name, index) in [(b"hold:".as_slice(), 4), (b"fail:", 6)] {
        let method = call(unit, b"declaredMethodAt:", &[size(index).unwrap()]);
        call(vt, b"addMethod", &[key(name), method]);
    }
    let gc = named(b"GC");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let identity = key(b"identityOf:");
    let collect = key(b"collectPrecisely");
    let hold = key(b"hold:");
    let (value, stamp) = {
        let temporary = h.scope();
        let value = temporary.call(named(b"Object"), b"clone", &[]).unwrap();
        let stamp = unsafe { h.id.send(h.id.root, identity, &[value]) }.unwrap();
        temporary.call(object, b"hold:", &[value]).unwrap();
        (value, stamp)
    };
    call(gc, b"collectPrecisely", &[]);
    assert_eq!(
        unsafe { h.id.send(h.id.root, identity, &[value]) }.unwrap(),
        stamp
    );
    call(object, b"fail:", &[size(1).unwrap()]);
    assert!(unsafe { h.id.send(gc, collect, &[]) }.is_err());
    call(object, b"fail:", &[size(0).unwrap()]);
    call(gc, b"collectPrecisely", &[]);
    assert_eq!(
        unsafe { h.id.send(h.id.root, identity, &[value]) }.unwrap(),
        stamp
    );
    unsafe { h.id.send(object, hold, &[std::ptr::null_mut()]) }.unwrap();
    call(gc, b"collectPrecisely", &[]);
    assert!(unsafe { h.id.send(h.id.root, identity, &[value]) }.is_err());
}
