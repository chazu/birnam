//! Isolated authored positive-query slice; canonical Store and bootstrap stay unchanged.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{Operand, Outcome, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
};
use std::os::unix::ffi::OsStrExt;
use support::*;

struct H {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    gc: Oop,
    tokens: RefCell<Vec<usize>>,
}
impl H {
    unsafe fn new() -> Self {
        unsafe {
            let id = id();
            let ns = load(
                id,
                "bootstrap",
                &[Operand::bytes(directory().as_os_str().as_bytes())],
            );
        optional_modules(id, ns, &["store", "relational"]);
            let named = |n: &[u8]| {
                object(send(
                    id,
                    ns,
                    b"at:",
                    &[Operand::object(id.intern_name(n).unwrap())],
                ))
            };
            Self {
                id,
                ns,
                roots: named(b"Roots"),
                gc: named(b"GC"),
                tokens: RefCell::new(vec![]),
            }
        }
    }
    unsafe fn keep(&self, o: Oop) -> Oop {
        self.tokens
            .borrow_mut()
            .push(unsafe { retain(self.id, self.roots, o) });
        o
    }
    unsafe fn named(&self, n: &[u8]) -> Oop {
        unsafe {
            object(send(
                self.id,
                self.ns,
                b"at:",
                &[Operand::object(self.id.intern_name(n).unwrap())],
            ))
        }
    }
    unsafe fn symbol(&self, n: &[u8]) -> Oop {
        unsafe { self.keep(self.id.intern_name(n).unwrap()) }
    }
    unsafe fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                r,
                n,
                &a.iter().map(|o| Operand::object(*o)).collect::<Vec<_>>(),
            )))
        }
    }
    unsafe fn i(&self, n: i64) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                self.named(b"Integer"),
                b"fromWord:",
                &[Operand::word(n as usize)],
            )))
        }
    }
    unsafe fn number(&self, o: Oop) -> i64 {
        unsafe {
            send(
                self.id,
                self.named(b"Integer"),
                b"wordOf:",
                &[Operand::object(o)],
            )
            .as_word()
            .unwrap() as i64
        }
    }
    unsafe fn metric(&self, r: Oop, n: &[u8]) -> i64 {
        unsafe { self.number(self.call(r, n, &[])) }
    }
    unsafe fn clear(&self) {
        for t in self.tokens.take() {
            unsafe { release(self.id, self.roots, t) }
        }
    }
    unsafe fn eval(&self, source: &[u8]) -> Oop {
        unsafe {
            let text = self.keep(object(send(
                self.id,
                self.named(b"String"),
                b"fromBytes:",
                &[Operand::bytes(source)],
            )));
            let result = self.call(self.named(b"Codegen"), b"eval:", &[text]);
            if self.call(result, b"isError", &[]) == self.named(b"true") {
                let message = self.call(result, b"message", &[]);
                let bytes = send(self.id, message, b"bytes", &[]).as_bytes().unwrap();
                panic!(
                    "source {}: {}",
                    String::from_utf8_lossy(source),
                    String::from_utf8_lossy(
                        birnam_abi::borrowed_bytes(bytes.pointer, bytes.length).unwrap()
                    )
                );
            }
            result
        }
    }
    unsafe fn assert(&self, st: Oop, e: Oop, a: Oop, v: Oop) {
        assert_eq!(
            unsafe { self.call(st, b"assert:attr:value:", &[e, a, v]) },
            st
        );
    }
}

impl H {
    unsafe fn vector(&self, values: &[Oop]) -> Oop {
        unsafe {
            let mut v = self.call(self.named(b"Vector"), b"new", &[]);
            for value in values {
                v = self.call(v, b"add:", &[*value]);
            }
            v
        }
    }
    unsafe fn elements(&self, v: Oop) -> Vec<Oop> {
        unsafe {
            (0..self.metric(v, b"size"))
                .map(|i| self.call(v, b"at:", &[self.i(i)]))
                .collect()
        }
    }
    unsafe fn nums(&self, v: Oop) -> Vec<i64> {
        unsafe { self.elements(v).iter().map(|o| self.number(*o)).collect() }
    }
    unsafe fn query(&self, st: Oop, result: Oop, clauses: &[&[Oop]]) -> Oop {
        unsafe {
            let clauses = clauses.iter().map(|c| self.vector(c)).collect::<Vec<_>>();
            self.call(st, b"findAll:where:", &[result, self.vector(&clauses)])
        }
    }
}

