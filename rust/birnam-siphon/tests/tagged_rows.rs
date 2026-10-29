//! Schema-tagged table data stays with Bytes and uses current numeric owners.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, size};
#[test]
#[ignore = "requires production Oop owners"]
fn tagged_tables_preserve_positions_and_live_numeric_owners() {
    let host = oop::bootstrap();
    let q = host.scope();
    let hooks = oop::hooks::Hooks::new(&host);
    let named = |n: &[u8]| host.named(n).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let gc = named(b"GC");
    call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let integer = named(b"Integer");
    let number = |n| call(integer, b"fromWord:", &[size(n).unwrap()]);
    let read = |o| q.word_bits(call(integer, b"wordOf:", &[o])).unwrap();
    let mut data = 2u32.to_le_bytes().to_vec();
    data.extend([0]);
    data.extend(2u32.to_le_bytes());
    data.extend([0, 2]);
    data.extend(1u32.to_le_bytes());
    data.extend([2]);
    data.extend(1u32.to_le_bytes());
    data.extend(1u32.to_le_bytes());
    data.extend([2]);
    data.extend(0u32.to_le_bytes());
    let blob = call(
        named(b"Bytes"),
        b"fromBytes:",
        &[q.byte_values(&data).unwrap()],
    );
    let schema = q.string(b"012").unwrap();
    let mask = q.string(b"100").unwrap();
    let refs = q.string(b"001").unwrap();
    let two = number(2);
    let zero = number(0);
    let nil = named(b"nil");
    let rows = call(blob, b"readTaggedTableWithSchema:limit:", &[schema, two]);
    q.check_result(rows).unwrap();
    assert_eq!(read(call(rows, b"firstLocatorOutside:", &[mask])), 1);
    assert_eq!(call(rows, b"firstIndexForTags:atLeast:", &[refs, two]), nil);
    let selected = call(rows, b"whereLocatorTags:", &[refs]);
    let row = call(selected, b"at:", &[zero]);
    assert_eq!(read(call(row, b"sourceIndex", &[])), 1);
    let joined = call(selected, b"concat:", &[rows]);
    assert_eq!(
        read(call(joined, b"firstLocatorOutside:", &[mask])),
        0,
        "concat creates a new base ordering while selection preserves source position"
    );
    let vt = named(b"IntegerVt");
    let word_of = key(b"wordOf:");
    let from_word = key(b"fromWord:");
    let original = call(vt, b"lookup", &[word_of]);
    let state = hooks.context(&[original, gc, size(0).unwrap()]);
    call(
        vt,
        b"addMethod",
        &[word_of, hooks.method(b"collectEvery", state)],
    );
    assert_eq!(call(rows, b"firstIndexForTags:atLeast:", &[refs, two]), nil);
    assert!(hooks.count(state, 2) > 0);
    call(
        vt,
        b"addMethod",
        &[word_of, hooks.method(b"fixed", size(0).unwrap())],
    );
    let changed = call(rows, b"firstIndexForTags:atLeast:", &[refs, two]);
    assert_ne!(
        changed, nil,
        "the bound must come from the current Integer owner"
    );
    call(vt, b"addMethod", &[word_of, original]);
    let original = call(vt, b"lookup", &[from_word]);
    let state = hooks.context(&[original, gc, size(0).unwrap()]);
    call(
        vt,
        b"addMethod",
        &[from_word, hooks.method(b"collectEvery", state)],
    );
    let position = call(row, b"sourceIndex", &[]);
    assert!(hooks.count(state, 2) > 0);
    assert_eq!(read(position), 1);
    call(vt, b"addMethod", &[from_word, original]);

    // Exercise the five-argument operation with collection during each numeric
    // conversion. Invalid arities and null arguments must fail before callbacks.
    let records = call(named(b"Bytes"), b"fromByteValues",
        &[99, 8, 9, 2, 7, 0, 0, 0, 4, 5, 0].map(|v| size(v).unwrap()));
    let one = number(1);
    let record_schema = q.string(b"002").unwrap();
    let args = [one, two, two, one, record_schema];
    let selector = b"readTaggedRecordsAt:count:prefixBytes:fields:schema:";
    let original = call(vt, b"lookup", &[word_of]);
    let state = hooks.context(&[original, gc, size(0).unwrap()]);
    call(vt, b"addMethod", &[word_of, hooks.method(b"collectEvery", state)]);
    let result = call(records, selector, &args);
    q.check_result(result).unwrap();
    assert!(hooks.count(state, 2) >= 4);
    let before = hooks.count(state, 2);
    for invalid_args in [&args[..0], &args[..4], &[one, two, two, one, record_schema, nil][..],
                         &[one, two, two, one, std::ptr::null_mut()][..]] {
        assert!(host.send(records, selector, invalid_args).is_err());
    }
    assert_eq!(hooks.count(state, 2), before);
    call(vt, b"addMethod", &[word_of, original]);
    assert_eq!(read(call(result, b"size", &[])), 2);
    let row = call(result, b"at:", &[zero]);
    let fields = call(row, b"at:", &[one]);
    let field = call(fields, b"at:", &[zero]);
    assert_eq!(read(call(field, b"at:", &[one])), 7);
}
