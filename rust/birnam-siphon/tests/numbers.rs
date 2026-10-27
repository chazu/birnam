//! Production numeric values checked against host arithmetic and live owner messages.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use birnam_host_support::{Context, Scope};
use std::path::Path;

struct H<'a> {
    host: &'a Context,
    q: Scope<'a>,
    integer: Oop,
    float: Oop,
    nil: Oop,
    truth: Oop,
}
impl H<'_> {
    fn call(&self, r: Oop, name: &[u8], args: &[Oop]) -> Oop {
        self.q.call(r, name, args).unwrap()
    }
    fn i(&self, v: i64) -> Oop {
        self.call(
            self.integer,
            b"fromWord:",
            &[self.q.word(v as usize).unwrap()],
        )
    }
    fn f(&self, v: f64) -> Oop {
        self.call(
            self.float,
            b"fromBits:",
            &[self.q.word(v.to_bits() as usize).unwrap()],
        )
    }
    fn s(&self, v: &[u8]) -> Oop {
        self.q.string(v).unwrap()
    }
    fn iv(&self, v: Oop) -> i64 {
        self.q.word_bits(self.call(v, b"word", &[])).unwrap() as i64
    }
    fn fv(&self, v: Oop) -> f64 {
        f64::from_bits(self.q.word_bits(self.call(v, b"bits", &[])).unwrap() as u64)
    }
    fn text(&self, v: Oop) -> Vec<u8> {
        self.q.string_bytes(v).unwrap()
    }
    fn fault(&self, v: Oop, kind: &[u8]) {
        assert_eq!(self.call(v, b"isError", &[]), self.truth);
        assert_eq!(self.call(v, b"kind", &[]), self.host.intern(kind).unwrap());
    }
}
fn same(actual: f64, expected: f64) {
    if expected.is_nan() {
        assert!(actual.is_nan());
    } else {
        assert_eq!(actual.to_bits(), expected.to_bits());
    }
}

