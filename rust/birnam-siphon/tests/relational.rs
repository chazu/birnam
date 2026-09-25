//! Relational public API over the real production message prefix.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
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
#[test]
#[ignore = "requires production dylibs (mise run native-messages)"]
fn relational_preserves_queries_rules_history_and_live_messages() {
    unsafe {
        let h = H::new();
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
        let raw = send(h.id, printed, b"bytes", &[]).as_bytes().unwrap();
        assert_eq!(
            birnam_abi::borrowed_bytes(raw.pointer, raw.length).unwrap(),
            b"?x"
        );
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
        let f = h.keep(object(send(
            h.id,
            h.named(b"Float"),
            b"fromBits:",
            &[Operand::word(2.5f64.to_bits() as usize)],
        )));
        h.assert(mixed, a, val, h.i(2));
        h.assert(mixed, b, val, f);
        let mb = h.vector(&[h.vector(&[x, val, y])]);
        let total = h.call(mixed, b"sum:where:", &[y, mb]);
        assert_eq!(
            send(
                h.id,
                h.named(b"Float"),
                b"bitsOf:",
                &[Operand::object(total)]
            )
            .as_word(),
            Some(4.5f64.to_bits() as usize)
        );
        let t1 = h.keep(object(send(
            h.id,
            h.named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(b"same")],
        )));
        let t2 = h.keep(object(send(
            h.id,
            h.named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(b"same")],
        )));
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
}
struct Observe {
    h: *const H,
    alias: Oop,
    calls: Cell<usize>,
    nested: Cell<bool>,
    st: Oop,
    x: Oop,
    y: Oop,
    attr: Oop,
}
unsafe extern "C" fn collecting_forward(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    unsafe {
        let o = &*c.cast::<Observe>();
        let h = &*o.h;
        o.calls.set(o.calls.get() + 1);
        collect(h.id, h.gc);
        if o.nested.replace(false) {
            let result = h.query(o.st, o.y, &[&[o.x, o.attr, o.y]]);
            assert!(!h.elements(result).is_empty());
        }
        let args = borrowed_slice(a, n).unwrap();
        h.id.send(r, o.alias, args)
            .map_or_else(|e| e, Outcome::loaded)
    }
}
unsafe extern "C" fn failure(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 917)
}
struct Emission {
    h: *const H,
    attr: Oop,
    entity: Cell<Oop>,
    callback: Cell<Oop>,
}
unsafe extern "C" fn emit_ephemeral(
    c: *mut c_void,
    _: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    unsafe {
        let e = &*c.cast::<Emission>();
        let h = &*e.h;
        let args = borrowed_slice(a, n).unwrap();
        let callback = object(args[0]);
        e.callback.set(h.keep(callback));
        let entity = allocate(h.id, h.named(b"EntityVt"), 0);
        e.entity.set(entity);
        let token = retain(h.id, h.roots, entity);
        let v = h.i(13);
        send(
            h.id,
            callback,
            b"value:value:value:",
            &[
                Operand::object(entity),
                Operand::object(e.attr),
                Operand::object(v),
            ],
        );
        release(h.id, h.roots, token);
        collect(h.id, h.gc);
        assert!(present(h.id, entity));
        Outcome::loaded(Operand::object(h.named(b"nil")))
    }
}
unsafe fn save(h: &H, vt: Oop, selector: &[u8], alias: &[u8]) -> NativeEntry {
    unsafe {
        let selected = send(
            h.id,
            h.id.receiver,
            b"bindingOf:in:",
            &[Operand::object(h.symbol(selector)), Operand::object(vt)],
        )
        .as_entry()
        .unwrap();
        send(
            h.id,
            vt,
            b"addMethod",
            &[Operand::object(h.symbol(alias)), Operand::entry(selected)],
        );
        selected
    }
}
unsafe fn restore(h: &H, vt: Oop, selector: &[u8], alias: &[u8], selected: NativeEntry) {
    unsafe {
        send(
            h.id,
            vt,
            b"addMethod",
            &[
                Operand::object(h.symbol(selector)),
                Operand::entry(selected),
            ],
        );
        send(
            h.id,
            vt,
            b"removeMethod:",
            &[Operand::object(h.symbol(alias))],
        );
    }
}
unsafe fn live_and_gc(h: &H, st: Oop, x: Oop, y: Oop, attr: Oop, body: Oop) {
    unsafe {
        // Each query consults the current Store, conversion and output methods.
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
            let selected = save(h, vt, selector, alias);
            let mut observer = Observe {
                h,
                alias: h.symbol(alias),
                calls: Cell::new(0),
                nested: Cell::new(true),
                st,
                x,
                y,
                attr,
            };
            install(
                h.id,
                vt,
                selector,
                collecting_forward,
                (&mut observer as *mut Observe).cast(),
            );
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
            assert!(observer.calls.get() > 0);
            restore(h, vt, selector, alias, selected);
        }
        let vt = h.named(b"StoreVt");
        let original = save(h, vt, b"liveTriplesDo:", b"savedRelTriples");
        install(h.id, vt, b"liveTriplesDo:", failure, core::ptr::null_mut());
        let failed =
            h.id.send_named(
                st,
                b"findAll:where:",
                &[Operand::object(y), Operand::object(body)],
            )
            .unwrap_err();
        assert_eq!(failed.detail, 917);
        let mut emission = Emission {
            h,
            attr,
            entity: Cell::new(core::ptr::null_mut()),
            callback: Cell::new(core::ptr::null_mut()),
        };
        install(
            h.id,
            vt,
            b"liveTriplesDo:",
            emit_ephemeral,
            (&mut emission as *mut Emission).cast(),
        );
        let result = h.query(st, x, &[&[x, attr, y]]);
        assert_eq!(h.elements(result), vec![emission.entity.get()]);
        // Escaped collectors have no native stack pointer and reject later delivery.
        assert!(
            h.id.send_named(
                emission.callback.get(),
                b"value:value:value:",
                &[
                    Operand::object(x),
                    Operand::object(attr),
                    Operand::object(y)
                ]
            )
            .is_err()
        );
        let vector_vt = h.named(b"VectorVt");
        let add = save(h, vector_vt, b"add:", b"savedFailedRelAdd");
        install(h.id, vector_vt, b"add:", failure, core::ptr::null_mut());
        let failed =
            h.id.send_named(
                st,
                b"findAll:where:",
                &[Operand::object(x), Operand::object(body)],
            )
            .unwrap_err();
        assert_eq!(failed.detail, 917);
        let abandoned = emission.entity.get();
        restore(h, vector_vt, b"add:", b"savedFailedRelAdd", add);
        collect(h.id, h.gc);
        assert!(!present(h.id, abandoned));
        restore(h, vt, b"liveTriplesDo:", b"savedRelTriples", original);
        let factory = h.named(b"Vector");
        let factory_vt = h.keep(behavior(h.id));
        let mut selected = Factory {
            h,
            original: factory,
            calls: Cell::new(0),
        };
        install(
            h.id,
            factory_vt,
            b"new",
            factory_new,
            (&mut selected as *mut Factory).cast(),
        );
        let replacement = h.keep(allocate(h.id, factory_vt, 0));
        let key = h.symbol(b"Vector");
        send(
            h.id,
            h.ns,
            b"at:put:",
            &[Operand::object(key), Operand::object(replacement)],
        );
        let result = h.call(st, b"findAll:where:", &[y, body]);
        assert_eq!(h.nums(result), vec![30, 40, 20]);
        assert!(selected.calls.get() > 0);
        send(
            h.id,
            h.ns,
            b"at:put:",
            &[Operand::object(key), Operand::object(factory)],
        );
        // Collection does not lose interned LogicVars or change repeated loading.
        let name = h.symbol(b"retainedLogicName");
        let variable = h.call(h.named(b"LogicVar"), b"named:", &[name]);
        collect(h.id, h.gc);
        assert_eq!(h.call(h.named(b"LogicVar"), b"named:", &[name]), variable);
        assert_eq!(
            load(h.id, "relational", &[Operand::object(h.ns)]),
            h.named(b"Store")
        );
    }
}

struct Factory {
    h: *const H,
    original: Oop,
    calls: Cell<usize>,
}
unsafe extern "C" fn factory_new(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    unsafe {
        assert_eq!(n, 0);
        let f = &*c.cast::<Factory>();
        let h = &*f.h;
        f.calls.set(f.calls.get() + 1);
        collect(h.id, h.gc);
        h.id.send_named(f.original, b"new", &[])
            .map_or_else(|e| e, Outcome::loaded)
    }
}
