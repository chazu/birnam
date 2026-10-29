//! Fixed-width image fields preserve exact bits and current owner operations.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{size, Oop};
#[test]
#[ignore = "requires production ImageFormat"]
fn image_integer_fields_preserve_full_width_bits_and_live_builder() {
    let h = oop::bootstrap();
    let q = h.scope();
    let hooks = oop::hooks::Hooks::new(&h);
    let named = |n: &[u8]| h.named(n).unwrap();
    let key = |n: &[u8]| hooks.symbol(n);
    let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
    let format = h.global(&q, b"ImageFormat").unwrap();
    let worker = call(format, b"worker", &[]);
    let number = |v: u64| {
        call(
            named(b"Integer"),
            b"fromWord:",
            &[q.word(v as usize).unwrap()],
        )
    };
    for value in [
        0u64,
        1,
        255,
        256,
        u32::MAX as u64,
        (1u64 << 53) + 1,
        i64::MAX as u64,
        1u64 << 63,
        (1u64 << 63) + 1,
        u64::MAX,
    ] {
        for width in [1, 4, 8] {
            let builder = call(named(b"Bytes"), b"fromByteValues", &[]);
            assert_eq!(
                call(
                    worker,
                    b"write:width:on:",
                    &[number(value), number(width), builder]
                ),
                builder
            );
            assert_eq!(
                q.read_bytes(call(builder, b"asString", &[])).unwrap(),
                value.to_le_bytes()[..width as usize]
            );
        }
    }
    let value = number(0xaabb);
    let builder = call(named(b"Bytes"), b"fromByteValues", &[]);
    let bv = call(h.id.root, b"behaviorOf:", &[builder]);
    let selector = key(b"appendByteValues");
    let original = call(bv, b"lookup", &[selector]);
    let state = hooks.context(&[
        original,
        named(b"GC"),
        size(0).unwrap(),
        std::ptr::null_mut(),
    ]);
    call(
        bv,
        b"addMethod",
        &[selector, hooks.method(b"observed", state)],
    );
    let result = h.send(worker, b"write:width:on:", &[value, number(8), builder]);
    call(bv, b"addMethod", &[selector, original]);
    assert_eq!(result.unwrap(), builder);
    assert_eq!(hooks.count(state, 2), 8);
    assert_eq!(
        q.read_bytes(call(builder, b"asString", &[])).unwrap(),
        0xaabbu64.to_le_bytes()
    );
    for name in [
        "image-empty-stripped.bin",
        "image-block-binding-stripped.bin",
        "image-mixed-code.bin",
        "image-mixed-facts-stripped.bin",
        "image-mixed-stripped.bin",
        "image-shared-cyclic-stripped.bin",
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/portable-code-v1")
            .join(name);
        let input = std::fs::read(path).unwrap();
        let bytes = call(
            named(b"Bytes"),
            b"fromBytes:",
            &[q.byte_values(&input).unwrap()],
        );
        let abi = call(bytes, b"readU64At:", &[number(20)]);
        let decoded = call(
            format,
            b"inspect:architecture:abi:",
            &[bytes, number(1), abi],
        );
        q.check_result(decoded).unwrap();
        let mut fields = call(named(b"Vector"), b"new", &[]);
        for value in [number(1), abi] {
            fields = call(fields, b"add:", &[value]);
        }
        for i in 0..q.vector_size(decoded).unwrap() {
            let mut value = q.vector_at(decoded, i).unwrap();
            if i == 5 && value != named(b"nil") {
                value = call(
                    value,
                    b"copyFrom:to:",
                    &[number(72), call(value, b"size", &[])],
                );
            }
            fields = call(fields, b"add:", &[value]);
        }
        let encoded = call(format, b"compose:", &[fields]);
        q.check_result(encoded).unwrap();
        assert_eq!(
            q.read_bytes(encoded).unwrap(),
            input,
            "frozen fixture {name}"
        );
    }
}
