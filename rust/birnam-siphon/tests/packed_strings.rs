//! String constructors receive byte objects, not expanded Sequences.
#[allow(dead_code)]
#[path = "support/minimal.rs"]
mod minimal;
#[allow(dead_code)]
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{as_size, size};
#[test]
#[ignore = "requires production Oop modules"]
fn packed_strings_preserve_constructors_binary_data_and_gc() {
    let h = minimal::native_floor();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let string = h.named(b"String").unwrap();
    let vt = h.named(b"StringVt").unwrap();
    let gc = h.named(b"GC").unwrap();
    q.read_bytes(q.string(b"initialize reader").unwrap()).unwrap();
    let from = hooks.symbol(b"fromBytes:");
    let original = q.call(vt, b"lookup", &[from]).unwrap();
    let state = hooks.context(&[original, gc, size(0).unwrap()]);
    q.call(vt, b"addMethod", &[from, hooks.method(b"collectEvery", state)]).unwrap();
    for n in [0, 1, 8192, 8193, 1 << 20] {
        let bytes: Vec<_> = (0..n).map(|i| (i % 256) as u8).collect();
        let value = q.string(&bytes).unwrap();
        assert_eq!(q.read_bytes(value).unwrap(), bytes);
        let builder = q.call(h.named(b"StringBuilder").unwrap(), b"new", &[]).unwrap();
        q.call(builder, b"add:", &[value]).unwrap();
        let result = q.call(builder, b"asString", &[]).unwrap();
        assert_eq!(q.read_bytes(result).unwrap(), bytes);
        assert_eq!(q.read_bytes(value).unwrap(), bytes);
    }
    assert_eq!(hooks.count(state, 2), 10, "each explicit construction remains replaceable");
    q.call(vt, b"addMethod", &[from, original]).unwrap();

    let source = q.string(b" \t aa\0aa \r\n").unwrap();
    let needle = q.string(b"aa").unwrap();
    let replacement = q.string(b"\xff").unwrap();
    let empty = q.string(b"").unwrap();
    q.call(vt, b"addMethod", &[from, hooks.method(b"collectEvery", state)]).unwrap();
    let trimmed = q.call(source, b"trimmed", &[]).unwrap();
    assert_eq!(q.read_bytes(trimmed).unwrap(), b"aa\0aa");
    let replaced = q.call(trimmed, b"replace:with:", &[needle, replacement]).unwrap();
    assert_eq!(q.read_bytes(replaced).unwrap(), b"\xff\0\xff");
    let unchanged = q.call(source, b"replace:with:", &[empty, replacement]).unwrap();
    assert_eq!(q.read_bytes(unchanged).unwrap(), b" \t aa\0aa \r\n");
    assert_eq!(q.read_bytes(source).unwrap(), b" \t aa\0aa \r\n");
    q.call(vt, b"addMethod", &[from, original]).unwrap();

    // Internal result construction must not allocate an Oop-per-byte Sequence.
    let input = q.string(b" binary\0\xff ").unwrap();
    let root_vt = q.call(h.id.root, b"behaviorOf:", &[h.id.root]).unwrap();
    let sequence = hooks.symbol(b"sequence");
    let saved = q.call(root_vt, b"lookup", &[sequence]).unwrap();
    let trimmed = hooks.symbol(b"trimmed");
    let add_method = hooks.symbol(b"addMethod");
    let denial = hooks.context(&[hooks.fault(71), size(0).unwrap()]);
    q.call(root_vt, b"addMethod", &[sequence, hooks.method(b"deny", denial)]).unwrap();
    let result = unsafe { h.id.send(input, trimmed, &[]) };
    unsafe { h.id.send(root_vt, add_method, &[sequence, saved]) }.unwrap();
    let result = q.keep(result.unwrap()).unwrap();
    assert_eq!(q.read_bytes(result).unwrap(), b"binary\0\xff");
    assert_eq!(hooks.count(denial, 1), 0);
    // Native String reads use its bulk operation, without constructing Bytes.
    let bytes = hooks.symbol(b"bytes");
    let saved_bytes = q.call(vt, b"lookup", &[bytes]).unwrap();
    q.call(vt, b"addMethod", &[bytes, hooks.method(b"deny", denial)]).unwrap();
    let read = q.string_bytes(input);
    q.call(vt, b"addMethod", &[bytes, saved_bytes]).unwrap();
    assert_eq!(read.unwrap(), b" binary\0\xff ");
    assert_eq!(hooks.count(denial, 1), 0);
    assert_eq!(as_size(q.call(string, b"sizeWord", &[]).unwrap()), Some(0));
}
