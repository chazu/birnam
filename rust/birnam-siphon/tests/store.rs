//! Explicit Store facts, history, snapshots and source through production Oops.
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
        host.load_native(
            &q,
            &host.resources.lib_dir.join("native/libbirnam_store.dylib"),
            &[host.namespace],
        )
        .unwrap();
        let path = std::env::temp_dir().join(format!("birnam-store-{}.dylib", std::process::id()));
        assert!(
            std::process::Command::new("clang")
                .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
                .arg(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../tests/fixtures/oop-host/store.c")
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
        for n in [
            b"watch".as_slice(),
            b"allocation:bytes:identity:",
            b"mask",
            b"seen",
        ] {
            h.bind(vt, n, h.method(n, probe));
        }
        h
    }
    fn named(&self, n: &[u8]) -> Oop {
        self.host.named(n).unwrap()
    }
    fn symbol(&self, n: &[u8]) -> Oop {
        self.keep(self.host.intern(n).unwrap())
    }
    fn keep(&self, o: Oop) -> Oop {
        self.tokens
            .borrow_mut()
            .push(unsafe { self.host.id.send(self.host.roots, self.retain, &[o]) }.unwrap());
        o
    }
    fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        self.keep(self.host.send(r, n, a).unwrap())
    }
    fn i(&self, n: i64) -> Oop {
        let q = self.host.scope();
        self.call(
            self.named(b"Integer"),
            b"fromWord:",
            &[q.word(n as usize).unwrap()],
        )
    }
    fn number(&self, o: Oop) -> i64 {
        self.host
            .scope()
            .word_bits(self.call(self.named(b"Integer"), b"wordOf:", &[o]))
            .unwrap() as i64
    }
    fn metric(&self, r: Oop, n: &[u8]) -> i64 {
        self.number(self.call(r, n, &[]))
    }
    fn fresh(&self) -> Oop {
        self.call(self.named(b"Object"), b"clone", &[])
    }
    fn clear_to(&self, mark: usize) {
        for t in self.tokens.borrow_mut().split_off(mark).into_iter().rev() {
            unsafe { self.host.id.send(self.host.roots, self.release, &[t]) }.unwrap();
        }
    }
    fn clear(&self) {
        self.clear_to(0)
    }
    fn collect(&self) {
        self.call(self.gc, b"collectPrecisely", &[]);
    }
    fn eval(&self, s: &[u8]) -> Oop {
        let q = self.host.scope();
        let text = q.string(s).unwrap();
        let result = self.call(self.named(b"Codegen"), b"eval:", &[text]);
        q.check_result(result).unwrap();
        result
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
    fn put(&self, c: Oop, i: usize, v: Oop) {
        self.call(c, b"at:put:", &[size(i).unwrap(), v]);
    }
    fn method(&self, n: &[u8], c: Oop) -> Oop {
        self.call(
            self.call(self.fixture, b"methodAt:", &[self.symbol(n)]),
            b"withContext:",
            &[c],
        )
    }
    fn vt(&self, v: Oop) -> Oop {
        self.call(self.host.id.root, b"behaviorOf:", &[v])
    }
    fn bind(&self, vt: Oop, n: &[u8], m: Oop) {
        self.call(vt, b"addMethod", &[self.symbol(n), m]);
    }
    fn callback(&self, selector: &[u8], n: &[u8], c: Oop) -> Oop {
        let r = self.fresh();
        let vt = self.call(self.host.id.root, b"ensurePrivateVtableFor:", &[r]);
        self.bind(vt, selector, self.method(n, c));
        r
    }
    fn assert(&self, st: Oop, e: Oop, a: Oop, v: Oop) {
        assert_eq!(self.call(st, b"assert:attr:value:", &[e, a, v]), st);
    }
    fn read(&self, st: Oop, e: Oop, a: Oop) -> Oop {
        self.call(st, b"currentValue:attr:", &[e, a])
    }
    fn watch(&self, a: &[Oop]) {
        self.host.send(self.probe, b"watch", a).unwrap();
    }
    fn mask(&self) -> usize {
        as_size(self.call(self.probe, b"mask", &[])).unwrap()
    }
    fn identity(&self, v: Oop) -> Oop {
        self.call(self.host.id.root, b"identityOf:", &[v])
    }
    fn load(&self) -> Oop {
        let q = self.host.scope();
        self.keep(
            self.host
                .load_native(
                    &q,
                    &self
                        .host
                        .resources
                        .lib_dir
                        .join("native/libbirnam_store.dylib"),
                    &[self.host.namespace],
                )
                .unwrap(),
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
    fn forwarding(&self, alias: &[u8]) -> Oop {
        self.context(&[
            self.gc,
            self.symbol(alias),
            size(0).unwrap(),
            size(0).unwrap(),
        ])
    }
}
impl Drop for H<'_> {
    fn drop(&mut self) {
        self.clear();
    }
}
fn isolated_case(name: &str) -> bool {
    if std::env::var("BIRNAM_STORE_CASE").as_deref() == Ok(name) {
        return false;
    }
    let r = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--ignored", "--nocapture"])
        .env("BIRNAM_STORE_CASE", name)
        .output()
        .unwrap();
    assert!(
        r.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&r.stdout),
        String::from_utf8_lossy(&r.stderr)
    );
    true
}
fn collecting_construction(h: &H<'_>) {
    let original_store = h.named(b"Store");
    let key = h.symbol(b"Store");
    h.call(h.host.namespace, b"at:put:", &[key, ptr::null_mut()]);
    let vt = h.vt(h.host.id.root);
    let original = h.save(vt, b"allocationSizeOf:", b"storeOriginalExtent:");
    let c = h.forwarding(b"storeOriginalExtent:");
    h.bind(vt, b"allocationSizeOf:", h.method(b"forward", c));
    let new = h.call(original_store, b"new", &[]);
    h.restore(vt, b"allocationSizeOf:", b"storeOriginalExtent:", original);
    assert!(h.count(c, 2) >= 2);
    assert_ne!(new, original_store);
    assert_eq!(h.metric(new, b"basis"), 3);
    assert_eq!(h.load(), original_store);
    assert!(h.call(h.host.namespace, b"at:", &[key]).is_null());
    h.call(h.host.namespace, b"at:put:", &[key, original_store]);
}
fn semantics(h: &H<'_>) {
    let st = h.call(h.named(b"Store"), b"new", &[]);
    let e = h.symbol(b"entity");
    let a = h.symbol(b"attribute");
    let v = h.i(41);
    let w = h.i(42);
    let nil = h.named(b"nil");
    assert_eq!(h.read(st, e, a), nil);
    assert!(h.call(st, b"rawCurrentValue:attr:", &[e, a]).is_null());
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
    h.assert(st, a, h.symbol(b"history"), h.named(b"true"));
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
    let tags = h.symbol(b"tags");
    h.assert(st, tags, h.symbol(b"cardinality"), many);
    h.assert(st, e, tags, v);
    h.assert(st, e, tags, w);
    h.assert(st, e, tags, w);
    assert_eq!(h.number(h.call(st, b"count:attr:", &[e, tags])), 2);
    h.call(st, b"retract:attr:value:", &[e, tags, v]);
    assert_eq!(h.number(h.call(st, b"count:attr:", &[e, tags])), 1);
    assert_eq!(h.call(st, b"cardinality:", &[tags]), many);
    assert!(h.host.send(st, b"assert:attr:value:", &[]).is_err());
    assert!(
        h.host
            .send(st, b"retainLast:", &[size(4).unwrap()])
            .is_err()
    );
    let cloned = h.call(st, b"clone", &[]);
    assert!(h.host.send(cloned, b"basis", &[]).is_err());
    assert_eq!(h.load(), h.named(b"Store"));
}
fn cells_and_dispatch(h: &H<'_>) {
    let st = h.call(h.named(b"Store"), b"new", &[]);
    let e = h.fresh();
    let a = h.symbol(b"cell-key");
    let value = h.fresh();
    let cell = h.call(st, b"cellFor:attr:", &[e, a]);
    h.call(cell, b"set:", &[value]);
    h.watch(&[st, e, a, value, cell]);
    h.clear();
    h.keep(cell);
    h.collect();
    assert_eq!(h.mask(), 31);
    assert_eq!(h.call(cell, b"value", &[]), value);
    let vt = h.named(b"StoreVt");
    let old = h.save(vt, b"currentValue:attr:", b"storeOriginalRead:attr:");
    let c = h.forwarding(b"storeOriginalRead:attr:");
    h.bind(vt, b"currentValue:attr:", h.method(b"forward", c));
    assert_eq!(h.call(cell, b"value", &[]), value);
    assert_eq!(h.count(c, 2), 1);
    h.restore(vt, b"currentValue:attr:", b"storeOriginalRead:attr:", old);
    let ivt = h.vt(h.named(b"Integer"));
    let old = h.save(ivt, b"fromWord:", b"storeOriginalInteger:");
    let c = h.forwarding(b"storeOriginalInteger:");
    h.bind(ivt, b"fromWord:", h.method(b"forward", c));
    assert!(h.metric(st, b"basis") > 3);
    assert_eq!(h.count(c, 2), 1);
    h.restore(ivt, b"fromWord:", b"storeOriginalInteger:", old);
    h.clear();
    h.collect();
    assert_eq!(h.mask(), 0);
}
fn snapshots(h: &H<'_>, fail: bool) {
    let st = h.call(h.named(b"Store"), b"new", &[]);
    let e = h.fresh();
    let a = h.symbol(b"snapshot-many");
    h.assert(st, a, h.symbol(b"cardinality"), h.symbol(b"many"));
    let values = (0..8).map(|_| h.fresh()).collect::<Vec<_>>();
    for &v in &values {
        h.assert(st, e, a, v);
    }
    let ids = values
        .iter()
        .map(|&v| [h.identity(e), h.identity(a), h.identity(v)])
        .collect::<Vec<_>>();
    let fault = h.hooks.fault(71);
    let c = h.context(&[
        h.gc,
        st,
        size(0).unwrap(),
        if fail { fault } else { ptr::null_mut() },
    ]);
    let visitor = h.callback(b"value:value:value:", b"snapshot", c);
    h.watch(&values);
    h.clear();
    h.keep(st);
    h.keep(visitor);
    let result = unsafe { h.host.id.send(st, h.symbol(b"liveTriplesDo:"), &[visitor]) };
    if fail {
        assert_eq!(result.unwrap_err(), fault);
    } else {
        assert!(result.is_ok());
        for row in ids {
            assert_eq!(as_size(h.call(h.probe, b"seen", &row)), Some(1));
        }
    }
    h.collect();
    assert_eq!(
        h.mask(),
        0,
        "snapshot references released after completion or failure"
    );
    h.clear();
    h.collect();
}
fn image_transfer(h: &H<'_>) {
    let st = h.call(h.named(b"Store"), b"new", &[]);
    let target = h.call(h.named(b"Store"), b"new", &[]);
    let e = h.fresh();
    let a = h.symbol(b"durable");
    let v = h.fresh();
    h.assert(st, e, a, v);
    let temporary = h.symbol(b"temporary");
    h.assert(st, temporary, h.symbol(b"imageTransient"), h.named(b"true"));
    h.assert(st, e, temporary, h.fresh());
    h.call(target, b"imageLoadBegin", &[]);
    let c = h.context(&[h.gc, target, size(0).unwrap()]);
    let visitor = h.callback(b"e:a:v:tx:op:historied:", b"imageCopy", c);
    h.call(st, b"imageFactsInto:", &[visitor]);
    assert!(h.count(c, 2) >= 4);
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
    assert_eq!(h.read(target, e, temporary), h.named(b"nil"));
    assert_eq!(h.metric(st, b"basis"), h.metric(target, b"basis"));
    assert!(
        h.host
            .send(
                target,
                b"imageAddFact:attr:value:tx:op:historied:",
                &[e, a, v, h.i(-1), h.i(0), h.named(b"true")]
            )
            .is_err()
    );
}
fn globals_are_independent(h: &H<'_>) {
    let globals = h.named(b"Globals");
    let st = h.named(b"Store");
    let e = h.symbol(b"__globals__");
    let key = h.symbol(b"historied-global");
    h.assert(st, key, h.symbol(b"history"), h.named(b"true"));
    let a = h.i(11);
    let b = h.i(22);
    h.call(globals, b"at:put:", &[key, a]);
    assert_eq!(h.read(st, e, key), h.named(b"nil"));
    h.assert(st, e, key, a);
    let basis = h.call(st, b"basis", &[]);
    h.call(st, b"retainSince:", &[basis]);
    h.call(globals, b"at:put:", &[key, b]);
    assert_eq!(h.read(st, e, key), a);
    h.assert(st, e, key, b);
    assert_eq!(h.call(st, b"valueOf:attr:asOf:", &[e, key, basis]), a);
    h.assert(st, e, key, a);
    assert_eq!(h.call(globals, b"at:", &[key]), b);
    let missing = h.symbol(b"absent-global");
    assert_eq!(h.call(globals, b"includes:", &[missing]), h.named(b"false"));
    h.call(globals, b"at:put:", &[missing, h.named(b"nil")]);
    assert_eq!(h.call(globals, b"includes:", &[missing]), h.named(b"true"));
    let vt = h.named(b"StoreVt");
    let old = h.save(vt, b"currentValue:attr:", b"storeOriginalRead:attr:");
    let c = h.forwarding(b"storeOriginalRead:attr:");
    h.bind(vt, b"currentValue:attr:", h.method(b"forward", c));
    assert_eq!(h.call(globals, b"at:", &[key]), b);
    assert_eq!(h.count(c, 2), 0);
    assert_eq!(h.read(st, e, key), a);
    assert_eq!(h.count(c, 2), 1);
    h.restore(vt, b"currentValue:attr:", b"storeOriginalRead:attr:", old);
    assert!(h.call(h.host.namespace, b"at:", &[key]).is_null());
}
fn mop_hooks(h: &H<'_>) {
    let st = h.named(b"Store");
    let target = h.fresh();
    let vt = h.call(h.host.id.root, b"ensurePrivateVtableFor:", &[target]);
    let selector = h.symbol(b"mopProbe");
    let value = h.i(17);
    let method = h.hooks.method(b"fixed", value);
    let c = h.context(&[
        h.gc,
        st,
        h.named(b"false"),
        size(0).unwrap(),
        size(0).unwrap(),
        method,
    ]);
    let auth = h.callback(b"value", b"authorize", c);
    let observer = h.callback(b"managedMethodChanged:on:", b"observe", c);
    for _ in 0..2 {
        assert_eq!(h.call(st, b"setMethodMutationAuthorizer:", &[auth]), st);
        assert_eq!(h.call(st, b"setMethodMutationObserver:", &[observer]), st);
    }
    for (setter, kind) in [
        (
            b"setMethodMutationObserver:".as_slice(),
            b"methodMutationObserverAlreadySet".as_slice(),
        ),
        (
            b"setMethodMutationAuthorizer:",
            b"methodMutationAuthorizerAlreadySet",
        ),
    ] {
        let err = h.call(st, setter, &[h.fresh()]);
        assert_eq!(h.call(err, b"kind", &[]), h.symbol(kind));
    }
    h.watch(&[auth, observer]);
    h.clear();
    for v in [target, vt, selector, method] {
        h.keep(v);
    }
    h.collect();
    assert_eq!(h.mask(), 3);
    assert_eq!(
        h.call(st, b"addMethod:on:with:", &[selector, vt, method]),
        h.named(b"false")
    );
    assert!(h.call(vt, b"lookup", &[selector]).is_null());
    assert_eq!(h.count(c, 4), 0);
    h.put(c, 2, h.named(b"true"));
    assert_eq!(
        h.call(st, b"addMethod:on:with:", &[selector, vt, method]),
        st
    );
    assert_eq!(h.call(target, b"mopProbe", &[]), value);
    assert_eq!(h.read(st, vt, selector), method);
    assert_eq!(h.count(c, 4), 1);
    let target_store = h.call(st, b"new", &[]);
    h.call(target_store, b"imageLoadBegin", &[]);
    let copy = h.context(&[h.gc, target_store, size(0).unwrap()]);
    let visitor = h.callback(b"e:a:v:tx:op:historied:", b"imageCopy", copy);
    h.call(st, b"imageFactsInto:", &[visitor]);
    assert_eq!(h.read(target_store, vt, selector), h.named(b"nil"));
    let global = h.symbol(b"historied-global");
    assert_eq!(
        h.number(h.read(target_store, h.symbol(b"__globals__"), global)),
        11
    );
    let store_vt = h.named(b"StoreVt");
    let changed = h.symbol(b"methodChanged:on:");
    let original = h.call(store_vt, b"lookup", &[changed]);
    let ignored = h.context(&[h.named(b"nil"), size(0).unwrap()]);
    h.bind(
        store_vt,
        b"methodChanged:on:",
        h.hooks.method(b"fixedCount", ignored),
    );
    let raw = h.hooks.method(b"fixed", value);
    h.bind(vt, b"mopProbe", raw);
    assert_eq!(h.count(ignored, 1), 0);
    h.call(st, b"methodChanged:on:", &[selector, vt]);
    assert_eq!(h.count(ignored, 1), 1);
    assert_eq!(h.read(st, vt, selector), method);
    h.bind(store_vt, b"methodChanged:on:", original);
    h.bind(vt, b"mopProbe", raw);
    assert_eq!(h.read(st, vt, selector), method);
    h.call(st, b"methodChanged:on:", &[selector, vt]);
    assert_eq!(h.read(st, vt, selector), h.named(b"nil"));
    assert_eq!(h.count(c, 4), 1);
    h.call(st, b"addMethod:on:with:", &[selector, vt, method]);
    h.put(c, 5, ptr::null_mut());
    h.call(st, b"removeMethod:on:", &[selector, vt]);
    assert_eq!(h.read(st, vt, selector), h.named(b"nil"));
    assert_eq!(h.count(c, 4), 3);
    let error = h.call(st, b"removeMethod:on:", &[selector, vt]);
    assert_eq!(
        h.call(error, b"kind", &[]),
        h.symbol(b"missingDirectMethod")
    );
    h.bind(vt, b"mopProbe", raw);
    let before = h.metric(st, b"basis");
    h.call(st, b"backfillMirror", &[]);
    assert_eq!(h.metric(st, b"basis"), before);
    assert_eq!(h.read(st, vt, selector), h.symbol(b"primitive"));
    h.call(vt, b"removeMethod:", &[selector]);
    assert_eq!(h.read(st, vt, selector), h.symbol(b"primitive"));
    h.call(st, b"methodChanged:on:", &[selector, vt]);
    assert_eq!(h.read(st, vt, selector), h.named(b"nil"));
    // Captured dictionary operations still require explicit invalidation.
    h.put(c, 5, method);
    h.call(st, b"addMethod:on:with:", &[selector, vt, method]);
    let add = h.call(h.host.id.root, b"bind:to:", &[h.symbol(b"addMethod"), vt]);
    let remove = h.call(
        h.host.id.root,
        b"bind:to:",
        &[h.symbol(b"removeMethod:"), vt],
    );
    h.call(
        add,
        b"invokeOn:arguments:",
        &[vt, h.context(&[selector, raw])],
    );
    h.call(st, b"methodChanged:on:", &[selector, vt]);
    assert_eq!(h.read(st, vt, selector), h.named(b"nil"));
    h.call(st, b"backfillMirror", &[]);
    h.call(
        remove,
        b"invokeOn:arguments:",
        &[vt, h.context(&[selector])],
    );
    h.call(st, b"methodChanged:on:", &[selector, vt]);
    assert_eq!(h.read(st, vt, selector), h.named(b"nil"));
}
fn phase_safety(h: &H<'_>) {
    let st = h.call(h.named(b"Store"), b"new", &[]);
    let c = h.context(&[
        st,
        h.fresh(),
        h.symbol(b"during-mark"),
        h.fresh(),
        size(0).unwrap(),
    ]);
    let owner = h.callback(b"referencesOf:do:", b"phase", c);
    let vt = h.call(h.named(b"ObjectVt"), b"delegated", &[]);
    h.call(vt, b"allocate", &[size(0).unwrap()]);
    h.call(h.gc, b"trace:with:", &[vt, owner]);
    let before = h.metric(st, b"basis");
    h.collect();
    assert_eq!(h.count(c, 4), 1);
    assert_eq!(h.metric(st, b"basis"), before);
    h.call(h.gc, b"trace:with:", &[vt, ptr::null_mut()]);
}
fn resource_limit(h: &H<'_>) {
    let st = h.call(h.named(b"Store"), b"new", &[]);
    h.call(st, b"retainSince:", &[h.i(0)]);
    let e = h.symbol(b"limited-entity");
    let a = h.symbol(b"limited-history");
    let mut rejected = false;
    for n in 0..20 {
        let before = h.metric(st, b"basis");
        let result = h.call(st, b"rawAssert:attr:value:", &[e, a, h.i(n)]);
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
fn source(h: &H<'_>) {
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

fn history_plateaus(h: &H<'_>) {
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
                    unsafe { h.host.id.send(h.host.roots, h.release, &[token]) }.unwrap();
                }
            }
            h.call(store, b"compact", &[]);
            h.collect();
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
                unsafe { h.host.id.send(h.host.roots, h.release, &[token]) }.unwrap();
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

#[test]
#[ignore = "requires production Oop modules"]
fn store_history_cells_snapshots_and_source_use_id_messages() {
    if isolated_case("store_history_cells_snapshots_and_source_use_id_messages") {
        return;
    }
    let host = oop::bootstrap();
    let h = H::new(&host);
    if std::env::var_os("BIRNAM_STORE_TEST_LIMIT").is_some() {
        resource_limit(&h);
        return;
    }
    collecting_construction(&h);
    semantics(&h);
    history_plateaus(&h);
    h.clear();
    h.collect();
    cells_and_dispatch(&h);
    snapshots(&h, false);
    snapshots(&h, true);
    image_transfer(&h);
    phase_safety(&h);
    source(&h);
    globals_are_independent(&h);
    mop_hooks(&h);
    h.clear();
    h.collect();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "store_history_cells_snapshots_and_source_use_id_messages",
            "--include-ignored",
            "--nocapture",
        ])
        .env("BIRNAM_STORE_TEST_LIMIT", "1")
        .env("BIRNAM_STORE_FACT_BYTES_LIMIT", "256")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
#[ignore = "requires production Oop modules"]
fn fact_cells_restore_owner_aliases_and_reject_removed_store_apis() {
    if isolated_case("fact_cells_restore_owner_aliases_and_reject_removed_store_apis") {
        return;
    }
    let host = oop::bootstrap();
    let h = H::new(&host);
    let store = h.call(h.named(b"Store"), b"new", &[]);
    let one = h.i(1);
    let two = h.i(2);
    let nil = h.named(b"nil");
    let entity = h.fresh();
    let attr = h.symbol(b"cellAmount");
    let cell = h.call(store, b"cellFor:attr:", &[entity, attr]);
    assert_eq!(h.call(cell, b"set:", &[one]), cell);
    let fields = [
        h.call(cell, b"imageFieldAt:", &[h.i(0)]),
        h.call(cell, b"imageFieldAt:", &[h.i(1)]),
        h.call(cell, b"imageFieldAt:", &[h.i(2)]),
    ];
    assert_eq!(fields, [store, entity, attr]);
    let restored = h.call(store, b"cellFor:attr:", &[nil, nil]);
    h.call(restored, b"restoreImageStore:entity:attr:", &fields);
    h.collect();
    assert_eq!(h.call(restored, b"value", &[]), one);
    assert_eq!(h.call(restored, b"set:", &[two]), restored);
    assert_eq!(h.call(cell, b"value", &[]), two);
    assert!(
        host.send(
            restored,
            b"restoreImageStore:entity:attr:",
            &[h.symbol(b"localCell"), one, nil]
        )
        .is_err()
    );
    assert_eq!(h.call(restored, b"value", &[]), two);
    let vt = h.vt(store);
    for selector in [
        b"localCell:".as_slice(),
        b"localCell:prototype:",
        b"swapImageStateWith:atRevision:stagedRevision:",
    ] {
        assert!(h.call(vt, b"lookup", &[h.symbol(selector)]).is_null());
    }
    h.clear();
}
