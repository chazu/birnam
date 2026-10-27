//! Persistent collections compared with independent Rust sequences through Oop messages.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use birnam_host_support::{Context, Scope};
use std::{path::Path, ptr};
struct H<'a> {
    host: &'a Context,
    q: Scope<'a>,
    gc: Oop,
}
impl H<'_> {
    fn call(&self, r: Oop, name: &[u8], args: &[Oop]) -> Oop {
        self.q.call(r, name, args).unwrap()
    }
    fn named(&self, name: &[u8]) -> Oop {
        self.host.named(name).unwrap()
    }
    fn i(&self, n: i64) -> Oop {
        self.call(
            self.named(b"Integer"),
            b"fromWord:",
            &[self.q.word(n as usize).unwrap()],
        )
    }
    fn f(&self, n: f64) -> Oop {
        self.call(
            self.named(b"Float"),
            b"fromBits:",
            &[self.q.word(n.to_bits() as usize).unwrap()],
        )
    }
    fn s(&self, b: &[u8]) -> Oop {
        self.q.string(b).unwrap()
    }
    fn iv(&self, v: Oop) -> i64 {
        self.q.word_bits(self.call(v, b"word", &[])).unwrap() as i64
    }
    fn size(&self, v: Oop) -> i64 {
        self.iv(self.call(v, b"size", &[]))
    }
    fn text(&self, v: Oop) -> Vec<u8> {
        self.q.read_bytes(self.call(v, b"bytes", &[])).unwrap()
    }
    fn at(&self, v: Oop, i: i64) -> Oop {
        self.call(v, b"at:", &[self.i(i)])
    }
    fn seq(&self, v: Oop) -> Vec<i64> {
        (0..self.size(v)).map(|i| self.iv(self.at(v, i))).collect()
    }
    fn put(&self, m: Oop, k: Oop, v: Oop) -> Oop {
        self.call(m, b"at:put:", &[k, v])
    }
    fn get(&self, m: Oop, k: Oop) -> Oop {
        self.call(m, b"at:", &[k])
    }
    fn range(&self, a: i64, b: i64) -> Oop {
        self.call(self.i(a), b"to:", &[self.i(b)])
    }
}
#[test]
#[ignore = "requires production Oop modules"]
fn persistent_collections_use_live_owner_messages_and_survive_gc() {
    let host = oop::bootstrap();
    let h = H {
        host: &host,
        q: host.scope(),
        gc: host.named(b"GC").unwrap(),
    };
    let hooks = oop::hooks::Hooks::new(&host);
    let path =
        std::env::temp_dir().join(format!("birnam-collections-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/collections.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let module = host.load_native(&h.q, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let method = |name: &[u8], context| {
        h.call(
            h.call(module, b"methodAt:", &[hooks.symbol(name)]),
            b"withContext:",
            &[context],
        )
    };
    let callback_object = |name: &[u8], method| {
        let value = h.call(h.named(b"Object"), b"clone", &[]);
        let behavior = h.call(host.id.root, b"ensurePrivateVtableFor:", &[value]);
        h.call(behavior, b"addMethod", &[hooks.symbol(name), method]);
        (value, behavior)
    };
    let probe = h.call(h.named(b"Object"), b"clone", &[]);
    let probe_vt = h.call(host.id.root, b"ensurePrivateVtableFor:", &[probe]);
    for name in [
        b"watch".as_slice(),
        b"mask",
        b"observed",
        b"reset",
        b"allocation:bytes:identity:",
    ] {
        h.call(
            probe_vt,
            b"addMethod",
            &[hooks.symbol(name), method(name, probe)],
        );
    }
    // String is available before Collection during boot, but must join its
    // shared protocol once Bootstrap finishes, just like Vector and Map.
    assert_eq!(
        h.call(host.id.root, b"parentOf:", &[h.named(b"StringVt")]),
        h.named(b"CollectionVt")
    );
    let nil = h.named(b"nil");
    let truth = h.named(b"true");
    let falsity = h.named(b"false");
    h.call(h.gc, b"budget:", &[size(128 * 1024).unwrap()]);
    for (name, title) in [
        ("word", b"Word".as_slice()),
        ("vector", b"Vector"),
        ("map", b"Map"),
    ] {
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
            h.named(title)
        );
    }
    // Word's full bit domain, overflow and independent shallow cloning.
    let word = h.named(b"Word");
    for bits in [0, 1, i64::MAX as usize, 1usize << 63, usize::MAX] {
        let w = h.call(word, b"fromBits:", &[h.q.word(bits).unwrap()]);
        assert_eq!(h.q.word_bits(h.call(w, b"bits", &[])).ok(), Some(bits));
        let cloned = h.call(w, b"clone", &[]);
        assert_ne!(w, cloned);
        assert_eq!(h.call(w, b"=", &[cloned]), truth);
        assert_eq!(
            h.text(h.call(w, b"printString", &[])),
            format!("0x{bits:x}").as_bytes()
        );
        let integer = h.call(w, b"asInteger", &[]);
        if bits <= i64::MAX as usize {
            assert_eq!(h.iv(integer), bits as i64)
        } else {
            assert_eq!(h.call(integer, b"kind", &[]), hooks.symbol(b"overflow"))
        }
    }
    // Cross the tail, first trie and second trie growth boundaries.
    let empty = h.call(h.named(b"Vector"), b"new", &[]);
    let mut v = empty;
    let mut snapshots = Vec::new();
    for i in 0..1100 {
        v = h.call(v, b"add:", &[h.i(i)]);
        if [31, 32, 33, 1023, 1024, 1055, 1056, 1099].contains(&i) {
            snapshots.push((v, i + 1));
        }
    }
    h.call(h.gc, b"collectPrecisely", &[]);
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
    let small = h.call(v, b"copyFrom:to:", &[h.i(5), h.i(15)]);
    assert_eq!(h.seq(small), (5..15).collect::<Vec<_>>());
    assert_eq!(
        h.seq(h.call(small, b"reversed", &[])),
        (5..15).rev().collect::<Vec<_>>()
    );
    let joined = h.call(small, b",", &[small]);
    assert_eq!(h.seq(joined), (5..15).chain(5..15).collect::<Vec<_>>());
    assert_eq!(h.iv(h.call(small, b"indexOf:", &[h.i(11)])), 6);
    let fault = hooks.fault(57);
    let context = hooks.context(&[
        h.gc,
        h.named(b"Integer"),
        size(1).unwrap(),
        size(0).unwrap(),
        ptr::null_mut(),
        ptr::null_mut(),
        fault,
        probe,
    ]);
    let replacement = method(b"replacement", context);
    hooks.put(context, 5, replacement);
    let (block, b) = callback_object(b"value:", method(b"callback", context));
    hooks.put(context, 4, b);
    let sorted = h.call(small, b"sortBy:", &[block]);
    let mut expected: Vec<i64> = (5..15).collect();
    expected.sort_by_key(|n| n % 3);
    assert_eq!(h.seq(sorted), expected);
    assert_eq!(hooks.count(context, 3), 10);
    assert_eq!(as_size(h.call(probe, b"observed", &[])), Some(10));
    h.call(h.gc, b"collectPrecisely", &[]);
    assert_eq!(as_size(h.call(probe, b"mask", &[])), Some(0));
    h.call(probe, b"reset", &[]);
    // Native failure also releases every key accumulated before the failure.
    hooks.put(context, 2, size(5).unwrap());
    hooks.put(context, 3, size(0).unwrap());
    assert_eq!(
        unsafe { host.id.send(small, hooks.symbol(b"sortBy:"), &[block]) }.unwrap_err(),
        fault
    );
    assert_eq!(as_size(h.call(probe, b"observed", &[])), Some(2));
    h.call(h.gc, b"collectPrecisely", &[]);
    assert_eq!(as_size(h.call(probe, b"mask", &[])), Some(0));
    h.call(probe, b"reset", &[]);
    hooks.put(context, 3, size(0).unwrap());
    hooks.put(context, 2, size(4).unwrap());
    h.call(small, b"do:", &[block]);
    assert_eq!(hooks.count(context, 3), 901);
    hooks.put(context, 2, size(2).unwrap());
    let (fold, _) = callback_object(b"value:value:", method(b"callback", context));
    assert_eq!(
        h.iv(h.call(small, b"inject:into:", &[h.i(0), fold])),
        (5..15).sum::<i64>()
    );
    assert_eq!(
        h.text(h.call(small, b"printString", &[])),
        b"#[5 6 7 8 9 10 11 12 13 14]"
    );
    h.call(h.gc, b"collectPrecisely", &[]);
    assert_eq!(as_size(h.call(probe, b"observed", &[])), Some(10));
    assert_eq!(as_size(h.call(probe, b"mask", &[])), Some(1 << 9));
    h.call(probe, b"reset", &[]);
    // Map persistence and independent value-key probes, including signed zero.
    let map = h.call(h.named(b"Map"), b"new", &[]);
    let mut m = map;
    for i in 0..200 {
        m = h.put(m, h.i(i), h.i(i * 7));
    }
    h.call(h.gc, b"collectPrecisely", &[]);
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
    let wa = h.call(word, b"fromBits:", &[h.q.word(5).unwrap()]);
    let wb = h.call(word, b"fromBits:", &[h.q.word(5).unwrap()]);
    m = h.put(m, wa, h.i(15));
    assert_eq!(h.get(m, wb), nil);
    assert_eq!(h.iv(h.get(m, wa)), 15);
    let symbol = hooks.symbol(b"symbol-key");
    m = h.put(m, symbol, h.i(16));
    assert_eq!(h.iv(h.get(m, symbol)), 16);
    assert_eq!(h.call(m, b"includesKey:", &[h.i(-999)]), falsity);
    assert_eq!(h.call(m, b"includesKey:", &[h.i(17)]), truth);
    hooks.put(context, 2, size(3).unwrap());
    let (fallback, _) = callback_object(b"value", method(b"callback", context));
    assert_eq!(
        h.iv(h.call(m, b"at:ifAbsent:", &[h.i(-999), fallback])),
        789
    );
    assert_eq!(h.size(h.call(m, b"keys", &[])), h.size(m));
    assert_eq!(h.size(h.call(m, b"values", &[])), h.size(m));
    // Force full-hash collision nodes without knowing Map's representation.
    let cb = h.call(h.named(b"ObjectVt"), b"delegated", &[]);
    h.call(
        cb,
        b"addMethod",
        &[
            hooks.symbol(b"collectionHash"),
            hooks.method(b"fixed", size(7).unwrap()),
        ],
    );
    let mut cm = map;
    let mut keys = Vec::new();
    for i in 0..40 {
        let key = h.call(cb, b"allocate", &[size(0).unwrap()]);
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
    let eq = hooks.context(&[h.gc, map, h.i(99), h.i(98), truth, size(0).unwrap()]);
    h.call(
        cb,
        b"addMethod",
        &[
            hooks.symbol(b"collectionEquals:"),
            method(b"nestedEqual", eq),
        ],
    );
    let one = h.put(map, keys[0], h.i(1));
    let two = h.put(one, keys[1], h.i(2));
    assert_eq!(h.size(two), 1);
    assert!(hooks.count(eq, 5) > 0);
    // Range bounds use wide loop arithmetic, including both i64 endpoints.
    hooks.put(context, 2, size(2).unwrap());
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
    let copied = h.call(range, b"clone", &[]);
    h.call(h.gc, b"collectPrecisely", &[]);
    assert_eq!(h.size(copied), 5);
    assert_eq!(h.iv(h.at(copied, 0)), 3);
    assert_eq!(h.iv(h.at(copied, 4)), 7);
    assert_eq!(h.call(range, b"at:", &[h.f(1.0)]), nil);
    assert_eq!(h.iv(h.call(range, b"inject:into:", &[h.i(0), fold])), 25);
    assert_eq!(h.call(range, b"includes:", &[h.i(5)]), truth);
    assert_eq!(h.call(range, b"includes:", &[h.f(5.0)]), falsity);
    assert_eq!(h.text(h.call(range, b"printString", &[])), b"#[3 4 5 6 7]");
    assert_eq!(
        h.seq(h.call(range, b",", &[small])),
        (3..=7).chain(5..15).collect::<Vec<_>>()
    );
    // Binary-safe split preserves empty/trailing fields and live factories.
    let sb = h.named(b"StringVt");
    let vb = h.named(b"VectorVt");
    let original_string = h.call(sb, b"lookup", &[hooks.symbol(b"fromBytes:")]);
    let original_add = h.call(vb, b"lookup", &[hooks.symbol(b"add:")]);
    let sw = hooks.context(&[original_string, h.gc, size(0).unwrap()]);
    let vw = hooks.context(&[original_add, h.gc, size(0).unwrap()]);
    h.call(
        sb,
        b"addMethod",
        &[
            hooks.symbol(b"fromBytes:"),
            hooks.method(b"collectEvery", sw),
        ],
    );
    h.call(
        vb,
        b"addMethod",
        &[hooks.symbol(b"add:"), hooks.method(b"collectEvery", vw)],
    );
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
        let result = h.call(text, b"split:", &[h.s(separator)]);
        assert_eq!(h.size(result), parts.len() as i64);
        for (i, part) in parts.iter().enumerate() {
            assert_eq!(h.text(h.at(result, i as i64)), *part);
        }
    }
    let text = h.s(b"unchanged");
    let split = h.call(text, b"split:", &[nil]);
    assert_eq!(h.at(split, 0), text);
    assert!(hooks.count(sw, 2) > 5 && hooks.count(vw, 2) > 5);
    h.call(
        sb,
        b"addMethod",
        &[hooks.symbol(b"fromBytes:"), original_string],
    );
    h.call(vb, b"addMethod", &[hooks.symbol(b"add:"), original_add]);
    // Numeric bits must not retain the object whose address they happen to match.
    h.call(h.gc, b"precisePayloads:", &[size(1).unwrap()]);
    let model = h.named(b"ObjectModel");
    {
        let victims = host.scope();
        let victim = victims
            .call(
                model,
                b"allocate:bytes:",
                &[h.named(b"ObjectVt"), size(0).unwrap()],
            )
            .unwrap();
        let raw_word = h.call(word, b"fromBits:", &[h.q.word(victim.addr()).unwrap()]);
        let raw_range = h.range(victim.addr() as i64, victim.addr() as i64);
        h.call(host.id.root, b"ensurePrivateVtableFor:", &[raw_word]);
        h.call(host.id.root, b"ensurePrivateVtableFor:", &[raw_range]);
        host.send(probe, b"watch", &[victim]).unwrap();
    }
    h.call(h.gc, b"collectPrecisely", &[]);
    assert_eq!(as_size(h.call(probe, b"mask", &[])), Some(0));
    // Malformed language-visible reference slots fail instead of reading outside storage.
    let bad_map = h.call(map, b"clone", &[]);
    h.call(
        model,
        b"slotAt:in:put:",
        &[h.q.word(0).unwrap(), bad_map, h.i(1)],
    );
    assert!(host.send(bad_map, b"at:", &[h.i(1)]).is_err());
    h.call(
        model,
        b"slotAt:in:put:",
        &[h.q.word(0).unwrap(), bad_map, ptr::null_mut()],
    );
    let bad_vector = h.call(empty, b"add:", &[nil]);
    h.call(
        model,
        b"slotAt:in:put:",
        &[h.q.word(3).unwrap(), bad_vector, h.i(1)],
    );
    assert!(host.send(bad_vector, b"at:", &[h.i(0)]).is_err());
    // Private collection behavior still retains and operates on its own references.
    h.call(host.id.root, b"ensurePrivateVtableFor:", &[small]);
    h.call(host.id.root, b"ensurePrivateVtableFor:", &[m]);
    h.call(h.gc, b"collectPrecisely", &[]);
    assert_eq!(h.seq(small), (5..15).collect::<Vec<_>>());
    assert_eq!(h.iv(h.get(m, h.i(17))), -5);
    // Collection edges alone retain text; releasing the holder releases its contents.
    {
        let holder_roots = host.scope();
        {
            let text_roots = host.scope();
            let text = text_roots.string(b"only-through-vector").unwrap();
            holder_roots.call(empty, b"add:", &[text]).unwrap();
            host.send(probe, b"watch", &[text]).unwrap();
        }
        h.call(h.gc, b"collectPrecisely", &[]);
        assert_eq!(as_size(h.call(probe, b"mask", &[])), Some(1));
    }
    h.call(h.gc, b"collectPrecisely", &[]);
    assert_eq!(as_size(h.call(probe, b"mask", &[])), Some(0));
    {
        let holder_roots = host.scope();
        {
            let text_roots = host.scope();
            let key = text_roots.string(b"only-through-map-key").unwrap();
            let value = text_roots.string(b"only-through-map-value").unwrap();
            holder_roots.call(map, b"at:put:", &[key, value]).unwrap();
            host.send(probe, b"watch", &[key, value]).unwrap();
        }
        h.call(h.gc, b"collectPrecisely", &[]);
        assert_eq!(as_size(h.call(probe, b"mask", &[])), Some(3));
    }
    h.call(h.gc, b"collectPrecisely", &[]);
    assert_eq!(as_size(h.call(probe, b"mask", &[])), Some(0));
    let gc = h.gc;
    drop(hooks);
    drop(h);
    host.send(gc, b"collectPrecisely", &[]).unwrap();
}
