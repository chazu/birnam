//! Production Store, real source consumers, and callbacks that erase their own
//! original roots. The snapshot oracle in birnam-store remains independent.
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
    unsafe fn fresh(&self) -> Oop {
        unsafe { self.call(self.named(b"Object"), b"clone", &[]) }
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
    unsafe fn callback(&self, selector: &[u8], method: native::Method, c: *mut c_void) -> Oop {
        unsafe {
            let vt = self.keep(behavior(self.id));
            install(self.id, vt, selector, method, c);
            self.keep(allocate(self.id, vt, 0))
        }
    }
    unsafe fn assert(&self, st: Oop, e: Oop, a: Oop, v: Oop) {
        assert_eq!(
            unsafe { self.call(st, b"assert:attr:value:", &[e, a, v]) },
            st
        );
    }
    unsafe fn read(&self, st: Oop, e: Oop, a: Oop) -> Oop {
        unsafe { self.call(st, b"currentValue:attr:", &[e, a]) }
    }
}

struct Visitor {
    h: *const H,
    store: Oop,
    calls: Cell<usize>,
    seen: RefCell<Vec<[Oop; 3]>>,
    fail: bool,
}
unsafe extern "C" fn snapshot_visitor(
    c: *mut c_void,
    _: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    unsafe {
        let c = &*c.cast::<Visitor>();
        let h = &*c.h;
        let a = borrowed_slice(a, n).unwrap();
        assert_eq!(a.len(), 3);
        if c.calls.get() == 0 {
            send(h.id, c.store, b"imageLoadBegin", &[]);
        }
        collect(h.id, h.gc);
        let row = [object(a[0]), object(a[1]), object(a[2])];
        for o in row {
            assert!(present(h.id, o));
        }
        c.seen.borrow_mut().push(row);
        c.calls.set(c.calls.get() + 1);
        if c.fail {
            Outcome::failed(native::INVALID_REQUEST, 71)
        } else {
            Outcome::loaded(Operand::object(c.store))
        }
    }
}
struct Replacement {
    h: *const H,
    calls: Cell<usize>,
    entry: NativeEntry,
}
struct ConstructionProbe {
    h: *const H,
    entry: NativeEntry,
    inside: Cell<bool>,
    calls: Cell<usize>,
}
unsafe extern "C" fn collecting_allocation_query(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    unsafe {
        let c = &*c.cast::<ConstructionProbe>();
        if !c.inside.replace(true) {
            let h = &*c.h;
            c.calls.set(c.calls.get() + 1);
            collect(h.id, h.gc);
            c.inside.set(false);
        }
        c.entry.method.unwrap()(c.entry.context, r, a, n)
    }
}
unsafe fn collecting_construction(h: &H) {
    unsafe {
        // Deliberately remove publication, then construct another complete
        // instance. The old instance remains registered with its own state.
        let store = h.symbol(b"Store");
        send(
            h.id,
            h.ns,
            b"at:put:",
            &[Operand::object(store), Operand::unit()],
        );
        let query = h.symbol(b"allocationSizeOf:");
        let entry = send(
            h.id,
            h.id.receiver,
            b"bindingOf:in:",
            &[Operand::object(query), Operand::object(h.id.receiver)],
        )
        .as_entry()
        .unwrap();
        let mut c = ConstructionProbe {
            h,
            entry,
            inside: Cell::new(false),
            calls: Cell::new(0),
        };
        install(
            h.id,
            h.id.receiver,
            b"allocationSizeOf:",
            collecting_allocation_query,
            (&mut c as *mut ConstructionProbe).cast(),
        );
        let new = load(h.id, "store", &[Operand::object(h.ns)]);
        send(
            h.id,
            h.id.receiver,
            b"addMethod",
            &[Operand::object(query), Operand::entry(entry)],
        );
        assert!(c.calls.get() >= 2);
        assert_eq!(new, h.named(b"Store"));
        assert_eq!(h.metric(new, b"basis"), 3);
    }
}
unsafe extern "C" fn replacement(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let c = &*c.cast::<Replacement>();
        let h = &*c.h;
        c.calls.set(c.calls.get() + 1);
        collect(h.id, h.gc);
        c.entry.method.unwrap()(c.entry.context, r, a, n)
    }
}
struct ImageCopy {
    h: *const H,
    target: Oop,
    count: Cell<usize>,
}
unsafe extern "C" fn image_copy(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let c = &*c.cast::<ImageCopy>();
        let h = &*c.h;
        let a = borrowed_slice(a, n).unwrap();
        assert_eq!(a.len(), 6);
        collect(h.id, h.gc);
        c.count.set(c.count.get() + 1);
        h.id.send_named(c.target, b"imageAddFact:attr:value:tx:op:historied:", a)
            .map_or_else(|e| e, Outcome::loaded)
    }
}

