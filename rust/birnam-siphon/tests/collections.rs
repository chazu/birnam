//! The real Bootstrap collection branch, compared with independent Rust sequences.
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
    ns: Oop,
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
    unsafe fn named(&self, name: &[u8]) -> Oop {
        unsafe {
            self.call(
                self.ns,
                b"at:",
                &[Operand::object(self.id.intern_name(name).unwrap())],
            )
        }
    }
    unsafe fn i(&self, n: i64) -> Oop {
        unsafe {
            self.call(
                self.named(b"Integer"),
                b"fromWord:",
                &[Operand::word(n as usize)],
            )
        }
    }
    unsafe fn f(&self, n: f64) -> Oop {
        unsafe {
            self.call(
                self.named(b"Float"),
                b"fromBits:",
                &[Operand::word(n.to_bits() as usize)],
            )
        }
    }
    unsafe fn s(&self, b: &[u8]) -> Oop {
        unsafe { self.call(self.named(b"String"), b"fromBytes:", &[Operand::bytes(b)]) }
    }
    unsafe fn iv(&self, v: Oop) -> i64 {
        unsafe { send(self.id, v, b"word", &[]) }.as_word().unwrap() as i64
    }
    unsafe fn size(&self, v: Oop) -> i64 {
        unsafe { self.iv(self.call(v, b"size", &[])) }
    }
    unsafe fn text(&self, v: Oop) -> Vec<u8> {
        unsafe {
            let b = send(self.id, v, b"bytes", &[]).as_bytes().unwrap();
            borrowed_bytes(b.pointer, b.length).unwrap().to_vec()
        }
    }
    unsafe fn at(&self, v: Oop, i: i64) -> Oop {
        unsafe { self.call(v, b"at:", &[Operand::object(self.i(i))]) }
    }
    unsafe fn seq(&self, v: Oop) -> Vec<i64> {
        unsafe { (0..self.size(v)).map(|i| self.iv(self.at(v, i))).collect() }
    }
    unsafe fn put(&self, m: Oop, k: Oop, v: Oop) -> Oop {
        unsafe { self.call(m, b"at:put:", &[Operand::object(k), Operand::object(v)]) }
    }
    unsafe fn get(&self, m: Oop, k: Oop) -> Oop {
        unsafe { self.call(m, b"at:", &[Operand::object(k)]) }
    }
    unsafe fn range(&self, a: i64, b: i64) -> Oop {
        unsafe { self.call(self.i(a), b"to:", &[Operand::object(self.i(b))]) }
    }
    unsafe fn callback(
        &self,
        selector: &[u8],
        method: native::Method,
        c: *mut c_void,
    ) -> (Oop, Oop) {
        unsafe {
            let b = self.keep(behavior(self.id));
            install(self.id, b, selector, method, c);
            (self.keep(allocate(self.id, b, 0)), b)
        }
    }
}
struct Callback {
    h: *const H,
    calls: Cell<usize>,
    mode: Cell<usize>,
    behavior: Cell<Oop>,
    fresh: RefCell<Vec<(Oop, usize)>>,
}
unsafe extern "C" fn callback(c: *mut c_void, _: Oop, a: *const Operand, count: usize) -> Outcome {
    unsafe {
        let c = &*c.cast::<Callback>();
        let h = &*c.h;
        collect(h.id, h.gc);
        // These addresses are observations, not registered roots. During sorting,
        // only Vector owns the keys returned by earlier calls.
        if matches!(c.mode.get(), 1 | 5) {
            for &(key, identity) in c.fresh.borrow().iter() {
                assert!(present(h.id, key), "Vector lost a prior sort key");
                assert_eq!(
                    send(h.id, h.id.receiver, b"identityOf:", &[Operand::object(key)]).as_word(),
                    Some(identity)
                );
            }
        }
        let args = borrowed_slice(a, count).unwrap();
        c.calls.set(c.calls.get() + 1);
        if c.mode.get() == 5 && c.calls.get() == 3 {
            return Outcome::failed(native::INVALID_REQUEST, 57);
        }
        let fresh_integer = |value: i64| {
            let integer = h.named(b"Integer");
            // Deliberately bypass H::call/H::i: no harness retention after return.
            let value = object(send(
                h.id,
                integer,
                b"fromWord:",
                &[Operand::word(value as usize)],
            ));
            let identity = send(
                h.id,
                h.id.receiver,
                b"identityOf:",
                &[Operand::object(value)],
            )
            .as_word()
            .unwrap();
            c.fresh.borrow_mut().push((value, identity));
            value
        };
        let value = match c.mode.get() {
            0 => object(args[0]),
            1 | 5 => fresh_integer(h.iv(object(args[0])) % 3),
            2 => fresh_integer(h.iv(object(args[0])) + h.iv(object(args[1]))),
            3 => fresh_integer(789),
            4 => {
                install(
                    h.id,
                    c.behavior.get(),
                    b"value:",
                    replacement,
                    c as *const _ as *mut c_void,
                );
                object(args[0])
            }
            _ => unreachable!(),
        };
        Outcome::loaded(Operand::object(value))
    }
}
unsafe extern "C" fn replacement(
    c: *mut c_void,
    _: Oop,
    a: *const Operand,
    count: usize,
) -> Outcome {
    unsafe {
        let c = &*c.cast::<Callback>();
        let h = &*c.h;
        collect(h.id, h.gc);
        c.calls.set(c.calls.get() + 100);
        Outcome::loaded(borrowed_slice(a, count).unwrap()[0])
    }
}
unsafe extern "C" fn collision(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::word(7))
}
struct Equality {
    h: *const H,
    empty: Oop,
    calls: Cell<usize>,
}
unsafe extern "C" fn nested_equal(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe {
        let e = &*c.cast::<Equality>();
        let h = &*e.h;
        e.calls.set(e.calls.get() + 1);
        collect(h.id, h.gc);
        // A nested insertion adds a key; the outer operation is a replacement.
        let nested = h.put(e.empty, h.i(99), h.i(98));
        assert_eq!(h.size(nested), 1);
        Outcome::loaded(Operand::object(h.named(b"true")))
    }
}
struct Wrap {
    h: *const H,
    original: NativeEntry,
    calls: Cell<usize>,
}
unsafe extern "C" fn collecting(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let w = &*c.cast::<Wrap>();
        let h = &*w.h;
        w.calls.set(w.calls.get() + 1);
        collect(h.id, h.gc);
        w.original.method.unwrap()(w.original.context, r, a, n)
    }
}
unsafe fn selected(h: &H, b: Oop, name: &[u8]) -> NativeEntry {
    unsafe {
        send(
            h.id,
            b,
            b"lookup",
            &[Operand::object(h.id.intern_name(name).unwrap())],
        )
    }
    .as_entry()
    .unwrap()
}
unsafe fn restore(h: &H, b: Oop, name: &[u8], e: NativeEntry) {
    unsafe {
        send(
            h.id,
            b,
            b"addMethod",
            &[
                Operand::object(h.id.intern_name(name).unwrap()),
                Operand::entry(e),
            ],
        );
    }
}
#[test]
#[ignore = "scripts/native-message-test.sh builds production dylibs"]
fn persistent_collections_use_live_owner_messages_and_survive_gc() {
    unsafe {
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        let named = |n: &[u8]| {
            object(send(
                id,
                ns,
                b"at:",
                &[Operand::object(id.intern_name(n).unwrap())],
            ))
        };
        let h = Box::new(H {
            id,
            ns,
            roots: named(b"Roots"),
            gc: named(b"GC"),
            tokens: RefCell::new(Vec::new()),
        });
        let nil = h.named(b"nil");
        let truth = h.named(b"true");
        let falsity = h.named(b"false");
        send(id, h.gc, b"budget:", &[Operand::word(128 * 1024)]);
        for name in ["word", "vector", "map", "range"] {
            let title = match name {
                "word" => b"Word".as_slice(),
                "vector" => b"Vector",
                "map" => b"Map",
                _ => b"Range",
            };
            assert_eq!(load(id, name, &[Operand::object(ns)]), h.named(title));
        }
        // Word's full bit domain, overflow and independent shallow cloning.
        let word = h.named(b"Word");
        for bits in [0, 1, i64::MAX as usize, 1usize << 63, usize::MAX] {
            let w = h.call(word, b"fromBits:", &[Operand::word(bits)]);
            assert_eq!(send(id, w, b"bits", &[]).as_word(), Some(bits));
            let cloned = h.call(w, b"clone", &[]);
            assert_ne!(w, cloned);
            assert_eq!(h.call(w, b"=", &[Operand::object(cloned)]), truth);
            assert_eq!(
                h.text(h.call(w, b"printString", &[])),
                format!("0x{bits:x}").as_bytes()
            );
            let integer = h.call(w, b"asInteger", &[]);
            if bits <= i64::MAX as usize {
                assert_eq!(h.iv(integer), bits as i64)
            } else {
                assert_eq!(
                    h.call(integer, b"kind", &[]),
                    id.intern_name(b"overflow").unwrap()
                )
            }
        }
        // Cross the tail, first trie and second trie growth boundaries.
        let empty = h.call(h.named(b"Vector"), b"new", &[]);
        let mut v = empty;
        let mut snapshots = Vec::new();
        for i in 0..1100 {
            v = h.call(v, b"add:", &[Operand::object(h.i(i))]);
            if [31, 32, 33, 1023, 1024, 1055, 1056, 1099].contains(&i) {
                snapshots.push((v, i + 1));
            }
        }
        collect(id, h.gc);
        assert_eq!(h.size(empty), 0);
        for (snapshot, len) in snapshots {
            assert_eq!(h.size(snapshot), len);
            for i in [0, len / 2, len - 1] {
                assert_eq!(h.iv(h.at(snapshot, i)), i);
            }
        }
        assert_eq!(h.at(v, -1), nil);
        assert_eq!(h.at(v, 1100), nil);
        assert_eq!(h.size(h.call(v, b"clone", &[])), 0);
        let small = h.call(
            v,
            b"copyFrom:to:",
            &[Operand::object(h.i(5)), Operand::object(h.i(15))],
        );
        assert_eq!(h.seq(small), (5..15).collect::<Vec<_>>());
        assert_eq!(
            h.seq(h.call(small, b"reversed", &[])),
            (5..15).rev().collect::<Vec<_>>()
        );
        let joined = h.call(small, b",", &[Operand::object(small)]);
        assert_eq!(h.seq(joined), (5..15).chain(5..15).collect::<Vec<_>>());
        assert_eq!(
            h.iv(h.call(small, b"indexOf:", &[Operand::object(h.i(11))])),
            6
        );
        let c = Box::new(Callback {
            h: &*h,
            calls: Cell::new(0),
            mode: Cell::new(1),
            behavior: Cell::new(ptr::null_mut()),
            fresh: RefCell::new(Vec::new()),
        });
        let cp = &*c as *const _ as *mut c_void;
        let (block, b) = h.callback(b"value:", callback, cp);
        c.behavior.set(b);
        let sorted = h.call(small, b"sortBy:", &[Operand::object(block)]);
        let mut expected: Vec<i64> = (5..15).collect();
        expected.sort_by_key(|n| n % 3);
        assert_eq!(h.seq(sorted), expected);
        assert_eq!(c.calls.get(), 10);
        let reclaimed = |values: &[(Oop, usize)]| {
            for &(value, identity) in values {
                assert!(
                    !present(id, value)
                        || send(id, id.receiver, b"identityOf:", &[Operand::object(value)])
                            .as_word()
                            != Some(identity),
                    "temporary root leaked after return"
                );
            }
        };
        collect(id, h.gc);
        reclaimed(&c.fresh.borrow());
        c.fresh.borrow_mut().clear();
        // Native failure must also release keys accumulated before the failure.
        c.mode.set(5);
        c.calls.set(0);
        let failure = id
            .send_named(small, b"sortBy:", &[Operand::object(block)])
            .unwrap_err();
        assert_eq!(failure.status, native::INVALID_REQUEST);
        collect(id, h.gc);
        reclaimed(&c.fresh.borrow());
        c.fresh.borrow_mut().clear();

        c.calls.set(0);
        c.mode.set(4);
        h.call(small, b"do:", &[Operand::object(block)]);
        assert_eq!(c.calls.get(), 901);
        c.mode.set(2);
        let (fold, _) = h.callback(b"value:value:", callback, cp);
        assert_eq!(
            h.iv(h.call(
                small,
                b"inject:into:",
                &[Operand::object(h.i(0)), Operand::object(fold)]
            )),
            (5..15).sum()
        );
        assert_eq!(
            h.text(h.call(small, b"printString", &[])),
            b"#[5 6 7 8 9 10 11 12 13 14]"
        );
        collect(id, h.gc);
        {
            let values = c.fresh.borrow();
            assert_eq!(values.len(), 10);
            reclaimed(&values[..values.len() - 1]);
            assert!(present(id, values.last().unwrap().0));
        }
        c.fresh.borrow_mut().clear();
        // Map persistence and independent value-key probes, including signed zero.
        let map = h.call(h.named(b"Map"), b"new", &[]);
        let mut m = map;
        for i in 0..200 {
            m = h.put(m, h.i(i), h.i(i * 7));
        }
        collect(id, h.gc);
        assert_eq!(h.size(map), 0);
        assert_eq!(h.size(m), 200);
        for i in 0..200 {
            assert_eq!(h.iv(h.get(m, h.i(i))), i * 7);
        }
        let old = m;
        m = h.put(m, h.i(17), h.i(-5));
        assert_eq!(h.size(m), 200);
        assert_eq!(h.iv(h.get(old, h.i(17))), 119);
        assert_eq!(h.iv(h.get(m, h.i(17))), -5);
        m = h.put(m, h.s(b"a\0b"), h.i(11));
        assert_eq!(h.iv(h.get(m, h.s(b"a\0b"))), 11);
        m = h.put(m, h.f(-0.0), h.i(12));
        assert_eq!(h.iv(h.get(m, h.f(0.0))), 12);
        m = h.put(m, h.f(17.0), h.i(13));
        assert_eq!(h.iv(h.get(m, h.i(17))), -5);
        let nan = h.f(f64::NAN);
        m = h.put(m, nan, h.i(14));
        assert_eq!(h.iv(h.get(m, nan)), 14);
        assert_eq!(h.get(m, h.f(f64::NAN)), nil);
        let wa = h.call(word, b"fromBits:", &[Operand::word(5)]);
        let wb = h.call(word, b"fromBits:", &[Operand::word(5)]);
        m = h.put(m, wa, h.i(15));
        assert_eq!(h.get(m, wb), nil);
        assert_eq!(h.iv(h.get(m, wa)), 15);
        let symbol = h.keep(id.intern_name(b"symbol-key").unwrap());
        m = h.put(m, symbol, h.i(16));
        assert_eq!(h.iv(h.get(m, symbol)), 16);
        assert_eq!(
            h.call(m, b"includesKey:", &[Operand::object(h.i(-999))]),
            falsity
        );
        assert_eq!(
            h.call(m, b"includesKey:", &[Operand::object(h.i(17))]),
            truth
        );
        c.mode.set(3);
        let (fallback, _) = h.callback(b"value", callback, cp);
        assert_eq!(
            h.iv(h.call(
                m,
                b"at:ifAbsent:",
                &[Operand::object(h.i(-999)), Operand::object(fallback)]
            )),
            789
        );
        assert_eq!(h.size(h.call(m, b"keys", &[])), h.size(m));
        assert_eq!(h.size(h.call(m, b"values", &[])), h.size(m));
        // Force full-hash collision nodes without knowing Map's representation.
        let cb = h.keep(behavior(id));
        install(id, cb, b"collectionHash", collision, ptr::null_mut());
        let mut cm = map;
        let mut keys = Vec::new();
        for i in 0..40 {
            let key = h.keep(allocate(id, cb, 0));
            keys.push(key);
            cm = h.put(cm, key, h.i(i));
        }
        assert_eq!(h.size(cm), 40);
        for (i, k) in keys.iter().enumerate() {
            assert_eq!(h.iv(h.get(cm, *k)), i as i64);
        }
        let replaced = h.put(cm, keys[17], h.i(999));
        assert_eq!(h.size(replaced), 40);
        assert_eq!(h.iv(h.get(cm, keys[17])), 17);
        // Reentrant equality cannot corrupt the outer insertion count.
        let eq = Box::new(Equality {
            h: &*h,
            empty: map,
            calls: Cell::new(0),
        });
        install(
            id,
            cb,
            b"collectionEquals:",
            nested_equal,
            &*eq as *const _ as *mut c_void,
        );
        let one = h.put(map, keys[0], h.i(1));
        let two = h.put(one, keys[1], h.i(2));
        assert_eq!(h.size(two), 1);
        assert!(eq.calls.get() > 0);
        // Range bounds use wide loop arithmetic, including both i64 endpoints.
        c.mode.set(2);
        for (a, b) in [
            (3, 7),
            (9, 4),
            (i64::MAX - 2, i64::MAX),
            (i64::MIN, i64::MIN + 2),
        ] {
            let range = h.range(a, b);
            let expected: Vec<i64> = (i128::from(a)..=i128::from(b)).map(|v| v as i64).collect();
            assert_eq!(h.size(range), expected.len() as i64);
            for (i, n) in expected.iter().enumerate() {
                assert_eq!(h.iv(h.at(range, i as i64)), *n);
            }
            assert_eq!(h.at(range, expected.len() as i64), nil);
            let mut reverse = expected.clone();
            reverse.reverse();
            assert_eq!(h.seq(h.call(range, b"reversed", &[])), reverse);
        }
        assert_eq!(h.call(h.range(i64::MIN, i64::MAX), b"size", &[]), nil);
        let range = h.range(3, 7);
        assert_eq!(
            h.iv(h.call(
                range,
                b"inject:into:",
                &[Operand::object(h.i(0)), Operand::object(fold)]
            )),
            25
        );
        assert_eq!(
            h.call(range, b"includes:", &[Operand::object(h.i(5))]),
            truth
        );
        assert_eq!(
            h.call(range, b"includes:", &[Operand::object(h.f(5.0))]),
            falsity
        );
        assert_eq!(h.text(h.call(range, b"printString", &[])), b"#[3 4 5 6 7]");
        assert_eq!(
            h.seq(h.call(range, b",", &[Operand::object(small)])),
            (3..=7).chain(5..15).collect::<Vec<_>>()
        );
        // Binary-safe split preserves empty/trailing fields and live factories.
        let sb = h.named(b"StringVt");
        let vb = h.named(b"VectorVt");
        let sw = Box::new(Wrap {
            h: &*h,
            original: selected(&h, sb, b"fromBytes:"),
            calls: Cell::new(0),
        });
        let vw = Box::new(Wrap {
            h: &*h,
            original: selected(&h, vb, b"add:"),
            calls: Cell::new(0),
        });
        install(
            id,
            sb,
            b"fromBytes:",
            collecting,
            &*sw as *const _ as *mut c_void,
        );
        install(id, vb, b"add:", collecting, &*vw as *const _ as *mut c_void);
        for (input, separator, parts) in [
            (
                b",a,,b,".as_slice(),
                b",".as_slice(),
                vec![b"".as_slice(), b"a", b"", b"b", b""],
            ),
            (b"a\0b\0", b"\0", vec![b"a", b"b", b""]),
            (b"abc", b"", vec![b"abc"]),
            (b"", b"x", vec![b""]),
        ] {
            let text = h.s(input);
            let result = h.call(text, b"split:", &[Operand::object(h.s(separator))]);
            assert_eq!(h.size(result), parts.len() as i64);
            for (i, part) in parts.iter().enumerate() {
                assert_eq!(h.text(h.at(result, i as i64)), *part);
            }
        }
        let text = h.s(b"unchanged");
        let split = h.call(text, b"split:", &[Operand::object(nil)]);
        assert_eq!(h.at(split, 0), text);
        assert!(sw.calls.get() > 5 && vw.calls.get() > 5);
        restore(&h, sb, b"fromBytes:", sw.original);
        restore(&h, vb, b"add:", vw.original);
        // GC does not mistake Word or Range raw bits for references, even after privatization.
        let model = h.named(b"ObjectModel");
        let object_vt = h.named(b"ObjectVt");
        let victim = object(send(
            id,
            model,
            b"allocate:bytes:",
            &[Operand::object(object_vt), Operand::word(0)],
        ));
        let token = retain(id, h.roots, victim);
        let raw_word = h.call(word, b"fromBits:", &[Operand::word(victim.addr())]);
        let raw_range = h.range(victim.addr() as i64, victim.addr() as i64);
        send(
            id,
            id.receiver,
            b"ensurePrivateVtableFor:",
            &[Operand::object(raw_word)],
        );
        send(
            id,
            id.receiver,
            b"ensurePrivateVtableFor:",
            &[Operand::object(raw_range)],
        );
        release(id, h.roots, token);
        collect(id, h.gc);
        assert!(!present(id, victim));
        // Malformed language-visible reference slots fail instead of reading outside storage.
        let bad_map = h.call(map, b"clone", &[]);
        send(
            id,
            model,
            b"slotAt:in:put:",
            &[
                Operand::word(0),
                Operand::object(bad_map),
                Operand::object(h.i(1)),
            ],
        );
        assert!(
            id.send_named(bad_map, b"at:", &[Operand::object(h.i(1))])
                .is_err()
        );
        send(
            id,
            model,
            b"slotAt:in:put:",
            &[Operand::word(0), Operand::object(bad_map), Operand::unit()],
        );
        let bad_vector = h.call(empty, b"add:", &[Operand::object(nil)]);
        send(
            id,
            model,
            b"slotAt:in:put:",
            &[
                Operand::word(3),
                Operand::object(bad_vector),
                Operand::object(h.i(1)),
            ],
        );
        assert!(
            id.send_named(bad_vector, b"at:", &[Operand::object(h.i(0))])
                .is_err()
        );
        // Private collection behavior still retains and operates on its own references.
        send(
            id,
            id.receiver,
            b"ensurePrivateVtableFor:",
            &[Operand::object(small)],
        );
        send(
            id,
            id.receiver,
            b"ensurePrivateVtableFor:",
            &[Operand::object(m)],
        );
        collect(id, h.gc);
        assert_eq!(h.seq(small), (5..15).collect::<Vec<_>>());
        assert_eq!(h.iv(h.get(m, h.i(17))), -5);
        // Collection edges alone retain text; retirement releases buffers after dropping it.
        let text = object(send(
            id,
            h.named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(b"only-through-vector")],
        ));
        let text_token = retain(id, h.roots, text);
        let holder = object(send(id, empty, b"add:", &[Operand::object(text)]));
        let holder_token = retain(id, h.roots, holder);
        release(id, h.roots, text_token);
        collect(id, h.gc);
        assert!(present(id, text));
        release(id, h.roots, holder_token);
        collect(id, h.gc);
        assert!(!present(id, text));
        let key = object(send(
            id,
            h.named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(b"only-through-map-key")],
        ));
        let key_token = retain(id, h.roots, key);
        let value = object(send(
            id,
            h.named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(b"only-through-map-value")],
        ));
        let value_token = retain(id, h.roots, value);
        let holder = object(send(
            id,
            map,
            b"at:put:",
            &[Operand::object(key), Operand::object(value)],
        ));
        let holder_token = retain(id, h.roots, holder);
        release(id, h.roots, key_token);
        release(id, h.roots, value_token);
        collect(id, h.gc);
        assert!(present(id, key) && present(id, value));
        release(id, h.roots, holder_token);
        collect(id, h.gc);
        assert!(!present(id, key) && !present(id, value));
        for token in std::mem::take(&mut *h.tokens.borrow_mut()) {
            release(id, h.roots, token);
        }
        collect(id, h.gc);
    }
}