#[test]
#[ignore = "requires production Oop modules"]
fn numbers_text_and_live_dependencies_survive_collection() {
    let host = oop::bootstrap();
    let h = H {
        host: &host,
        q: host.scope(),
        integer: host.named(b"Integer").unwrap(),
        float: host.named(b"Float").unwrap(),
        nil: host.named(b"nil").unwrap(),
        truth: host.named(b"true").unwrap(),
    };
    let hooks = oop::hooks::Hooks::new(&host);
    let gc = host.named(b"GC").unwrap();
    let string = host.named(b"String").unwrap();
    h.call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    for (name, expected) in [("integer", h.integer), ("float", h.float)] {
        assert_eq!(
            host.load_native(
                &h.q,
                &host
                    .resources
                    .lib_dir
                    .join(format!("libbirnam_{name}.dylib")),
                &[host.namespace]
            )
            .unwrap(),
            expected
        );
    }
    let ints = [
        i64::MIN,
        i64::MIN + 1,
        -9_007_199_254_740_993,
        -17,
        -1,
        0,
        1,
        17,
        9_007_199_254_740_993,
        i64::MAX - 1,
        i64::MAX,
    ];
    let operators = [b"+".as_slice(), b"-", b"*", b"/", b"mod:"];
    for a in ints {
        let left = h.i(a);
        assert_eq!(h.iv(left), a);
        assert_eq!(
            h.text(h.call(left, b"printString", &[])),
            a.to_string().as_bytes()
        );
        assert_eq!(h.iv(h.call(left, b"clone", &[])), a);
        for b in ints {
            let right = h.i(b);
            for (op, name) in operators.into_iter().enumerate() {
                let result = h.call(left, name, &[right]);
                let expected = match op {
                    0 => a.checked_add(b),
                    1 => a.checked_sub(b),
                    2 => a.checked_mul(b),
                    3 => a.checked_div(b),
                    4 if a == i64::MIN && b == -1 => Some(0),
                    4 => a.checked_rem(b),
                    _ => unreachable!(),
                };
                if let Some(value) = expected {
                    assert_eq!(h.iv(result), value);
                } else {
                    h.fault(
                        result,
                        if op >= 3 && b == 0 {
                            b"zeroDivide"
                        } else {
                            b"overflow"
                        },
                    );
                }
            }
            for (name, expected) in [
                (b"=".as_slice(), a == b),
                (b"<", (a as f64) < (b as f64)),
                (b"<=", (a as f64) <= (b as f64)),
                (b">", (a as f64) > (b as f64)),
                (b">=", (a as f64) >= (b as f64)),
            ] {
                assert_eq!(h.call(left, name, &[right]) == h.truth, expected);
            }
        }
    }
    let floats = [
        f64::NEG_INFINITY,
        -9_223_372_036_854_775_808.0,
        -17.5,
        -1.0,
        -0.0,
        0.0,
        f64::from_bits(1),
        0.5,
        17.5,
        9_223_372_036_854_775_808.0,
        f64::INFINITY,
        f64::from_bits(0xfff8_0000_0000_0042),
    ];
    for a in floats {
        let left = h.f(a);
        assert_eq!(h.fv(left).to_bits(), a.to_bits());
        same(h.fv(h.call(left, b"clone", &[])), a);
        same(h.fv(h.call(left, b"negated", &[])), -a);
        same(h.fv(h.call(left, b"abs", &[])), a.abs());
        let root = h.call(left, b"sqrt", &[]);
        if a < 0.0 {
            h.fault(root, b"domain");
        } else {
            same(h.fv(root), a.sqrt());
        }
        for b in floats {
            let right = h.f(b);
            for (op, name) in operators[..4].iter().enumerate() {
                let result = h.call(left, name, &[right]);
                if op == 3 && b == 0.0 {
                    h.fault(result, b"zeroDivide");
                } else {
                    same(
                        h.fv(result),
                        match op {
                            0 => a + b,
                            1 => a - b,
                            2 => a * b,
                            _ => a / b,
                        },
                    );
                }
            }
            for (name, expected) in [
                (b"=".as_slice(), a == b),
                (b"<", a < b),
                (b"<=", a <= b),
                (b">", a > b),
                (b">=", a >= b),
            ] {
                assert_eq!(h.call(left, name, &[right]) == h.truth, expected);
            }
        }
        for b in [-17, 0, 17, i64::MAX] {
            let integer = h.i(b);
            for (op, name) in operators[..4].iter().enumerate() {
                for (receiver, right, x, y) in
                    [(left, integer, a, b as f64), (integer, left, b as f64, a)]
                {
                    let result = h.call(receiver, name, &[right]);
                    if op == 3 && y == 0.0 {
                        h.fault(result, b"zeroDivide");
                    } else {
                        same(
                            h.fv(result),
                            match op {
                                0 => x + y,
                                1 => x - y,
                                2 => x * y,
                                _ => x / y,
                            },
                        );
                    }
                }
            }
            assert_ne!(h.call(integer, b"=", &[left]), h.truth);
            assert_ne!(h.call(left, b"=", &[integer]), h.truth);
            same(h.fv(h.call(integer, b"asFloat", &[])), b as f64);
        }
    }
    for a in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -0.0,
        0.49,
        0.5,
        -0.5,
        1.5,
        -1.5,
        -9_223_372_036_854_775_808.0,
        9_223_372_036_854_775_808.0,
        f64::from_bits(9_223_372_036_854_775_808.0f64.to_bits() - 1),
    ] {
        let left = h.f(a);
        for (name, expected) in [
            (b"asInteger".as_slice(), a.trunc()),
            (b"floor", a.floor()),
            (b"ceiling", a.ceil()),
            (b"rounded", a.round()),
        ] {
            let value = h.call(left, name, &[]);
            if !expected.is_finite()
                || expected < i64::MIN as f64
                || expected >= 9_223_372_036_854_775_808.0
            {
                h.fault(value, b"overflow");
            } else {
                assert_eq!(h.iv(value), expected as i64);
            }
        }
    }
    for (value, text) in [
        (0.0, b"0.0".as_slice()),
        (-0.0, b"-0.0"),
        (2.0, b"2.0"),
        (f64::NAN, b"NaN.0"),
        (f64::INFINITY, b"inf.0"),
    ] {
        assert_eq!(h.text(h.call(h.f(value), b"printString", &[])), text);
    }
    let fault = h.call(
        host.named(b"Error").unwrap(),
        b"message:",
        &[h.s(b"incoming")],
    );
    for value in [h.i(4), h.f(4.0)] {
        for name in [b"+".as_slice(), b"<", b"/"] {
            assert_eq!(h.call(value, name, &[fault]), fault);
            h.fault(h.call(value, name, &[h.nil]), b"typeMismatch");
        }
        assert_ne!(h.call(value, b"=", &[fault]), h.truth);
    }
    assert!(h.call(h.integer, b"wordOf:", &[h.float]).is_null());
    assert!(h.call(h.float, b"bitsOf:", &[h.integer]).is_null());
    let text = h.s(&[b'a', 0, 255, b'a']);
    assert_eq!(h.iv(h.call(text, b"size", &[])), 4);
    assert_eq!(h.iv(h.call(text, b"byteAt:", &[h.i(2)])), 255);
    assert_eq!(h.text(h.call(text, b"at:", &[h.i(1)])), [0]);
    for index in [-1, 4, i64::MAX] {
        assert_eq!(h.call(text, b"at:", &[h.i(index)]), h.nil);
    }
    assert_eq!(h.call(text, b"at:", &[h.f(1.0)]), h.nil);
    assert_eq!(
        h.text(h.call(text, b"copyFrom:to:", &[h.i(1), h.i(3)])),
        [0, 255]
    );
    assert_eq!(h.call(text, b"copyFrom:to:", &[h.i(3), h.i(2)]), h.nil);
    assert_eq!(h.iv(h.call(text, b"indexOf:", &[h.s(&[0, 255])])), 1);
    assert_eq!(h.iv(h.call(text, b"indexOf:", &[string])), 0);
    assert_eq!(h.call(text, b"indexOf:", &[h.s(b"no")]), h.nil);
    assert_eq!(
        h.iv(h.call(h.s(b"[ { \"[\\\"}\" // }\n]"), b"bracketDepth", &[])),
        1
    );
    for (source, value) in [
        (b" \t-9223372036854775808\n".as_slice(), i64::MIN),
        (b"9223372036854775807", i64::MAX),
        (b"+17", 17),
    ] {
        assert_eq!(h.iv(h.call(h.s(source), b"asInteger", &[])), value);
    }
    for source in [
        b"9223372036854775808".as_slice(),
        b"1.5",
        b"",
        b"\x0b1",
        b"1\0",
        b"\xff",
    ] {
        assert_eq!(h.call(h.s(source), b"asInteger", &[]), h.nil);
    }
    assert_eq!(h.iv(h.call(h.s(b"17"), b"asNumber", &[])), 17);
    for (source, value) in [
        (b"1.5".as_slice(), 1.5),
        (b"1e2", 100.0),
        (b"9223372036854775808", 9_223_372_036_854_775_808.0),
        (b"NaN", f64::NAN),
    ] {
        same(h.fv(h.call(h.s(source), b"asNumber", &[])), value);
    }
    let builder = h.call(host.named(b"StringBuilder").unwrap(), b"new", &[]);
    for value in [h.i(321), h.i(-1), h.nil] {
        h.call(builder, b"addByte:", &[value]);
    }
    assert_eq!(h.iv(h.call(builder, b"size", &[])), 2);
    assert_eq!(h.text(h.call(builder, b"asString", &[])), [65, 255]);
    for (behavior, name) in [
        (host.named(b"IntegerVt").unwrap(), b"wordOf:".as_slice()),
        (host.named(b"IntegerVt").unwrap(), b"fromWord:"),
        (host.named(b"FloatVt").unwrap(), b"bitsOf:"),
        (host.named(b"FloatVt").unwrap(), b"fromBits:"),
        (host.named(b"StringVt").unwrap(), b"fromBytes:"),
    ] {
        let selector = hooks.symbol(name);
        let original = h.call(behavior, b"lookup", &[selector]);
        let state = hooks.context(&[original, gc, size(0).unwrap()]);
        h.call(
            behavior,
            b"addMethod",
            &[selector, hooks.method(b"collectEvery", state)],
        );
        let a = h.i(2);
        let b = h.f(0.5);
        same(h.fv(h.call(a, b"+", &[b])), 2.5);
        same(h.fv(h.call(b, b"+", &[a])), 2.5);
        assert_eq!(h.iv(h.call(text, b"size", &[])), 4);
        h.call(text, b"at:", &[a]);
        h.call(a, b"printString", &[]);
        assert!(hooks.count(state, 2) > 0);
        h.call(behavior, b"addMethod", &[selector, original]);
    }
    let path = std::env::temp_dir().join(format!("birnam-numbers-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/numbers.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let module = host.load_native(&h.q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let declared = |name: &[u8]| h.call(module, b"methodAt:", &[hooks.symbol(name)]);
    let state = hooks.context(&[
        gc,
        h.call(host.named(b"Vector").unwrap(), b"new", &[]),
        h.nil,
    ]);
    let step = h.call(declared(b"step"), b"withContext:", &[state]);
    let behavior = h.call(host.named(b"ObjectVt").unwrap(), b"delegated", &[]);
    h.call(behavior, b"addMethod", &[hooks.symbol(b"value:"), step]);
    let block = h.call(behavior, b"allocate", &[size(0).unwrap()]);
    h.call(h.i(i64::MAX - 1), b"to:do:", &[h.i(i64::MAX), block]);
    let seen = hooks.field(state, 1);
    assert_eq!(h.q.vector_size(seen).unwrap(), 2);
    assert_eq!(h.iv(h.q.vector_at(seen, 0).unwrap()), i64::MAX - 1);
    assert_eq!(h.iv(h.q.vector_at(seen, 1).unwrap()), i64::MAX);
    hooks.put(state, 2, fault);
    assert_eq!(h.call(h.i(0), b"to:do:", &[h.i(9), block]), fault);
    let target = host
        .send(
            host.named(b"ObjectVt").unwrap(),
            b"allocate",
            &[size(0).unwrap()],
        )
        .unwrap();
    let temporary = host.scope();
    temporary.keep(target).unwrap();
    let watch_args = temporary
        .call(host.id.root, b"sequence", &[target])
        .unwrap();
    temporary
        .call(
            declared(b"watch"),
            b"invokeOn:arguments:",
            &[module, watch_args],
        )
        .unwrap();
    let raw = h.i(target.addr() as i64);
    host.send(host.id.root, b"ensurePrivateVtableFor:", &[raw])
        .unwrap();
    drop(temporary);
    host.send(gc, b"collectPrecisely", &[]).unwrap();
    let visitor_type = h.call(host.named(b"ObjectVt").unwrap(), b"delegated", &[]);
    h.call(
        visitor_type,
        b"addMethod",
        &[
            hooks.symbol(b"allocation:bytes:identity:"),
            declared(b"allocation:bytes:identity:"),
        ],
    );
    let visitor = h.call(visitor_type, b"allocate", &[size(0).unwrap()]);
    let remaining = h.call(declared(b"remaining"), b"withContext:", &[visitor]);
    assert_eq!(
        as_size(h.call(remaining, b"invokeOn:", &[module])),
        Some(0),
        "numeric bits must not retain an object with the same address"
    );
    h.call(gc, b"budget:", &[size(1).unwrap()]);
    for _ in 0..8 {
        let a = h.i(9);
        let b = h.f(0.5);
        same(h.fv(h.call(a, b"/", &[b])), 18.0);
        assert_eq!(h.iv(h.call(text, b"size", &[])), 4);
    }
    h.call(gc, b"budget:", &[size(1024 * 1024).unwrap()]);
    drop(h);
    host.send(gc, b"collectPrecisely", &[]).unwrap();
}