unsafe fn semantics(h: &H) {
    unsafe {
        let st = h.call(h.named(b"Store"), b"new", &[]);
        let e = h.symbol(b"entity");
        let a = h.symbol(b"attribute");
        let v = h.i(41);
        let w = h.i(42);
        let nil = h.named(b"nil");
        assert_eq!(h.read(st, e, a), nil);
        assert_eq!(
            send(
                h.id,
                st,
                b"rawCurrentValue:attr:",
                &[Operand::object(e), Operand::object(a)]
            )
            .kind(),
            native::UNIT
        );
        h.assert(st, e, a, nil);
        assert_eq!(h.call(st, b"hasValue:attr:", &[e, a]), h.named(b"true"));
        assert_eq!(
            h.call(st, b"currentValue:attr:ifAbsent:", &[e, a, h.fresh()]),
            nil
        );
        let before = h.metric(st, b"basis");
        h.assert(st, e, a, nil);
        assert_eq!(h.metric(st, b"basis"), before);
        h.assert(st, e, a, v);
        h.assert(st, e, a, w);
        assert_eq!(h.read(st, e, a), w);
        assert_eq!(h.number(h.call(st, b"history:attr:", &[e, a])), 1);
        h.call(st, b"retract:attr:value:", &[e, a, w]);
        assert_eq!(h.read(st, e, a), nil);
        // Prospective history and exact as-of watermarks, retained across compaction.
        let history = h.symbol(b"history");
        h.assert(st, a, history, h.named(b"true"));
        h.assert(st, e, a, v);
        let basis = h.call(st, b"basis", &[]);
        h.call(st, b"retainSince:", &[basis]);
        h.assert(st, e, a, w);
        assert_eq!(h.call(st, b"valueOf:attr:asOf:", &[e, a, basis]), v);
        h.call(st, b"compact", &[]);
        assert_eq!(h.call(st, b"valueOf:attr:asOf:", &[e, a, basis]), v);
        h.call(st, b"retainCurrentOnly", &[]);
        h.call(st, b"compact", &[]);
        let error = h.call(st, b"valueOf:attr:asOf:", &[e, a, basis]);
        assert_eq!(h.call(error, b"kind", &[]), h.symbol(b"historyNotRetained"));
        let many = h.symbol(b"many");
        let cardinality = h.symbol(b"cardinality");
        let tags = h.symbol(b"tags");
        h.assert(st, tags, cardinality, many);
        h.assert(st, e, tags, v);
        h.assert(st, e, tags, w);
        h.assert(st, e, tags, w);
        assert_eq!(h.number(h.call(st, b"count:attr:", &[e, tags])), 2);
        h.call(st, b"retract:attr:value:", &[e, tags, v]);
        assert_eq!(h.number(h.call(st, b"count:attr:", &[e, tags])), 1);
        assert_eq!(h.call(st, b"cardinality:", &[tags]), many);
        // Typed arity and receiver checks, including unsupported empty clones.
        assert!(h.id.send_named(st, b"assert:attr:value:", &[]).is_err());
        assert!(
            h.id.send_named(st, b"retainLast:", &[Operand::word(4)])
                .is_err()
        );
        let clone = h.call(st, b"clone", &[]);
        assert!(h.id.send_named(clone, b"basis", &[]).is_err());
        assert_eq!(
            load(h.id, "store", &[Operand::object(h.ns)]),
            h.named(b"Store")
        );
    }
}

