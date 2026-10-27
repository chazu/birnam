//! Relational queries and callbacks through production Oop message owners.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use birnam_host_support::{Context, Scope};
use std::{cell::RefCell, path::Path, ptr};
struct H<'a> {
    host: &'a Context,
    _floor: Scope<'a>,
    hooks: oop::hooks::Hooks<'a>,
    tokens: RefCell<Vec<Oop>>,
    retain: Oop,
    release: Oop,
    gc: Oop,
    fixture: Oop,
    probe: Oop,
}
impl<'a> H<'a> {
    fn new(host: &'a Context) -> Self {
        let q = host.scope();
        let gc = host.named(b"GC").unwrap();
        q.call(gc, b"precisePayloads:", &[size(1).unwrap()])
            .unwrap();
        for name in ["store", "relational"] {
            host.load_native(
                &q,
                &host
                    .resources
                    .lib_dir
                    .join(format!("native/libbirnam_{name}.dylib")),
                &[host.namespace],
            )
            .unwrap();
        }
        let path =
            std::env::temp_dir().join(format!("birnam-relational-{}.dylib", std::process::id()));
        assert!(
            std::process::Command::new("clang")
                .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
                .arg(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../tests/fixtures/oop-host/relational.c")
                )
                .arg("-o")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let fixture = host.load_native(&q, &path, &[]).unwrap();
        std::fs::remove_file(path).unwrap();
        let probe = q
            .call(host.named(b"Object").unwrap(), b"clone", &[])
            .unwrap();
        let h = Self {
            host,
            _floor: q,
            hooks: oop::hooks::Hooks::new(host),
            tokens: RefCell::new(vec![]),
            retain: host.intern(b"retain:").unwrap(),
            release: host.intern(b"release:").unwrap(),
            gc,
            fixture,
            probe,
        };
        let vt = h.call(host.id.root, b"ensurePrivateVtableFor:", &[probe]);
        for n in [b"allocation:bytes:identity:".as_slice(), b"present"] {
            h.bind(vt, n, h.method(n, probe));
        }
        h
    }
    fn named(&self, n: &[u8]) -> Oop {
        self.host.named(n).unwrap()
    }
    fn keep(&self, v: Oop) -> Oop {
        self.tokens
            .borrow_mut()
            .push(unsafe { self.host.id.send(self.host.roots, self.retain, &[v]) }.unwrap());
        v
    }
    fn symbol(&self, n: &[u8]) -> Oop {
        self.keep(self.host.intern(n).unwrap())
    }
    fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        self.keep(self.host.send(r, n, a).unwrap())
    }
    fn text(&self, b: &[u8]) -> Oop {
        let q = self.host.scope();
        self.keep(q.string(b).unwrap())
    }
    fn word(&self, n: usize) -> Oop {
        let q = self.host.scope();
        self.keep(q.word(n).unwrap())
    }
    fn i(&self, n: i64) -> Oop {
        self.call(
            self.named(b"Integer"),
            b"fromWord:",
            &[self.word(n as usize)],
        )
    }
    fn number(&self, v: Oop) -> i64 {
        self.host
            .scope()
            .word_bits(self.call(self.named(b"Integer"), b"wordOf:", &[v]))
            .unwrap() as i64
    }
    fn metric(&self, r: Oop, n: &[u8]) -> i64 {
        self.number(self.call(r, n, &[]))
    }
    fn clear(&self) {
        for t in self.tokens.take().into_iter().rev() {
            unsafe { self.host.id.send(self.host.roots, self.release, &[t]) }.unwrap();
        }
    }
    fn eval(&self, s: &[u8]) -> Oop {
        let result = self.call(self.named(b"Codegen"), b"eval:", &[self.text(s)]);
        self.host.scope().check_result(result).unwrap();
        result
    }
    fn assert(&self, st: Oop, e: Oop, a: Oop, v: Oop) {
        assert_eq!(self.call(st, b"assert:attr:value:", &[e, a, v]), st);
    }
    fn vector(&self, values: &[Oop]) -> Oop {
        let mut v = self.call(self.named(b"Vector"), b"new", &[]);
        for value in values {
            v = self.call(v, b"add:", &[*value]);
        }
        v
    }
    fn elements(&self, v: Oop) -> Vec<Oop> {
        (0..self.metric(v, b"size"))
            .map(|i| self.call(v, b"at:", &[self.i(i)]))
            .collect()
    }
    fn nums(&self, v: Oop) -> Vec<i64> {
        self.elements(v).iter().map(|v| self.number(*v)).collect()
    }
    fn query(&self, st: Oop, result: Oop, clauses: &[&[Oop]]) -> Oop {
        let clauses = clauses.iter().map(|c| self.vector(c)).collect::<Vec<_>>();
        self.call(st, b"findAll:where:", &[result, self.vector(&clauses)])
    }
    fn context(&self, a: &[Oop]) -> Oop {
        self.call(self.host.id.root, b"sequence", a)
    }
    fn field(&self, c: Oop, i: usize) -> Oop {
        self.call(c, b"at:", &[size(i).unwrap()])
    }
    fn count(&self, c: Oop, i: usize) -> usize {
        as_size(self.field(c, i)).unwrap()
    }
    fn bind(&self, vt: Oop, n: &[u8], m: Oop) {
        self.call(vt, b"addMethod", &[self.symbol(n), m]);
    }
    fn method(&self, n: &[u8], c: Oop) -> Oop {
        self.call(
            self.call(self.fixture, b"methodAt:", &[self.symbol(n)]),
            b"withContext:",
            &[c],
        )
    }
    fn save(&self, vt: Oop, n: &[u8], alias: &[u8]) -> Oop {
        let m = self.call(vt, b"lookup", &[self.symbol(n)]);
        self.bind(vt, alias, m);
        m
    }
    fn restore(&self, vt: Oop, n: &[u8], alias: &[u8], m: Oop) {
        self.bind(vt, n, m);
        self.call(vt, b"removeMethod:", &[self.symbol(alias)]);
    }
    fn collect(&self) {
        self.call(self.gc, b"collectPrecisely", &[]);
    }
}
impl Drop for H<'_> {
    fn drop(&mut self) {
        self.clear();
    }
}
#[test]
#[ignore = "requires production Oop modules"]
fn relational_preserves_queries_rules_history_and_live_messages() {
    let host = oop::bootstrap();
    let h = H::new(&host);
    let st = h.call(h.named(b"Store"), b"new", &[]);
    let x = h.call(h.named(b"LogicVar"), b"named:", &[h.symbol(b"x")]);
    let y = h.call(h.named(b"LogicVar"), b"named:", &[h.symbol(b"y")]);
    let z = h.call(h.named(b"LogicVar"), b"named:", &[h.symbol(b"z")]);
    assert_eq!(
        x,
        h.call(h.named(b"LogicVar"), b"named:", &[h.symbol(b"x")])
    );
    assert_eq!(h.call(x, b"name", &[]), h.symbol(b"x"));
    let printed = h.call(x, b"printString", &[]);
    assert_eq!(h.host.scope().string_bytes(printed).unwrap(), b"?x");
    let copy = h.call(x, b"clone", &[]);
    assert_ne!(copy, x);
    assert_eq!(h.call(copy, b"name", &[]), h.symbol(b"x"));
    let a = h.call(st, b"entity", &[]);
    let b = h.call(st, b"entity", &[]);
    let c = h.call(st, b"entity", &[]);
    assert_ne!(a, b);
    assert_ne!(a, c);
    let age = h.symbol(b"age");
    let city = h.symbol(b"city");
    let edge = h.symbol(b"edge");
    let many = h.symbol(b"many");
    h.assert(st, age, h.symbol(b"history"), h.named(b"true"));
    h.assert(st, a, age, h.i(20));
    h.assert(st, b, age, h.i(40));
    h.assert(st, c, age, h.i(20));
    h.assert(st, a, city, h.symbol(b"A"));
    h.assert(st, b, city, h.symbol(b"B"));
    h.assert(st, c, city, h.symbol(b"A"));
    assert_eq!(h.nums(h.query(st, y, &[&[x, age, y]])), vec![20, 40]);
    assert_eq!(
        h.elements(h.query(st, x, &[&[x, age, y], &[y, h.symbol(b">"), h.i(20)]])),
        vec![b]
    );
    assert_eq!(
        h.elements(h.query(st, x, &[&[x, age, y], &[x, city, h.symbol(b"A")]])),
        vec![a, c]
    );
    let neg = h.vector(&[h.symbol(b"not"), h.vector(&[x, city, h.symbol(b"A")])]);
    let base = h.vector(&[x, age, y]);
    assert_eq!(
        h.elements(h.call(st, b"findAll:where:", &[x, h.vector(&[base, neg])])),
        vec![b]
    );
    let or = h.vector(&[
        h.symbol(b"or"),
        h.vector(&[x, age, h.i(20)]),
        h.vector(&[x, age, h.i(40)]),
    ]);
    assert_eq!(
        h.elements(h.call(st, b"findAll:where:", &[x, h.vector(&[or])])),
        vec![a, c, b]
    );
    let body = h.vector(&[base]);
    for (selector, expected) in [
        (b"count:where:".as_slice(), 2),
        (b"sum:where:", 60),
        (b"min:where:", 20),
        (b"max:where:", 40),
    ] {
        assert_eq!(h.number(h.call(st, selector, &[y, body])), expected);
    }
    let average = h.call(st, b"avg:where:", &[y, body]);
    assert_eq!(h.number(average), 30);
    let empty = h.vector(&[h.vector(&[x, h.symbol(b"absent"), y])]);
    for selector in [b"count:where:".as_slice(), b"sum:where:"] {
        assert_eq!(h.number(h.call(st, selector, &[y, empty])), 0);
    }
    for selector in [b"min:where:".as_slice(), b"max:where:", b"avg:where:"] {
        assert_eq!(h.call(st, selector, &[y, empty]), h.named(b"nil"));
    }
    let groups = h.call(
        st,
        b"groupBy:aggregate:of:where:",
        &[
            z,
            h.symbol(b"sum"),
            y,
            h.vector(&[base, h.vector(&[x, city, z])]),
        ],
    );
    assert_eq!(h.number(h.call(groups, b"at:", &[h.symbol(b"A")])), 20);
    assert_eq!(h.number(h.call(groups, b"at:", &[h.symbol(b"B")])), 40);
    let before = h.call(st, b"basis", &[]);
    h.assert(st, a, age, h.i(30));
    let after = h.call(st, b"basis", &[]);
    assert_eq!(
        h.nums(h.call(st, b"findAll:where:asOf:", &[y, body, before])),
        vec![20, 40]
    );
    assert_eq!(
        h.nums(h.call(st, b"findAll:where:since:", &[y, body, before])),
        vec![30]
    );
    assert_eq!(
        h.nums(h.call(st, b"findAll:where:between:and:", &[y, body, before, after])),
        vec![30]
    );
    assert_eq!(
        h.call(st, b"findAll:where:asOf:", &[y, body, h.i(-1)]),
        h.named(b"nil")
    );
    assert_eq!(
        h.call(st, b"findAll:where:between:and:", &[y, body, after, before]),
        h.named(b"nil")
    );
    assert_eq!(
        h.call(st, b"findAll:where:", &[h.i(1), body]),
        h.named(b"nil")
    );
    assert_eq!(
        h.call(st, b"findAll:where:", &[y, h.vector(&[h.vector(&[x])])]),
        h.named(b"nil")
    );
    // Rule definitions remain ordinary Store facts, rebuilt from each snapshot.
    h.assert(st, edge, h.symbol(b"cardinality"), many);
    h.assert(st, a, edge, b);
    h.assert(st, b, edge, c);
    let rule = h.symbol(b"reachable");
    let tag = h.symbol(b"rule");
    h.call(
        st,
        b"defineRule:params:body:",
        &[
            rule,
            h.vector(&[x, y]),
            h.vector(&[h.vector(&[x, edge, y])]),
        ],
    );
    h.call(
        st,
        b"defineRule:params:body:",
        &[
            rule,
            h.vector(&[x, y]),
            h.vector(&[h.vector(&[x, edge, z]), h.vector(&[tag, rule, z, y])]),
        ],
    );
    assert_eq!(
        h.elements(h.query(st, y, &[&[tag, rule, a, y]])),
        vec![b, c]
    );
    assert_eq!(h.call(st, b"rebuildRules", &[]), st);
    h.call(st, b"retract:attr:value:", &[b, edge, c]);
    assert_eq!(h.elements(h.query(st, y, &[&[tag, rule, a, y]])), vec![b]);
    h.assert(st, b, edge, c);
    let pulled = h.call(
        st,
        b"pull:spec:",
        &[a, h.vector(&[age, h.vector(&[edge, h.vector(&[age])])])],
    );
    assert_eq!(h.number(h.call(pulled, b"at:", &[age])), 30);
    let children = h.call(pulled, b"at:", &[edge]);
    let child = h.call(children, b"at:", &[h.i(0)]);
    assert_eq!(h.number(h.call(child, b"at:", &[age])), 40);
    // Source spelling and selectors are unchanged.
    assert_eq!(h.number(h.eval(b"let s := [Store new]; let e := [s entity]; [s assert: e attr: #score value: 7]; [s sum: ?v where: #[ #[?e #score ?v] ]]")),7);
    // Equal scalar values still deduplicate, and mixed aggregates promote.
    let mixed = h.call(h.named(b"Store"), b"new", &[]);
    let val = h.symbol(b"scalar");
    let f = h.call(
        h.named(b"Float"),
        b"fromBits:",
        &[h.word(2.5f64.to_bits() as usize)],
    );
    h.assert(mixed, a, val, h.i(2));
    h.assert(mixed, b, val, f);
    let mb = h.vector(&[h.vector(&[x, val, y])]);
    let total = h.call(mixed, b"sum:where:", &[y, mb]);
    assert_eq!(
        h.host
            .scope()
            .word_bits(h.call(h.named(b"Float"), b"bitsOf:", &[total]))
            .unwrap(),
        4.5f64.to_bits() as usize
    );
    let t1 = h.text(b"same");
    let t2 = h.text(b"same");
    h.assert(mixed, a, val, t1);
    h.assert(mixed, b, val, t2);
    let strings = h.query(mixed, y, &[&[x, val, y]]);
    assert_eq!(h.metric(strings, b"size"), 1);
    assert_eq!(h.call(mixed, b"sum:where:", &[y, mb]), h.named(b"nil"));
    h.assert(mixed, a, val, h.i(i64::MAX));
    h.assert(mixed, b, val, h.i(1));
    assert_eq!(h.call(mixed, b"sum:where:", &[y, mb]), h.named(b"nil"));
    live_and_gc(&h, st, x, y, age, body);
    h.clear();
}
fn live_and_gc(h: &H<'_>, st: Oop, x: Oop, y: Oop, attr: Oop, body: Oop) {
    let nested = h.vector(&[h.vector(&[x, attr, y])]);
    for (vt, selector, alias) in [
        (
            h.named(b"StoreVt"),
            b"liveTriplesDo:".as_slice(),
            b"savedRelTriples".as_slice(),
        ),
        (h.named(b"IntegerVt"), b"wordOf:", b"savedRelWord"),
        (h.named(b"VectorVt"), b"add:", b"savedRelAdd"),
        (h.named(b"MapVt"), b"at:put:", b"savedRelPut"),
    ] {
        let old = h.save(vt, selector, alias);
        let c = h.context(&[
            h.gc,
            h.symbol(alias),
            size(0).unwrap(),
            size(1).unwrap(),
            st,
            y,
            nested,
            h.named(b"Integer"),
        ]);
        h.bind(vt, selector, h.method(b"forward", c));
        let result = h.call(st, b"findAll:where:", &[y, body]);
        assert_eq!(h.nums(result), vec![30, 40, 20]);
        let pull = h.call(
            st,
            b"pull:spec:",
            &[
                h.elements(h.query(st, x, &[&[x, attr, y]]))[0],
                h.vector(&[attr]),
            ],
        );
        assert_ne!(pull, h.named(b"nil"));
        assert!(h.count(c, 2) > 0);
        h.restore(vt, selector, alias, old);
    }
    let vt = h.named(b"StoreVt");
    let old = h.save(vt, b"liveTriplesDo:", b"savedRelTriples");
    let fault = h.hooks.fault(917);
    let denied = h.context(&[fault, size(0).unwrap()]);
    let failure = h.hooks.method(b"deny", denied);
    h.bind(vt, b"liveTriplesDo:", failure);
    assert_eq!(
        unsafe { h.host.id.send(st, h.symbol(b"findAll:where:"), &[y, body]) }.unwrap_err(),
        fault
    );
    let emission = h.context(&[
        h.gc,
        h.named(b"EntityVt"),
        h.host.roots,
        h.named(b"Integer"),
        attr,
        ptr::null_mut(),
        size(0).unwrap(),
        h.named(b"nil"),
    ]);
    h.bind(vt, b"liveTriplesDo:", h.method(b"emit", emission));
    let result = h.query(st, x, &[&[x, attr, y]]);
    let values = h.elements(result);
    assert_eq!(values.len(), 1);
    assert_eq!(
        h.call(h.host.id.root, b"identityOf:", &[values[0]]),
        h.field(emission, 6)
    );
    let escaped = h.field(emission, 5);
    assert!(
        h.host
            .send(escaped, b"value:value:value:", &[x, attr, y])
            .is_err()
    );
    let vector_vt = h.named(b"VectorVt");
    let add = h.save(vector_vt, b"add:", b"savedFailedRelAdd");
    h.bind(vector_vt, b"add:", failure);
    assert_eq!(
        unsafe { h.host.id.send(st, h.symbol(b"findAll:where:"), &[x, body]) }.unwrap_err(),
        fault
    );
    let abandoned = h.field(emission, 6);
    h.restore(vector_vt, b"add:", b"savedFailedRelAdd", add);
    h.collect();
    assert_eq!(as_size(h.call(h.probe, b"present", &[abandoned])), Some(0));
    h.restore(vt, b"liveTriplesDo:", b"savedRelTriples", old);
    let factory = h.named(b"Vector");
    let replacement = h.call(h.named(b"Object"), b"clone", &[]);
    let fvt = h.call(h.host.id.root, b"ensurePrivateVtableFor:", &[replacement]);
    let c = h.context(&[h.gc, factory, size(0).unwrap()]);
    h.bind(fvt, b"new", h.method(b"factory", c));
    let key = h.symbol(b"Vector");
    h.call(h.host.namespace, b"at:put:", &[key, replacement]);
    let result = h.call(st, b"findAll:where:", &[y, body]);
    assert_eq!(h.nums(result), vec![30, 40, 20]);
    assert!(h.count(c, 2) > 0);
    h.call(h.host.namespace, b"at:put:", &[key, factory]);
    let name = h.symbol(b"retainedLogicName");
    let variable = h.call(h.named(b"LogicVar"), b"named:", &[name]);
    h.collect();
    assert_eq!(h.call(h.named(b"LogicVar"), b"named:", &[name]), variable);
    let q = h.host.scope();
    assert_eq!(
        h.host
            .load_native(
                &q,
                &h.host
                    .resources
                    .lib_dir
                    .join("native/libbirnam_relational.dylib"),
                &[h.host.namespace]
            )
            .unwrap(),
        h.named(b"Store")
    );
}