unsafe fn setup(h: &H) -> Oop {
    unsafe {
        let path = directory().join("libbirnam_stdlib.dylib");
        let text = h.keep(object(send(
            h.id,
            h.named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(path.as_os_str().as_bytes())],
        )));
        h.call(h.named(b"Codegen"), b"load:", &[text]);
        h.eval(b"[Module markLoaded: #stdlib]; true");
        let source = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../modules/relational-query.birnam"
        ))
        .unwrap();
        let text = h.keep(object(send(
            h.id,
            h.named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(&source)],
        )));
        let output = std::env::temp_dir().join(format!(
            "birnam-relational-slice-{}.dylib",
            std::process::id()
        ));
        let out = h.keep(object(send(
            h.id,
            h.named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(output.as_os_str().as_bytes())],
        )));
        let name = h.eval(b"\"modules/relational-query.birnam\"");
        let answer = h.call(
            h.named(b"Codegen"),
            b"compile:output:from:synchronousBodies:",
            &[text, out, name, h.named(b"true")],
        );
        if h.call(answer, b"isError", &[]) == h.named(b"true") {
            let msg = h.call(answer, b"message", &[]);
            let raw = send(h.id, msg, b"bytes", &[]).as_bytes().unwrap();
            panic!(
                "compile: {}",
                String::from_utf8_lossy(
                    birnam_abi::borrowed_bytes(raw.pointer, raw.length).unwrap()
                )
            );
        }
        assert_ne!(answer, h.named(b"nil"));
        assert_eq!(h.call(answer, b"isError", &[]), h.named(b"false"));
        h.call(h.named(b"Codegen"), b"load:", &[out]);
        h.named(b"RelationalQuery")
    }
}
unsafe fn query(h: &H, owner: Oop, store: Oop, result: Oop, clauses: &[&[Oop]]) -> Oop {
    unsafe {
        let facade = h.call(owner, b"on:", &[store]);
        let rows = clauses.iter().map(|row| h.vector(row)).collect::<Vec<_>>();
        let answer = h.call(facade, b"findAll:where:", &[result, h.vector(&rows)]);
        if answer == h.named(b"nil") || h.call(answer, b"isError", &[]) == h.named(b"true") {
            let printed = h.call(answer, b"printString", &[]);
            let raw = send(h.id, printed, b"bytes", &[]).as_bytes().unwrap();
            panic!(
                "query: {}",
                String::from_utf8_lossy(
                    birnam_abi::borrowed_bytes(raw.pointer, raw.length).unwrap()
                )
            );
        }
        answer
    }
}