unsafe fn cells_and_dispatch(h: &H) {
    unsafe {
        let st = h.call(h.named(b"Store"), b"new", &[]);
        let e = h.fresh();
        let attr = h.symbol(b"cell-key");
        let value = h.fresh();
        let cell = h.call(st, b"cellFor:attr:", &[e, attr]);
        h.call(cell, b"set:", &[value]);
        h.clear();
        h.keep(cell);
        collect(h.id, h.gc);
        for o in [st, e, attr, value, cell] {
            assert!(present(h.id, o));
        }
        assert_eq!(h.call(cell, b"value", &[]), value);
        let vt = h.named(b"StoreVt");
        let key = h.symbol(b"currentValue:attr:");
        let entry = send(
            h.id,
            h.id.receiver,
            b"bindingOf:in:",
            &[Operand::object(key), Operand::object(vt)],
        )
        .as_entry()
        .unwrap();
        let mut c = Replacement {
            h,
            calls: Cell::new(0),
            entry,
        };
        install(
            h.id,
            vt,
            b"currentValue:attr:",
            replacement,
            (&mut c as *mut Replacement).cast(),
        );
        assert_eq!(h.call(cell, b"value", &[]), value);
        assert_eq!(c.calls.get(), 1);
        send(
            h.id,
            vt,
            b"addMethod",
            &[Operand::object(key), Operand::entry(entry)],
        );
        // Fact result construction must use the current Integer factory too.
        let integer = h.named(b"Integer");
        let ivt = object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(integer)],
        ));
        let key = h.symbol(b"fromWord:");
        let entry = send(
            h.id,
            h.id.receiver,
            b"bindingOf:in:",
            &[Operand::object(key), Operand::object(ivt)],
        )
        .as_entry()
        .unwrap();
        let mut factory = Replacement {
            h,
            calls: Cell::new(0),
            entry,
        };
        install(
            h.id,
            ivt,
            b"fromWord:",
            replacement,
            (&mut factory as *mut Replacement).cast(),
        );
        assert!(h.metric(st, b"basis") > 3);
        assert_eq!(factory.calls.get(), 1);
        send(
            h.id,
            ivt,
            b"addMethod",
            &[Operand::object(key), Operand::entry(entry)],
        );
        h.clear();
        collect(h.id, h.gc);
        for o in [st, e, attr, value, cell] {
            assert!(!present(h.id, o), "Store or Cell retained a dead edge");
        }
    }
}

unsafe fn snapshots(h: &H, fail: bool) {
    unsafe {
        let st = h.call(h.named(b"Store"), b"new", &[]);
        let e = h.fresh();
        let attr = h.symbol(b"snapshot-many");
        h.assert(st, attr, h.symbol(b"cardinality"), h.symbol(b"many"));
        let values = (0..8).map(|_| h.fresh()).collect::<Vec<_>>();
        for &v in &values {
            h.assert(st, e, attr, v);
        }
        let mut visitor = Visitor {
            h,
            store: st,
            calls: Cell::new(0),
            seen: RefCell::new(vec![]),
            fail,
        };
        let block = h.callback(
            b"value:value:value:",
            snapshot_visitor,
            (&mut visitor as *mut Visitor).cast(),
        );
        h.clear();
        h.keep(st);
        h.keep(block);
        let result =
            h.id.send_named(st, b"liveTriplesDo:", &[Operand::object(block)]);
        if fail {
            assert_eq!(result.unwrap_err().detail, 71);
        } else {
            assert!(result.is_ok());
            let seen = visitor.seen.borrow();
            for &v in &values {
                assert!(seen.contains(&[e, attr, v]));
            }
        }
        collect(h.id, h.gc);
        for v in values {
            assert!(
                !present(h.id, v),
                "snapshot leaked after callback completion/failure"
            );
        }
        h.clear();
        collect(h.id, h.gc);
    }
}

