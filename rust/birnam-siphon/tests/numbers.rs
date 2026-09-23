//! Production numeric values checked against host arithmetic and explicit boundaries.
mod support;
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
    ptr,
};
use std::os::unix::ffi::OsStrExt;
use support::*;
struct H {
    id: Protocol,
    roots: Oop,
    gc: Oop,
    integer: Oop,
    float: Oop,
    string: Oop,
    nil: Oop,
    truth: Oop,
    tokens: RefCell<Vec<usize>>,
}
impl H {
    unsafe fn keep(&self, v: Oop) -> Oop {
        self.tokens
            .borrow_mut()
            .push(unsafe { retain(self.id, self.roots, v) });
        v
    }
    unsafe fn call(&self, r: Oop, name: &[u8], args: &[Operand]) -> Oop {
        unsafe { self.keep(object(send(self.id, r, name, args))) }
    }
    unsafe fn i(&self, v: i64) -> Oop {
        unsafe { self.call(self.integer, b"fromWord:", &[Operand::word(v as usize)]) }
    }
    unsafe fn f(&self, v: f64) -> Oop {
        unsafe {
            self.call(
                self.float,
                b"fromBits:",
                &[Operand::word(v.to_bits() as usize)],
            )
        }
    }
    unsafe fn s(&self, v: &[u8]) -> Oop {
        unsafe { self.call(self.string, b"fromBytes:", &[Operand::bytes(v)]) }
    }
    unsafe fn iv(&self, v: Oop) -> i64 {
        unsafe { send(self.id, v, b"word", &[]) }.as_word().unwrap() as i64
    }
    unsafe fn fv(&self, v: Oop) -> f64 {
        f64::from_bits(unsafe { send(self.id, v, b"bits", &[]) }.as_word().unwrap() as u64)
    }
    unsafe fn text(&self, v: Oop) -> Vec<u8> {
        let b = unsafe { send(self.id, v, b"bytes", &[]) }
            .as_bytes()
            .unwrap();
        unsafe { borrowed_bytes(b.pointer, b.length) }
            .unwrap()
            .to_vec()
    }
    unsafe fn fault(&self, v: Oop, kind: &[u8]) {
        unsafe {
            assert_eq!(self.call(v, b"isError", &[]), self.truth);
            assert_eq!(
                self.call(v, b"kind", &[]),
                self.id.intern_name(kind).unwrap()
            );
        }
    }
}
unsafe fn selected(id: Protocol, b: Oop, name: &[u8]) -> NativeEntry {
    unsafe {
        send(
            id,
            b,
            b"lookup",
            &[Operand::object(id.intern_name(name).unwrap())],
        )
    }
    .as_entry()
    .unwrap()
}
fn same(a: f64, b: f64) {
    if b.is_nan() {
        assert!(a.is_nan());
    } else {
        assert_eq!(a.to_bits(), b.to_bits());
    }
}
struct Callback {
    id: Protocol,
    gc: Oop,
    entry: NativeEntry,
    calls: Cell<usize>,
}
unsafe extern "C" fn collecting(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let c = unsafe { &*c.cast::<Callback>() };
    c.calls.set(c.calls.get() + 1);
    if let Err(e) = unsafe { c.id.send_named(c.gc, b"collectPrecisely", &[]) } {
        return e;
    }
    unsafe { c.entry.method.unwrap()(c.entry.context, r, a, n) }
}
struct Loop {
    id: Protocol,
    gc: Oop,
    nil: Oop,
    error: Cell<Oop>,
    seen: RefCell<Vec<i64>>,
}
unsafe extern "C" fn step(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    let l = unsafe { &*c.cast::<Loop>() };
    let result = (|| unsafe {
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(Outcome::failed(native::INVALID_REQUEST, 0));
        };
        l.id.send_named(l.gc, b"collectPrecisely", &[])?;
        l.seen.borrow_mut().push(
            l.id.send_named(v.as_object().unwrap(), b"word", &[])?
                .as_word()
                .unwrap() as i64,
        );
        Ok(Operand::object(if l.error.get().is_null() {
            l.nil
        } else {
            l.error.get()
        }))
    })();
    result.map_or_else(|e| e, Outcome::loaded)
}
#[test]
#[ignore = "scripts/native-message-test.sh builds production dylibs"]
fn numbers_text_and_live_dependencies_survive_collection() {
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
        let h = H {
            id,
            roots: named(b"Roots"),
            gc: named(b"GC"),
            integer: named(b"Integer"),
            float: named(b"Float"),
            string: named(b"String"),
            nil: named(b"nil"),
            truth: named(b"true"),
            tokens: RefCell::new(Vec::new()),
        };
        assert_eq!(load(id, "integer", &[Operand::object(ns)]), h.integer);
        assert_eq!(load(id, "float", &[Operand::object(ns)]), h.float);
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
                    let result = h.call(left, name, &[Operand::object(right)]);
                    let expected = match op {
                        0 => a.checked_add(b),
                        1 => a.checked_sub(b),
                        2 => a.checked_mul(b),
                        3 => a.checked_div(b),
                        4 if a == i64::MIN && b == -1 => Some(0),
                        4 => a.checked_rem(b),
                        _ => unreachable!(),
                    };
                    if let Some(v) = expected {
                        assert_eq!(h.iv(result), v);
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
                    assert_eq!(
                        h.call(left, name, &[Operand::object(right)]) == h.truth,
                        expected
                    );
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
                    let result = h.call(left, name, &[Operand::object(right)]);
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
                    assert_eq!(
                        h.call(left, name, &[Operand::object(right)]) == h.truth,
                        expected
                    );
                }
            }
            for b in [-17, 0, 17, i64::MAX] {
                let integer = h.i(b);
                for (op, name) in operators[..4].iter().enumerate() {
                    for (receiver, right, x, y) in
                        [(left, integer, a, b as f64), (integer, left, b as f64, a)]
                    {
                        let result = h.call(receiver, name, &[Operand::object(right)]);
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
                assert_ne!(h.call(integer, b"=", &[Operand::object(left)]), h.truth);
                assert_ne!(h.call(left, b"=", &[Operand::object(integer)]), h.truth);
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
            f64::from_bits((9_223_372_036_854_775_808.0f64).to_bits() - 1),
        ] {
            let left = h.f(a);
            for (name, expected) in [
                (b"asInteger".as_slice(), a.trunc()),
                (b"floor", a.floor()),
                (b"ceiling", a.ceil()),
                (b"rounded", a.round()),
            ] {
                let v = h.call(left, name, &[]);
                if !expected.is_finite()
                    || expected < i64::MIN as f64
                    || expected >= 9_223_372_036_854_775_808.0
                {
                    h.fault(v, b"overflow");
                } else {
                    assert_eq!(h.iv(v), expected as i64);
                }
            }
        }
        for (v, text) in [
            (0.0, b"0.0".as_slice()),
            (-0.0, b"-0.0"),
            (2.0, b"2.0"),
            (f64::NAN, b"NaN.0"),
            (f64::INFINITY, b"inf.0"),
        ] {
            assert_eq!(h.text(h.call(h.f(v), b"printString", &[])), text);
        }
        let fault = h.call(
            named(b"Error"),
            b"message:",
            &[Operand::object(h.s(b"incoming"))],
        );
        for value in [h.i(4), h.f(4.0)] {
            for name in [b"+".as_slice(), b"<", b"/"] {
                assert_eq!(h.call(value, name, &[Operand::object(fault)]), fault);
                h.fault(
                    h.call(value, name, &[Operand::object(h.nil)]),
                    b"typeMismatch",
                );
            }
            assert_ne!(h.call(value, b"=", &[Operand::object(fault)]), h.truth);
        }
        // Native recognition reports Unit for other families, never foreign layout data.
        assert_eq!(
            send(id, h.integer, b"wordOf:", &[Operand::object(h.float)]).kind(),
            native::UNIT
        );
        assert_eq!(
            send(id, h.float, b"bitsOf:", &[Operand::object(h.integer)]).kind(),
            native::UNIT
        );
        let text = h.s(&[b'a', 0, 0xff, b'a']);
        assert_eq!(h.iv(h.call(text, b"size", &[])), 4);
        assert_eq!(
            h.iv(h.call(text, b"byteAt:", &[Operand::object(h.i(2))])),
            255
        );
        assert_eq!(
            h.text(h.call(text, b"at:", &[Operand::object(h.i(1))])),
            [0]
        );
        for i in [-1, 4, i64::MAX] {
            assert_eq!(h.call(text, b"at:", &[Operand::object(h.i(i))]), h.nil);
        }
        assert_eq!(h.call(text, b"at:", &[Operand::object(h.f(1.0))]), h.nil);
        assert_eq!(
            h.text(h.call(
                text,
                b"copyFrom:to:",
                &[Operand::object(h.i(1)), Operand::object(h.i(3))]
            )),
            [0, 255]
        );
        assert_eq!(
            h.call(
                text,
                b"copyFrom:to:",
                &[Operand::object(h.i(3)), Operand::object(h.i(2))]
            ),
            h.nil
        );
        assert_eq!(
            h.iv(h.call(text, b"indexOf:", &[Operand::object(h.s(&[0, 255]))])),
            1
        );
        assert_eq!(
            h.iv(h.call(text, b"indexOf:", &[Operand::object(h.string)])),
            0
        );
        assert_eq!(
            h.call(text, b"indexOf:", &[Operand::object(h.s(b"no"))]),
            h.nil
        );
        assert_eq!(
            h.iv(h.call(h.s(b"[ { \"[\\\"}\" // }\n]"), b"bracketDepth", &[])),
            1
        );
        for (s, v) in [
            (b" \t-9223372036854775808\n".as_slice(), i64::MIN),
            (b"9223372036854775807", i64::MAX),
            (b"+17", 17),
        ] {
            assert_eq!(h.iv(h.call(h.s(s), b"asInteger", &[])), v);
        }
        for s in [
            b"9223372036854775808".as_slice(),
            b"1.5",
            b"",
            b"\x0b1",
            b"1\0",
            b"\xff",
        ] {
            assert_eq!(h.call(h.s(s), b"asInteger", &[]), h.nil);
        }
        assert_eq!(h.iv(h.call(h.s(b"17"), b"asNumber", &[])), 17);
        for (s, v) in [
            (b"1.5".as_slice(), 1.5),
            (b"1e2", 100.0),
            (b"9223372036854775808", 9_223_372_036_854_775_808.0),
            (b"NaN", f64::NAN),
        ] {
            same(h.fv(h.call(h.s(s), b"asNumber", &[])), v);
        }
        let builder = h.call(named(b"StringBuilder"), b"new", &[]);
        h.call(builder, b"addByte:", &[Operand::object(h.i(321))]);
        h.call(builder, b"addByte:", &[Operand::object(h.i(-1))]);
        h.call(builder, b"addByte:", &[Operand::object(h.nil)]);
        assert_eq!(h.iv(h.call(builder, b"size", &[])), 2);
        assert_eq!(h.text(h.call(builder, b"asString", &[])), [65, 255]);
        // Every cross-family conversion is late-bound and retains inputs across GC.
        let ib = named(b"IntegerVt");
        let fb = named(b"FloatVt");
        let sv = named(b"StringVt");
        for (behavior, name) in [
            (ib, b"wordOf:".as_slice()),
            (ib, b"fromWord:"),
            (fb, b"bitsOf:"),
            (fb, b"fromBits:"),
            (sv, b"fromBytes:"),
        ] {
            let original = selected(id, behavior, name);
            let callback = Box::leak(Box::new(Callback {
                id,
                gc: h.gc,
                entry: original,
                calls: Cell::new(0),
            }));
            install(
                id,
                behavior,
                name,
                collecting,
                ptr::from_mut(callback).cast(),
            );
            let a = h.i(2);
            let b = h.f(0.5);
            same(h.fv(h.call(a, b"+", &[Operand::object(b)])), 2.5);
            same(h.fv(h.call(b, b"+", &[Operand::object(a)])), 2.5);
            assert_eq!(h.iv(h.call(text, b"size", &[])), 4);
            h.call(text, b"at:", &[Operand::object(a)]);
            h.call(a, b"printString", &[]);
            assert!(callback.calls.get() > 0);
            install(
                id,
                behavior,
                name,
                original.method.unwrap(),
                original.context,
            );
        }
        let bb = h.keep(behavior(id));
        let block = h.keep(allocate(id, bb, 0));
        let l = Box::leak(Box::new(Loop {
            id,
            gc: h.gc,
            nil: h.nil,
            error: Cell::new(ptr::null_mut()),
            seen: RefCell::new(Vec::new()),
        }));
        install(id, bb, b"value:", step, ptr::from_mut(l).cast());
        let start = h.i(i64::MAX - 1);
        h.call(
            start,
            b"to:do:",
            &[Operand::object(h.i(i64::MAX)), Operand::object(block)],
        );
        assert_eq!(*l.seen.borrow(), [i64::MAX - 1, i64::MAX]);
        l.error.set(fault);
        assert_eq!(
            h.call(
                h.i(0),
                b"to:do:",
                &[Operand::object(h.i(9)), Operand::object(block)]
            ),
            fault
        );
        // Raw numeric bits must never retain an object, even through a private behavior.
        let target = allocate(id, named(b"ObjectVt"), 0);
        let raw = h.i(target.addr() as i64);
        send(
            id,
            id.receiver,
            b"ensurePrivateVtableFor:",
            &[Operand::object(raw)],
        );
        collect(id, h.gc);
        assert!(!present(id, target));
        send(id, h.gc, b"budget:", &[Operand::word(1)]);
        for _ in 0..8 {
            let a = h.i(9);
            let b = h.f(0.5);
            same(h.fv(h.call(a, b"/", &[Operand::object(b)])), 18.0);
            assert_eq!(h.iv(h.call(text, b"size", &[])), 4);
        }
        send(id, h.gc, b"budget:", &[Operand::word(1024 * 1024)]);
        for token in h.tokens.take() {
            release(id, h.roots, token);
        }
        collect(id, h.gc);
    }
}