struct EqualProbe {
    h: *const H,
    behavior: Oop,
    owner: Oop,
    store: Oop,
    x: Oop,
    y: Oop,
    attr: Oop,
    mode: Cell<usize>,
    calls: Cell<usize>,
    pairs: RefCell<Vec<(Oop, Oop)>>,
    result: Oop,
}
unsafe extern "C" fn current_equal(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<EqualProbe>();
        let h = &*p.h;
        let [arg] = borrowed_slice(a, n).unwrap() else {
            panic!("equality arity")
        };
        p.pairs.borrow_mut().push((r, object(*arg)));
        p.calls.set(p.calls.get() + 1);
        collect(h.id, h.gc);
        Outcome::loaded(Operand::object(h.named(b"true")))
    }
}
unsafe extern "C" fn first_equal(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<EqualProbe>();
        let h = &*p.h;
        let [arg] = borrowed_slice(a, n).unwrap() else {
            panic!("equality arity")
        };
        p.pairs.borrow_mut().push((r, object(*arg)));
        p.calls.set(p.calls.get() + 1);
        match p.mode.get() {
            0 => {
                install(h.id, p.behavior, b"=", current_equal, c);
                Outcome::loaded(Operand::object(h.named(b"false")))
            }
            1 => {
                if p.calls.get() == 1 {
                    assert!(
                        !h.elements(query(h, p.owner, p.store, p.y, &[&[p.x, p.attr, p.y]]))
                            .is_empty()
                    );
                }
                collect(h.id, h.gc);
                Outcome::loaded(Operand::object(h.named(b"true")))
            }
            2 => Outcome::failed(719, 37),
            _ => Outcome::loaded(Operand::object(p.result)),
        }
    }
}
struct Emitter {
    h: *const H,
    collector: Cell<Oop>,
    entity: Cell<Oop>,
    mode: Cell<usize>,
    attr: Oop,
}
unsafe extern "C" fn emit(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Emitter>();
        let h = &*p.h;
        let [sink] = borrowed_slice(a, n).unwrap() else {
            panic!("snapshot arity")
        };
        let sink = h.keep(object(*sink));
        p.collector.set(sink);
        let e = allocate(h.id, h.named(b"EntityVt"), 0);
        p.entity.set(e);
        let token = retain(h.id, h.roots, e);
        let result = h.id.send_named(
            sink,
            b"value:value:value:",
            &[
                Operand::object(e),
                Operand::object(p.attr),
                Operand::object(h.named(b"nil")),
            ],
        );
        release(h.id, h.roots, token);
        collect(h.id, h.gc);
        assert!(present(h.id, e));
        result.unwrap();
        if p.mode.get() == 1 {
            return Outcome::failed(721, 39);
        }
        if p.mode.get() == 2 {
            assert!(
                h.id.send_named(sink, b"value:value:value:", &[Operand::word(1)])
                    .is_err()
            );
        }
        Outcome::loaded(Operand::object(h.named(b"nil")))
    }
}
unsafe fn compatible_graphs(h: &H, owner: Oop, x: Oop, y: Oop, z: Oop, attr: Oop) {
    unsafe {
        let nodes = [h.symbol(b"node0"), h.symbol(b"node1")];
        for bits in 0..16 {
            let checkpoint = h.tokens.borrow().len();
            let store = h.call(h.named(b"Store"), b"new", &[]);
            for i in 0..2 {
                for j in 0..2 {
                    if bits & (1 << (2 * i + j)) != 0 {
                        h.assert(store, nodes[i], attr, nodes[j]);
                    }
                }
            }
            for (result, clauses) in [
                (y, vec![vec![x, attr, y]]),
                (x, vec![vec![x, attr, x]]),
                (z, vec![vec![x, attr, y], vec![y, attr, z]]),
                (y, vec![vec![nodes[0], attr, y]]),
            ] {
                let clauses = clauses.iter().map(Vec::as_slice).collect::<Vec<_>>();
                assert_eq!(
                    h.elements(query(h, owner, store, result, &clauses)),
                    h.elements(h.query(store, result, &clauses)),
                    "graph={bits}"
                );
            }
            let extra = h.tokens.borrow_mut().split_off(checkpoint);
            for token in extra {
                release(h.id, h.roots, token);
            }
            collect(h.id, h.gc);
        }
    }
}
unsafe fn live_checks(h: &H, owner: Oop, _store: Oop, x: Oop, y: Oop, attr: Oop, expected: &[Oop]) {
    unsafe {
        let store = h.call(h.named(b"Store"), b"new", &[]);
        for (i, &value) in expected.iter().enumerate() {
            h.assert(store, h.i(i as i64), attr, value);
        }
        let pattern=h.eval(b"let p := [Object clone]; [Reflect defineMethod: #= on: p as: [:self :other | false]]; p");
        let vt = object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(pattern)],
        ));
        let error = h.call(
            h.named(b"Error"),
            b"kind:message:",
            &[h.symbol(b"queryProbe"), h.eval(b"\"live equality error\"")],
        );
        let mut probe = EqualProbe {
            h,
            behavior: vt,
            owner,
            store,
            x,
            y,
            attr,
            mode: Cell::new(0),
            calls: Cell::new(0),
            pairs: RefCell::new(vec![]),
            result: error,
        };
        let ctx = (&raw mut probe).cast();
        install(h.id, vt, b"=", first_equal, ctx);
        let order = h.nums(h.query(store, x, &[&[x, attr, y]]));
        assert_eq!(
            h.nums(query(h, owner, store, x, &[&[x, attr, pattern]])),
            order[1..].to_vec()
        );
        assert_eq!(probe.calls.get(), 3); // first binding changes the next two sends
        let pairs = probe.pairs.borrow().clone();
        assert!(pairs.iter().all(|(l, _)| *l == pattern));
        // Reentrant equality uses an independent collector and preserves outer roots.
        probe.calls.set(0);
        probe.mode.set(1);
        install(h.id, vt, b"=", first_equal, ctx);
        query(h, owner, store, x, &[&[x, attr, pattern]]);
        assert_eq!(probe.calls.get(), 3);
        let facade = h.call(owner, b"on:", &[store]);
        let body = h.vector(&[h.vector(&[x, attr, pattern])]);
        for mode in [2, 3] {
            probe.calls.set(0);
            probe.mode.set(mode);
            let result = h.id.send_named(
                facade,
                b"findAll:where:",
                &[Operand::object(x), Operand::object(body)],
            );
            if mode == 2 {
                let e = result.unwrap_err();
                assert_eq!((e.status, e.detail), (719, 37));
            } else {
                assert_eq!(result.unwrap().as_object(), Some(error));
            }
            assert_eq!(
                probe.calls.get(),
                1,
                "no comparisons after the first failure"
            );
        }
        let dedup = h.call(h.named(b"Store"), b"new", &[]);
        let value2 = h.eval(b"[Object clone]");
        let value3 = h.eval(b"[Object clone]");
        for (i, value) in [pattern, value2, value3].into_iter().enumerate() {
            h.assert(dedup, h.i(i as i64), attr, value);
        }
        probe.mode.set(0);
        probe.calls.set(0);
        probe.pairs.borrow_mut().clear();
        install(h.id, vt, b"=", first_equal, ctx);
        assert_eq!(
            h.elements(query(h, owner, dedup, y, &[&[x, attr, y]])),
            vec![pattern, value2]
        );
        assert_eq!(
            &*probe.pairs.borrow(),
            &[(pattern, value2), (pattern, value3)],
            "dedup sends in first-seen receiver order"
        );
        // Equality is not silently repaired into an equivalence relation.
        let repeated = h.call(h.named(b"Store"), b"new", &[]);
        for i in 0..3 {
            h.assert(repeated, h.i(i), attr, pattern);
        }
        probe.result = h.named(b"false");
        probe.mode.set(3);
        probe.calls.set(0);
        install(h.id, vt, b"=", first_equal, ctx);
        assert_eq!(
            h.elements(query(h, owner, repeated, y, &[&[x, attr, y]])),
            vec![pattern; 3]
        );
        assert_eq!(
            probe.calls.get(),
            3,
            "even identical Objects use current value equality"
        );
        // A replacement provider may discard its only original reference after emission.
        let fake=h.eval(b"let s := [Object clone]; [Reflect defineMethod: #liveTriplesDo: on: s as: [:self :sink | nil]]; s");
        let fvt = object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(fake)],
        ));
        let mut emitter = Emitter {
            h,
            collector: Cell::new(core::ptr::null_mut()),
            entity: Cell::new(core::ptr::null_mut()),
            mode: Cell::new(0),
            attr,
        };
        install(
            h.id,
            fvt,
            b"liveTriplesDo:",
            emit,
            (&raw mut emitter).cast(),
        );
        let facade = h.call(owner, b"on:", &[fake]);
        let body = h.vector(&[h.vector(&[x, attr, y])]);
        for mode in [0, 1, 2] {
            emitter.mode.set(mode);
            let result = h.id.send_named(
                facade,
                b"findAll:where:",
                &[Operand::object(x), Operand::object(body)],
            );
            if mode == 0 {
                let out = object(result.unwrap());
                let token = retain(h.id, h.roots, out);
                assert_eq!(
                    send(h.id, out, b"at:", &[Operand::object(h.i(0))]).as_object(),
                    Some(emitter.entity.get())
                );
                collect(h.id, h.gc);
                assert!(present(h.id, emitter.entity.get()));
                release(h.id, h.roots, token);
            } else {
                let e = result.unwrap_err();
                assert_eq!(
                    (e.status, e.detail),
                    if mode == 1 { (721, 39) } else { (1, 0) }
                );
            }
            assert!(
                h.id.send_named(
                    emitter.collector.get(),
                    b"value:value:value:",
                    &[Operand::object(x); 3]
                )
                .is_err()
            );
            collect(h.id, h.gc);
            assert!(
                !present(h.id, emitter.entity.get()),
                "expired collector retained its snapshot"
            );
        }
        // Remove callbacks holding native test stack pointers before returning.
        send(
            h.id,
            vt,
            b"removeMethod:",
            &[Operand::object(h.symbol(b"="))],
        );
        send(
            h.id,
            fvt,
            b"removeMethod:",
            &[Operand::object(h.symbol(b"liveTriplesDo:"))],
        );
    }
}
#[test]
#[ignore = "requires production prefix; run from repository root"]
fn authored_queries_preserve_order_and_use_live_world_equality() {
    unsafe {
        let h = H::new();
        let owner = setup(&h);
        let store = h.call(h.named(b"Store"), b"new", &[]);
        let x = h.call(h.named(b"LogicVar"), b"named:", &[h.symbol(b"x")]);
        let y = h.call(h.named(b"LogicVar"), b"named:", &[h.symbol(b"y")]);
        let z = h.call(h.named(b"LogicVar"), b"named:", &[h.symbol(b"z")]);
        let attr = h.symbol(b"edge");
        let other = h.symbol(b"value");
        let (a, b, c) = (h.symbol(b"a"), h.symbol(b"b"), h.symbol(b"c"));
        h.assert(store, a, attr, b);
        h.assert(store, b, attr, c);
        h.assert(store, c, attr, c);
        assert_eq!(
            h.elements(query(&h, owner, store, y, &[&[x, attr, y]])),
            vec![b, c]
        );
        assert_eq!(
            h.elements(query(&h, owner, store, z, &[&[x, attr, y], &[y, attr, z]])),
            vec![c]
        );
        assert_eq!(
            h.elements(query(&h, owner, store, x, &[&[x, attr, x]])),
            vec![c]
        );
        assert!(
            h.elements(query(&h, owner, store, z, &[&[x, attr, y]]))
                .is_empty()
        );
        let nil = h.named(b"nil");
        h.assert(store, a, other, nil);
        assert_eq!(
            h.elements(query(&h, owner, store, y, &[&[a, other, y]])),
            vec![nil]
        );
        let p=h.eval(b"let q := [Object clone]; [Reflect defineMethod: #= on: q as: [:self :other | true]]; q");
        assert_eq!(
            h.elements(query(&h, owner, store, y, &[&[p, attr, y]])),
            vec![b, c]
        );
        assert!(
            h.elements(h.query(store, y, &[&[p, attr, y]])).is_empty(),
            "the native kernel is not the replacement-sensitive oracle"
        );
        // Project actual snapshot objects rather than reconstructing scalar values.
        let text = h.eval(b"\"original scalar\"");
        h.assert(store, b, other, text);
        assert_eq!(
            h.elements(query(&h, owner, store, y, &[&[b, other, y]])),
            vec![text]
        );
        compatible_graphs(&h, owner, x, y, z, attr);
        live_checks(&h, owner, store, x, y, attr, &[b, c, c]);
        // Unsupported syntax is explicitly outside the positive-query slice.
        let facade = h.call(owner, b"on:", &[store]);
        for body in [
            h.vector(&[h.vector(&[x, attr])]),
            h.vector(&[h.vector(&[h.symbol(b"rule"), attr, x])]),
            h.vector(&[h.vector(&[x, h.symbol(b"="), y])]),
        ] {
            assert_eq!(
                h.call(facade, b"findAll:where:", &[x, body]),
                h.named(b"nil")
            );
        }
        assert_eq!(
            h.call(facade, b"findAll:where:", &[a, h.vector(&[])]),
            h.named(b"nil")
        );
        if let Some(path) = std::env::var_os("BIRNAM_RELATIONAL_BENCH_OUT") {
            benchmark(&h, owner, x, y, z, attr, &std::path::PathBuf::from(path));
        }
        h.clear();
        collect(h.id, h.gc);
    }
}