unsafe fn image_transfer(h: &H) {
    unsafe {
        let st = h.call(h.named(b"Store"), b"new", &[]);
        let target = h.call(h.named(b"Store"), b"new", &[]);
        let e = h.fresh();
        let a = h.symbol(b"durable");
        let v = h.fresh();
        h.assert(st, e, a, v);
        let transient = h.symbol(b"temporary");
        h.assert(st, transient, h.symbol(b"imageTransient"), h.named(b"true"));
        h.assert(st, e, transient, h.fresh());
        h.call(target, b"imageLoadBegin", &[]);
        let mut copy = ImageCopy {
            h,
            target,
            count: Cell::new(0),
        };
        let collector = h.callback(
            b"e:a:v:tx:op:historied:",
            image_copy,
            (&mut copy as *mut ImageCopy).cast(),
        );
        h.call(st, b"imageFactsInto:", &[collector]);
        assert!(copy.count.get() >= 4);
        let metadata = [
            h.call(st, b"basis", &[]),
            h.named(b"false"),
            h.call(st, b"oldestRetainedBasis", &[]),
            h.call(st, b"retentionPolicyCode", &[]),
            h.call(st, b"retentionPolicyArgument", &[]),
        ];
        h.call(
            target,
            b"imageLoadEnd:raw:oldest:policy:argument:",
            &metadata,
        );
        assert_eq!(h.read(target, e, a), v);
        assert_eq!(h.read(target, e, transient), h.named(b"nil"));
        assert_eq!(h.metric(st, b"basis"), h.metric(target, b"basis"));
        assert!(
            h.id.send_named(
                target,
                b"imageAddFact:attr:value:tx:op:historied:",
                &[
                    Operand::object(e),
                    Operand::object(a),
                    Operand::object(v),
                    Operand::object(h.i(-1)),
                    Operand::object(h.i(0)),
                    Operand::object(h.named(b"true"))
                ]
            )
            .is_err()
        );
    }
}

unsafe fn source(h: &H) {
    unsafe {
        let answer=h.eval(b"let storeSample := [Store new]; let storeCell := [storeSample cellFor: #sample attr: #value]; [storeCell set: 41]; [[storeCell value] + 1]");
        assert_eq!(h.number(answer), 42);
        assert_eq!(
            h.number(h.eval(b"[Globals at: #sourceGlobal put: 73]; [Globals at: #sourceGlobal]")),
            73
        );
        assert_eq!(
            h.number(h.eval(b"let declaredGlobal := 91; [Globals at: #declaredGlobal]")),
            91
        );
        assert_eq!(
            h.eval(b"[Globals at: #boundNil put: nil]; [Globals at: #boundNil ifAbsent: [| 99]]"),
            h.named(b"nil")
        );
        assert_eq!(
            h.number(h.eval(b"[Globals at: #missingStoreGlobal ifAbsent: [| 99]]")),
            99
        );
        assert_eq!(
            h.eval(b"[[db cellFor: #sample attr: #unset] value]"),
            h.named(b"nil")
        );
    }
}

unsafe fn globals_are_facts(h: &H) {
    unsafe {
        let globals = h.named(b"Globals");
        let store = h.named(b"Store");
        let entity = h.symbol(b"__globals__");
        let key = h.symbol(b"historied-global");
        h.assert(store, key, h.symbol(b"history"), h.named(b"true"));
        let a = h.i(11);
        let b = h.i(22);
        h.call(globals, b"at:put:", &[key, a]);
        let basis = h.call(store, b"basis", &[]);
        h.call(store, b"retainSince:", &[basis]);
        h.call(globals, b"at:put:", &[key, b]);
        assert_eq!(h.read(store, entity, key), b);
        assert_eq!(
            h.call(store, b"valueOf:attr:asOf:", &[entity, key, basis]),
            a
        );
        h.assert(store, entity, key, a);
        assert_eq!(h.call(globals, b"at:", &[key]), a);
        let missing = h.symbol(b"absent-global");
        assert_eq!(h.call(globals, b"includes:", &[missing]), h.named(b"false"));
        h.call(globals, b"at:put:", &[missing, h.named(b"nil")]);
        assert_eq!(h.call(globals, b"includes:", &[missing]), h.named(b"true"));
        // A replaced Store reader is observed by Globals, not bypassed via Namespace.
        let vt = h.named(b"StoreVt");
        let read = h.symbol(b"currentValue:attr:");
        let entry = send(
            h.id,
            h.id.receiver,
            b"bindingOf:in:",
            &[Operand::object(read), Operand::object(vt)],
        )
        .as_entry()
        .unwrap();
        let mut c = Replacement {
            h,
            calls: Cell::new(0),
            entry,
        };
        install(
            h.id,
            vt,
            b"currentValue:attr:",
            replacement,
            (&mut c as *mut Replacement).cast(),
        );
        assert_eq!(h.call(globals, b"at:", &[key]), a);
        assert_eq!(c.calls.get(), 1);
        send(
            h.id,
            vt,
            b"addMethod",
            &[Operand::object(read), Operand::entry(entry)],
        );
        assert_eq!(
            send(h.id, h.ns, b"at:", &[Operand::object(key)]).kind(),
            native::UNIT
        );
    }
}