#[path = "../examples/micro_support/clock.rs"]
mod clock;
unsafe fn benchmark(h: &H, owner: Oop, x: Oop, y: Oop, z: Oop, attr: Oop, path: &std::path::Path) {
    unsafe {
        let mut rows = Vec::new();
        let mut census = Vec::new();
        let selector = h.symbol(b"findAll:where:");
        for n in [8usize, 32] {
            let store = h.call(h.named(b"Store"), b"new", &[]);
            for i in 0..n {
                h.assert(store, h.i(i as i64), attr, h.i(((i + 1) % n) as i64));
            }
            let facade = h.call(owner, b"on:", &[store]);
            for joined in [false, true] {
                let mut clauses = vec![h.vector(&[x, attr, y])];
                if joined {
                    clauses.push(h.vector(&[y, attr, z]));
                }
                let body = h.vector(&clauses);
                let args = [
                    Operand::object(if joined { z } else { y }),
                    Operand::object(body),
                ];
                for receiver in [store, facade] {
                    let result = object(h.id.send(receiver, selector, &args).unwrap());
                    assert_eq!(h.metric(result, b"size"), n as i64);
                }
                for (variant, receiver) in [("rust", store), ("birnam", facade)] {
                    collect(h.id, h.gc);
                    send(h.id, h.gc, b"automaticCollection:", &[Operand::word(0)]);
                    let before = send(h.id, h.gc, b"allocationBytes", &[]).as_word().unwrap();
                    let result = object(h.id.send(receiver, selector, &args).unwrap());
                    let after = send(h.id, h.gc, b"allocationBytes", &[]).as_word().unwrap();
                    let token = retain(h.id, h.roots, result);
                    assert_eq!(h.metric(result, b"size"), n as i64);
                    release(h.id, h.roots, token);
                    census.push(serde_json::json!({"facts":n,"join":joined,"variant":variant,"allocated_bytes":after-before}));
                    send(h.id, h.gc, b"automaticCollection:", &[Operand::word(1)]);
                }
                for pair in 0..8 {
                    for v in if pair % 2 == 0 { [0, 1] } else { [1, 0] } {
                        collect(h.id, h.gc);
                        send(h.id, h.gc, b"budget:", &[Operand::word(16 * 1024 * 1024)]);
                        let start = clock::cpu_ns();
                        let wall = std::time::Instant::now();
                        let result =
                            object(h.id.send([store, facade][v], selector, &args).unwrap());
                        let elapsed = wall.elapsed().as_nanos();
                        let cpu = clock::cpu_ns() - start;
                        let token = retain(h.id, h.roots, result);
                        assert_eq!(h.metric(result, b"size"), n as i64);
                        release(h.id, h.roots, token);
                        rows.push(serde_json::json!({"facts":n,"join":joined,"pair":pair,"variant":(["rust","birnam"][v]),"cpu_ns":cpu,"wall_ns":elapsed}));
                    }
                }
            }
        }
        std::fs::write(path,serde_json::to_vec_pretty(&serde_json::json!({"gc_budget":16*1024*1024,"scope":"single-query alternating process-local samples; different dispatch/identity semantics; not optimizer speedup evidence","timings":rows,"allocations":census})).unwrap()).unwrap();
    }
}