struct MopProbe {
    h: *const H,
    store: Oop,
    allow: Cell<bool>,
    authorizations: Cell<usize>,
    notifications: Cell<usize>,
    expected: Cell<Oop>,
}
unsafe extern "C" fn authorize_mop(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    unsafe {
        assert_eq!(n, 0);
        let c = &*c.cast::<MopProbe>();
        let h = &*c.h;
        collect(h.id, h.gc);
        c.authorizations.set(c.authorizations.get() + 1);
        Outcome::loaded(Operand::object(h.named(if c.allow.get() {
            b"true"
        } else {
            b"false"
        })))
    }
}
unsafe extern "C" fn observe_mop(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let c = &*c.cast::<MopProbe>();
        let h = &*c.h;
        let a = borrowed_slice(a, n).unwrap();
        assert_eq!(n, 2);
        collect(h.id, h.gc);
        let row = send(h.id, c.store, b"rawCurrentValue:attr:", &[a[1], a[0]]);
        if c.expected.get().is_null() {
            assert_eq!(row.kind(), native::UNIT);
        } else {
            assert_eq!(row.as_object(), Some(c.expected.get()));
        }
        c.notifications.set(c.notifications.get() + 1);
        Outcome::loaded(Operand::unit())
    }
}
unsafe extern "C" fn method_answer(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    assert_eq!(n, 0);
    Outcome::loaded(Operand::object(c))
}
unsafe extern "C" fn ignore_dictionary_change(
    c: *mut c_void,
    _: Oop,
    _: *const Operand,
    n: usize,
) -> Outcome {
    assert_eq!(n, 2);
    let calls = unsafe { &*c.cast::<Cell<usize>>() };
    calls.set(calls.get() + 1);
    Outcome::loaded(Operand::unit())
}
unsafe fn mop_hooks(h: &H) {
    unsafe {
        let store = h.named(b"Store");
        let target = h.fresh();
        let vt = object(send(
            h.id,
            h.id.receiver,
            b"ensurePrivateVtableFor:",
            &[Operand::object(target)],
        ));
        h.keep(vt);
        let selector = h.symbol(b"mopProbe");
        let value = h.i(17);
        let method = h.keep(object(send(
            h.id,
            h.named(b"Method"),
            b"fromNative:owner:",
            &[
                Operand::entry(NativeEntry::new(method_answer, value)),
                Operand::object(value),
            ],
        )));
        let c = Box::leak(Box::new(MopProbe {
            h,
            store,
            allow: Cell::new(false),
            authorizations: Cell::new(0),
            notifications: Cell::new(0),
            expected: Cell::new(method),
        }));
        let context = (c as *mut MopProbe).cast();
        let authorizer = h.callback(b"value", authorize_mop, context);
        let observer = h.callback(b"managedMethodChanged:on:", observe_mop, context);
        h.call(store, b"setMethodMutationAuthorizer:", &[authorizer]);
        h.call(store, b"setMethodMutationObserver:", &[observer]);
        h.call(store, b"setMethodMutationAuthorizer:", &[authorizer]);
        h.call(store, b"setMethodMutationObserver:", &[observer]);
        let err = h.call(store, b"setMethodMutationObserver:", &[h.fresh()]);
        assert_eq!(
            h.call(err, b"kind", &[]),
            h.symbol(b"methodMutationObserverAlreadySet")
        );
        let err = h.call(store, b"setMethodMutationAuthorizer:", &[h.fresh()]);
        assert_eq!(
            h.call(err, b"kind", &[]),
            h.symbol(b"methodMutationAuthorizerAlreadySet")
        );
        h.clear();
        for o in [target, vt, selector, method] {
            h.keep(o);
        }
        collect(h.id, h.gc);
        assert!(present(h.id, authorizer));
        assert!(present(h.id, observer));
        assert_eq!(
            h.call(store, b"addMethod:on:with:", &[selector, vt, method]),
            h.named(b"false")
        );
        assert_eq!(
            send(
                h.id,
                h.id.receiver,
                b"bindingOf:in:",
                &[Operand::object(selector), Operand::object(vt)]
            )
            .kind(),
            native::UNIT
        );
        assert_eq!(c.notifications.get(), 0);
        c.allow.set(true);
        assert_eq!(
            h.call(store, b"addMethod:on:with:", &[selector, vt, method]),
            store
        );
        assert_eq!(h.call(target, b"mopProbe", &[]), value);
        assert_eq!(h.read(store, vt, selector), method);
        assert_eq!(c.notifications.get(), 1);
        // Derived method-mirror rows remain nondurable; global facts persist.
        let target_store = h.call(store, b"new", &[]);
        h.call(target_store, b"imageLoadBegin", &[]);
        let mut copy = ImageCopy {
            h,
            target: target_store,
            count: Cell::new(0),
        };
        let collector = h.callback(
            b"e:a:v:tx:op:historied:",
            image_copy,
            (&mut copy as *mut ImageCopy).cast(),
        );
        h.call(store, b"imageFactsInto:", &[collector]);
        assert_eq!(h.read(target_store, vt, selector), h.named(b"nil"));
        let global = h.symbol(b"historied-global");
        assert_eq!(
            h.number(h.read(target_store, h.symbol(b"__globals__"), global)),
            11
        );
        // Notification is a fresh send, including for native dictionary writes.
        let store_vt = h.named(b"StoreVt");
        let changed = h.symbol(b"methodChanged:on:");
        let original = send(
            h.id,
            h.id.receiver,
            b"bindingOf:in:",
            &[Operand::object(changed), Operand::object(store_vt)],
        )
        .as_entry()
        .unwrap();
        let mut calls = Cell::new(0usize);
        install(
            h.id,
            store_vt,
            b"methodChanged:on:",
            ignore_dictionary_change,
            (&mut calls as *mut Cell<usize>).cast(),
        );
        install(h.id, vt, b"mopProbe", method_answer, value);
        assert!(calls.get() >= 2);
        assert_eq!(h.read(store, vt, selector), method);
        send(
            h.id,
            store_vt,
            b"addMethod",
            &[Operand::object(changed), Operand::entry(original)],
        );
        // Raw writes invalidate the mirror, without pretending to be managed writes.
        install(h.id, vt, b"mopProbe", method_answer, value);
        assert_eq!(h.read(store, vt, selector), h.named(b"nil"));
        assert_eq!(c.notifications.get(), 1);
        h.call(store, b"addMethod:on:with:", &[selector, vt, method]);
        c.expected.set(core::ptr::null_mut());
        h.call(store, b"removeMethod:on:", &[selector, vt]);
        assert_eq!(h.read(store, vt, selector), h.named(b"nil"));
        assert_eq!(c.notifications.get(), 3);
        let err = h.call(store, b"removeMethod:on:", &[selector, vt]);
        assert_eq!(h.call(err, b"kind", &[]), h.symbol(b"missingDirectMethod"));
        install(h.id, vt, b"mopProbe", method_answer, value);
        let before = h.metric(store, b"basis");
        h.call(store, b"backfillMirror", &[]);
        assert_eq!(h.metric(store, b"basis"), before);
        assert_eq!(h.read(store, vt, selector), h.symbol(b"primitive"));
        send(h.id, vt, b"removeMethod:", &[Operand::object(selector)]);
        assert_eq!(h.read(store, vt, selector), h.named(b"nil"));
        // The alternate Id dictionary messages notify through the same send.
        c.expected.set(method);
        h.call(store, b"addMethod:on:with:", &[selector, vt, method]);
        send(
            h.id,
            h.id.receiver,
            b"addMethod:to:as:",
            &[
                Operand::object(selector),
                Operand::word(vt.addr()),
                Operand::entry(NativeEntry::new(method_answer, value)),
            ],
        );
        assert_eq!(h.read(store, vt, selector), h.named(b"nil"));
        h.call(store, b"backfillMirror", &[]);
        send(
            h.id,
            h.id.receiver,
            b"removeMethod:from:",
            &[Operand::object(selector), Operand::word(vt.addr())],
        );
        assert_eq!(h.read(store, vt, selector), h.named(b"nil"));
    }
}

unsafe fn resource_limit(h: &H) {
    unsafe {
        let st = h.call(h.named(b"Store"), b"new", &[]);
        h.call(st, b"retainSince:", &[h.i(0)]);
        let e = h.symbol(b"limited-entity");
        let a = h.symbol(b"limited-history");
        let mut rejected = false;
        for n in 0..20 {
            let before = h.metric(st, b"basis");
            let value = h.i(n);
            let result = h.call(st, b"rawAssert:attr:value:", &[e, a, value]);
            if result != st {
                assert_eq!(
                    h.call(result, b"kind", &[]),
                    h.symbol(b"historyResourceLimit")
                );
                assert_eq!(h.metric(st, b"basis"), before);
                rejected = true;
                break;
            }
        }
        assert!(rejected);
        assert!(h.metric(st, b"retainedFactBytes") <= h.metric(st, b"factBytesLimit"));
        h.call(st, b"retainCurrentOnly", &[]);
        h.call(st, b"compact", &[]);
        h.assert(st, e, a, h.i(99));
        assert_eq!(h.number(h.read(st, e, a)), 99);
    }
}

struct PhaseProbe {
    h: *const H,
    store: Oop,
    args: [Oop; 3],
    rejected: Cell<bool>,
}
unsafe extern "C" fn tracing_probe(c: *mut c_void, _: Oop, _: *const Operand, n: usize) -> Outcome {
    unsafe {
        assert_eq!(n, 2);
        let c = &*c.cast::<PhaseProbe>();
        let h = &*c.h;
        c.rejected.set(
            h.id.send_named(c.store, b"assert:attr:value:", &c.args.map(Operand::object))
                .is_err(),
        );
        Outcome::loaded(Operand::unit())
    }
}
unsafe fn phase_safety(h: &H) {
    unsafe {
        let st = h.call(h.named(b"Store"), b"new", &[]);
        let mut probe = PhaseProbe {
            h,
            store: st,
            args: [h.fresh(), h.symbol(b"during-mark"), h.fresh()],
            rejected: Cell::new(false),
        };
        let owner = h.callback(
            b"referencesOf:do:",
            tracing_probe,
            (&mut probe as *mut PhaseProbe).cast(),
        );
        let vt = h.keep(behavior(h.id));
        h.keep(allocate(h.id, vt, 0));
        send(
            h.id,
            h.gc,
            b"trace:with:",
            &[Operand::object(vt), Operand::object(owner)],
        );
        let before = h.metric(st, b"basis");
        collect(h.id, h.gc);
        assert!(probe.rejected.get());
        assert_eq!(h.metric(st, b"basis"), before);
        send(
            h.id,
            h.gc,
            b"trace:with:",
            &[Operand::object(vt), Operand::unit()],
        );
    }
}

#[test]
#[ignore = "requires production module directory"]
fn store_history_cells_snapshots_and_source_use_id_messages() {
    unsafe {
        let h = H::new();
        if std::env::var_os("BIRNAM_STORE_TEST_LIMIT").is_some() {
            resource_limit(&h);
            return;
        }
        collecting_construction(&h);
        semantics(&h);
        history_plateaus(&h);
        h.clear();
        collect(h.id, h.gc);
        cells_and_dispatch(&h);
        snapshots(&h, false);
        snapshots(&h, true);
        image_transfer(&h);
        phase_safety(&h);
        source(&h);
        globals_are_facts(&h);
        mop_hooks(&h);
        h.clear();
        collect(h.id, h.gc);
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "store_history_cells_snapshots_and_source_use_id_messages",
                "--include-ignored",
            ])
            .env("BIRNAM_STORE_TEST_LIMIT", "1")
            .env("BIRNAM_STORE_FACT_BYTES_LIMIT", "256")
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
    }
}

unsafe fn history_plateaus(h: &H) {
    unsafe {
        for policy in [
            b"retainCurrentOnly".as_slice(),
            b"retainSince:",
            b"retainLast:",
        ] {
            let store = h.call(h.named(b"Store"), b"new", &[]);
            let entity = h.fresh();
            let attribute = h.symbol(b"churnHistory");
            h.assert(store, attribute, h.symbol(b"history"), h.named(b"true"));
            let mut baseline = None;
            for batch in 0..3 {
                match policy {
                    b"retainSince:" => {
                        let basis = h.call(store, b"basis", &[]);
                        h.call(store, policy, &[basis]);
                    }
                    b"retainLast:" => {
                        h.call(store, policy, &[h.i(128)]);
                    }
                    _ => {
                        h.call(store, policy, &[]);
                    }
                }
                let mark = h.tokens.borrow().len();
                for value in 0..5000 {
                    h.assert(store, entity, attribute, h.i(value));
                    let temporary = h.tokens.borrow_mut().split_off(mark);
                    for token in temporary {
                        release(h.id, h.roots, token);
                    }
                }
                h.call(store, b"compact", &[]);
                collect(h.id, h.gc);
                assert_eq!(h.number(h.read(store, entity, attribute)), 4999);
                let row = (
                    h.metric(store, b"retainedFactCount"),
                    h.metric(store, b"retainedFactBytes"),
                );
                assert!(row.0 > 0 && row.1 > 0);
                if batch > 0 {
                    if let Some(expected) = baseline {
                        assert_eq!(row, expected, "retained history grew for {:?}", policy);
                    } else {
                        baseline = Some(row);
                    }
                }
            }
        }
        // A retained-history control must grow, so a counter stuck at zero or
        // an implementation that always discards history cannot pass the oracle.
        let store = h.call(h.named(b"Store"), b"new", &[]);
        let entity = h.fresh();
        let attribute = h.symbol(b"retainedControl");
        h.assert(store, attribute, h.symbol(b"history"), h.named(b"true"));
        h.call(store, b"retainSince:", &[h.i(0)]);
        let mut previous = (0, 0);
        let mark = h.tokens.borrow().len();
        for batch in 0..3 {
            for value in 0..1000 {
                h.assert(store, entity, attribute, h.i(value));
                let temporary = h.tokens.borrow_mut().split_off(mark);
                for token in temporary {
                    release(h.id, h.roots, token);
                }
            }
            h.call(store, b"compact", &[]);
            let current = (
                h.metric(store, b"retainedFactCount"),
                h.metric(store, b"retainedFactBytes"),
            );
            assert!(
                current.0 > previous.0 && current.1 > previous.1,
                "retained control batch {batch}"
            );
            previous = current;
        }
        h.call(store, b"retainCurrentOnly", &[]);
        h.call(store, b"compact", &[]);
        assert_eq!(h.number(h.read(store, entity, attribute)), 999);
        assert!(h.metric(store, b"retainedFactCount") < previous.0);
    }
}
