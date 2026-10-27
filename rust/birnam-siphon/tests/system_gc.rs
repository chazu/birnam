//! System GC controls, owner payload policies and live tracing changes through Oops.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use birnam_host_support::{Context, Scope};
use std::{path::Path, ptr};
struct H<'a> {
    host: &'a Context,
    q: Scope<'a>,
    hooks: oop::hooks::Hooks<'a>,
    fixture: Oop,
    probe: Oop,
    gc: Oop,
    model: Oop,
}
impl H<'_> {
    fn named(&self, n: &[u8]) -> Oop {
        self.host.named(n).unwrap()
    }
    fn key(&self, n: &[u8]) -> Oop {
        self.hooks.symbol(n)
    }
    fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        self.q.call(r, n, a).unwrap()
    }
    fn vt(&self, value: Oop) -> Oop {
        self.call(self.host.id.root, b"behaviorOf:", &[value])
    }
    fn binding(&self, vt: Oop, n: &[u8]) -> Oop {
        let selected = self.call(vt, b"lookup", &[self.key(n)]);
        assert!(!selected.is_null() && selected != self.named(b"nil"));
        selected
    }
    fn bind(&self, vt: Oop, n: &[u8], method: Oop) {
        self.call(vt, b"addMethod", &[self.key(n), method]);
    }
    fn method(&self, n: &[u8], c: Oop) -> Oop {
        self.call(
            self.call(self.fixture, b"methodAt:", &[self.key(n)]),
            b"withContext:",
            &[c],
        )
    }
    fn bits(&self, value: Oop) -> usize {
        self.q.word_bits(value).unwrap()
    }
    fn mode(&self, precise: bool) {
        self.call(
            self.gc,
            b"precisePayloads:",
            &[size(precise as usize).unwrap()],
        );
    }
    fn collect(&self) {
        self.call(self.gc, b"collectPrecisely", &[]);
    }
    fn watch(&self, values: &[Oop]) {
        self.host.send(self.probe, b"watch", values).unwrap();
    }
    fn mask(&self) -> usize {
        as_size(self.call(self.probe, b"mask", &[])).unwrap()
    }
    fn allocate(&self, vt: Oop) -> Oop {
        self.host
            .send(vt, b"allocate", &[size(0).unwrap()])
            .unwrap()
    }
    fn behavior(&self) -> Oop {
        self.host
            .send(self.named(b"ObjectVt"), b"delegated", &[])
            .unwrap()
    }
    fn layout(&self, vt: Oop, kind: usize) {
        self.call(
            self.model,
            b"setLayoutOf:to:rest:",
            &[vt, self.q.byte_values(&[]).unwrap(), size(kind).unwrap()],
        );
    }
    fn raw(&self, vt: Oop, target: Oop, kind: Option<usize>) -> Oop {
        let carrier = self
            .host
            .send(
                self.model,
                b"allocate:bytes:",
                &[vt, size(size_of::<Oop>()).unwrap()],
            )
            .unwrap();
        let (selector, value) = if kind == Some(0) {
            (
                b"restoreImageSlotAt:in:put:".as_slice(),
                self.q.word(target.addr()).unwrap(),
            )
        } else {
            (b"slotAt:in:put:".as_slice(), target)
        };
        self.host
            .send(self.model, selector, &[size(0).unwrap(), carrier, value])
            .unwrap();
        carrier
    }
    fn fail(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        unsafe { self.host.id.send(r, self.key(n), a) }.unwrap_err()
    }
    fn spy(&self, alias: &[u8]) -> Oop {
        self.hooks
            .context(&[self.key(alias), size(0).unwrap(), ptr::null_mut()])
    }
}
#[test]
#[ignore = "requires production Oop modules"]
fn gc_controls_and_payload_policy_use_messages() {
    let host = oop::bootstrap();
    let q = host.scope();
    let path = std::env::temp_dir().join(format!("birnam-system-gc-{}.dylib", std::process::id()));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/system-gc.c")
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
    let h = H {
        host: &host,
        q,
        hooks: oop::hooks::Hooks::new(&host),
        fixture,
        probe,
        gc: host.named(b"GC").unwrap(),
        model: host.named(b"ObjectModel").unwrap(),
    };
    let probe_vt = h.call(host.id.root, b"ensurePrivateVtableFor:", &[probe]);
    for name in [
        b"watch".as_slice(),
        b"mask",
        b"events",
        b"configureWalk",
        b"allocation:bytes:identity:",
    ] {
        h.bind(probe_vt, name, h.method(name, probe));
    }
    let system = h.named(b"System");
    let nil = h.named(b"nil");
    let truth = h.named(b"true");
    let falsity = h.named(b"false");
    assert_eq!(as_size(h.call(h.gc, b"precisePayloads", &[])), Some(0));
    for selector in [b"autoGc:".as_slice(), b"preciseGc:"] {
        for args in [vec![], vec![nil], vec![truth, falsity]] {
            assert_eq!(h.call(system, selector, &args), nil);
        }
    }
    for selector in [b"gc".as_slice(), b"liveObjects"] {
        assert_eq!(h.call(system, selector, &[nil]), nil);
    }
    for selector in [b"automaticCollection:".as_slice(), b"precisePayloads:"] {
        assert!(host.send(h.gc, selector, &[]).is_err());
        assert!(host.send(h.gc, selector, &[truth]).is_err());
    }
    assert_eq!(h.call(system, b"autoGc:", &[falsity]), system);
    // Check the unmodified service before any binding replacement.
    assert!(host.send(h.gc, b"allocationCount", &[nil]).is_err());
    let initial = h.call(system, b"liveObjects", &[]);
    assert!(h.bits(h.call(initial, b"word", &[])) > 0);
    let gc_vt = h.vt(h.gc);
    let integer = h.named(b"Integer");
    let int_vt = h.vt(integer);
    let int_original = h.binding(int_vt, b"fromWord:");
    let sentinel = h.q.keep(h.allocate(h.named(b"ObjectVt"))).unwrap();
    let constructor = h.hooks.context(&[sentinel, size(0).unwrap()]);
    let query = h.hooks.context(&[
        int_vt,
        h.hooks.method(b"fixedCount", constructor),
        h.q.word(usize::MAX).unwrap(),
        size(0).unwrap(),
    ]);
    // Query the current GC/Id method first, then the current Integer constructor.
    for (behavior, selector, public) in [
        (gc_vt, b"collect".as_slice(), b"gc".as_slice()),
        (gc_vt, b"allocationCount", b"liveObjects"),
        (h.vt(host.id.root), b"allocationCount", b"liveObjects"),
    ] {
        let original = h.binding(behavior, selector);
        h.bind(behavior, selector, h.method(b"query", query));
        let answer = h.call(system, public, &[]);

        assert_eq!(answer, sentinel);
        h.bind(behavior, selector, original);
        h.bind(int_vt, b"fromWord:", int_original);
    }
    assert_eq!(h.hooks.count(query, 3), 3);
    assert_eq!(h.hooks.count(constructor, 1), 3);
    let original = h.binding(gc_vt, b"collect");
    for count in [0, 7, i64::MAX as usize, (i64::MAX as usize) + 1, usize::MAX] {
        h.bind(
            gc_vt,
            b"collect",
            h.hooks.method(b"fixed", h.q.word(count).unwrap()),
        );
        let result = h.call(system, b"gc", &[]);
        assert_eq!(h.bits(h.call(result, b"word", &[])), count);
    }
    h.bind(gc_vt, b"collect", original);
    let alias_b = h.q.keep(h.behavior()).unwrap();
    let alias = h.q.keep(h.allocate(alias_b)).unwrap();
    for selector in [b"autoGc:".as_slice(), b"preciseGc:"] {
        h.bind(alias_b, selector, h.binding(h.named(b"SystemVt"), selector));
        assert_eq!(h.call(alias, selector, &[falsity]), alias);
    }
    h.bind(alias_b, b"gc", h.binding(h.named(b"SystemVt"), b"gc"));
    let collector = h.spy(b"collectPrecisely");
    let original = h.binding(gc_vt, b"collect");
    h.bind(gc_vt, b"collect", h.method(b"forward", collector));
    let unrooted = h.allocate(alias_b);
    h.watch(&[unrooted]);
    let result = h.call(unrooted, b"gc", &[]);
    h.bits(h.call(result, b"word", &[]));
    assert_eq!(h.mask(), 1);
    assert_eq!(h.hooks.count(collector, 1), 1);
    let fault = h.hooks.fault(971);
    h.hooks.put(collector, 2, fault);
    assert_eq!(h.fail(system, b"gc", &[]), fault);
    h.bind(gc_vt, b"collect", original);
    let count_vt = h.vt(host.id.root);
    let count_original = h.binding(count_vt, b"allocationCount");
    h.bind(
        count_vt,
        b"allocationCount",
        h.hooks
            .method(b"deny", h.hooks.context(&[fault, size(0).unwrap()])),
    );
    assert_eq!(h.fail(system, b"liveObjects", &[]), fault);
    h.bind(count_vt, b"allocationCount", count_original);
    for selector in [b"automaticCollection:".as_slice(), b"precisePayloads:"] {
        let original = h.binding(gc_vt, selector);
        h.bind(gc_vt, b"savedGcSetting:", original);
        let spy = h.spy(b"savedGcSetting:");
        h.bind(gc_vt, selector, h.method(b"forward", spy));
        let public = if selector == b"automaticCollection:" {
            b"autoGc:".as_slice()
        } else {
            b"preciseGc:"
        };
        assert_eq!(h.call(system, public, &[falsity]), system);
        assert_eq!(h.hooks.count(spy, 1), 1);
        h.hooks.put(spy, 2, fault);
        assert_eq!(h.fail(system, public, &[truth]), fault);
        h.bind(gc_vt, selector, original);
        h.call(gc_vt, b"removeMethod:", &[h.key(b"savedGcSetting:")]);
    }
    let count = h.bits(h.call(host.id.root, b"allocationCount", &[]));
    let answer = h.call(system, b"liveObjects", &[]);
    let observed = h.bits(h.call(answer, b"word", &[]));
    assert!(
        observed >= count,
        "query follows authored argument construction"
    );
    assert!(
        h.bits(h.call(host.id.root, b"allocationCount", &[])) > observed,
        "result boxing follows query"
    );
    for family in [b"Integer".as_slice(), b"Float", b"Word", b"Range"] {
        let empty = h.q.keep(h.allocate(h.vt(h.named(family)))).unwrap();
        h.watch(&[empty]);
        h.mode(false);
        h.collect();
        assert_eq!(h.mask(), 1);
    }
    // ObjectModel owns these slots and interprets their replaceable layout.
    for (kind, precise, alive) in [(0, false, true), (0, true, false), (1, true, true)] {
        let case = host.scope();
        h.mode(precise);
        let b = case.keep(h.behavior()).unwrap();
        h.layout(b, kind);
        let target = h.allocate(h.named(b"ObjectVt"));
        let carrier = case.keep(h.raw(b, target, Some(kind))).unwrap();
        h.watch(&[target, carrier]);
        h.collect();
        assert_eq!(h.mask(), if alive { 3 } else { 2 });
    }
    {
        let case = host.scope();
        h.mode(true);
        let b = case.keep(h.behavior()).unwrap();
        let target = h.allocate(h.named(b"ObjectVt"));
        case.keep(h.raw(b, target, None)).unwrap();
        h.watch(&[target]);
        h.collect();
        assert_eq!(h.mask(), 1, "missing layout remains conservative");
    }
    for family in [b"Integer".as_slice(), b"Float", b"Word"] {
        for precise in [false, true] {
            let case = host.scope();
            h.mode(precise);
            let target = h.allocate(h.named(b"ObjectVt"));
            let selector = if family == b"Integer" {
                b"fromWord:".as_slice()
            } else {
                b"fromBits:"
            };
            let number = case
                .call(
                    h.named(family),
                    selector,
                    &[case.word(target.addr()).unwrap()],
                )
                .unwrap();
            h.call(host.id.root, b"ensurePrivateVtableFor:", &[number]);
            h.watch(&[target]);
            h.collect();
            assert_eq!(
                h.mask(),
                usize::from(!precise),
                "{} precise={precise}",
                String::from_utf8_lossy(family)
            );
        }
    }
    for precise in [false, true] {
        let case = host.scope();
        h.mode(precise);
        let target = h.allocate(h.named(b"ObjectVt"));
        {
            let endpoints = host.scope();
            let bits = endpoints.word(target.addr()).unwrap();
            let begin = endpoints.call(integer, b"fromWord:", &[bits]).unwrap();
            let end = endpoints.call(integer, b"fromWord:", &[bits]).unwrap();
            case.call(begin, b"to:", &[end]).unwrap();
        }
        h.watch(&[target]);
        h.collect();
        assert_eq!(h.mask(), usize::from(!precise), "Range precise={precise}");
    }
    // Text now has zero-payload handles, not the former writable mirror words.
    // Its native bytes never retain objects, even when they spell an address.
    for family in [b"String".as_slice(), b"StringBuilder"] {
        for precise in [false, true] {
            let case = host.scope();
            h.mode(precise);
            let target = h.allocate(h.named(b"ObjectVt"));
            let text = case.string(&target.addr().to_ne_bytes()).unwrap();
            let carrier = if family == b"String" {
                text
            } else {
                let builder = case.call(h.named(family), b"new", &[]).unwrap();
                case.call(builder, b"add:", &[text]).unwrap();
                builder
            };
            assert_eq!(
                as_size(h.call(host.id.root, b"allocationSizeOf:", &[carrier])),
                Some(0)
            );
            assert!(
                host.send(
                    h.model,
                    b"slotAt:in:put:",
                    &[size(0).unwrap(), carrier, target]
                )
                .is_err()
            );
            h.watch(&[target]);
            h.collect();
            assert_eq!(h.mask(), 0);
        }
    }
    // The custom owner reports carrier then trigger; marking visits them LIFO.
    for (next, change_layout) in [(true, false), (false, false), (true, true)] {
        h.mode(!next);
        let case = host.scope();
        let b = case.keep(h.behavior()).unwrap();
        h.layout(b, 0);
        let target = h.allocate(h.named(b"ObjectVt"));
        let carrier = h.raw(b, target, Some(0));
        let parent = case.keep(h.allocate(b)).unwrap();
        let trigger = h.allocate(b);
        let owner = case.call(h.named(b"Object"), b"clone", &[]).unwrap();
        let owner_b = case
            .call(host.id.root, b"ensurePrivateVtableFor:", &[owner])
            .unwrap();
        let context = h
            .hooks
            .context(&[h.gc, h.model, h.q.byte_values(&[]).unwrap()]);
        h.bind(owner_b, b"referencesOf:do:", h.method(b"walk", context));
        host.send(
            probe,
            b"configureWalk",
            &[
                parent,
                carrier,
                trigger,
                b,
                size(next as usize).unwrap(),
                size(change_layout as usize).unwrap(),
            ],
        )
        .unwrap();
        h.call(h.gc, b"trace:with:", &[b, owner]);
        h.watch(&[target]);
        h.collect();
        assert_eq!(
            as_size(h.call(probe, b"events", &[])),
            Some(6),
            "trigger then carrier"
        );
        assert_eq!(h.mask(), usize::from(!next || change_layout));
        h.call(h.gc, b"trace:with:", &[b, ptr::null_mut()]);
        h.call(owner_b, b"removeMethod:", &[h.key(b"referencesOf:do:")]);
    }
    h.mode(true);
    let original = h.binding(gc_vt, b"precisePayloads");
    h.bind(gc_vt, b"savedPrecision", original);
    let spy = h.spy(b"savedPrecision");
    h.bind(gc_vt, b"precisePayloads", h.method(b"forward", spy));
    h.collect();
    assert!(h.hooks.count(spy, 1) > 0);
    let context = h
        .hooks
        .context(&[h.gc, h.named(b"ObjectVt"), spy, fault, probe]);
    h.bind(
        probe_vt,
        b"failedCollection",
        h.method(b"failedCollection", context),
    );
    assert_eq!(h.fail(probe, b"failedCollection", &[]), fault);
    assert_eq!(h.mask(), 1);
    h.bind(gc_vt, b"precisePayloads", original);
    h.call(gc_vt, b"removeMethod:", &[h.key(b"savedPrecision")]);
    h.collect();
    assert_eq!(h.mask(), 0);
    let context = h.hooks.context(&[h.gc, h.named(b"ObjectVt"), probe]);
    h.bind(probe_vt, b"stackRoot", h.method(b"stackRoot", context));
    for precise in [false, true] {
        h.mode(precise);
        h.call(probe, b"stackRoot", &[]);
    }
    h.mode(true);
    let context = h.hooks.context(&[
        h.gc,
        system,
        truth,
        falsity,
        h.named(b"ObjectVt"),
        h.named(b"Roots"),
    ]);
    h.bind(probe_vt, b"accounting", h.method(b"accounting", context));
    assert_eq!(as_size(h.call(probe, b"accounting", &[])), Some(1));
    let dead = h.allocate(h.named(b"ObjectVt"));
    h.watch(&[dead]);
    h.collect();
    assert_eq!(h.mask(), 0);
    let owner = h.call(h.named(b"Object"), b"clone", &[]);
    let owner_b = h.call(host.id.root, b"ensurePrivateVtableFor:", &[owner]);
    let finalizer = h.hooks.context(&[h.gc, size(0).unwrap(), size(0).unwrap()]);
    h.bind(owner_b, b"finalize:", h.method(b"finalize", finalizer));
    let dead = h.allocate(h.named(b"ObjectVt"));
    host.send(h.gc, b"finalize:with:", &[dead, owner]).unwrap();
    h.watch(&[dead]);
    h.mode(false);
    h.collect();
    assert_eq!(h.hooks.count(finalizer, 1), 1);
    assert_eq!(h.hooks.count(finalizer, 2), 0);
    assert_eq!(h.mask(), 0);
    assert_eq!(as_size(h.call(h.gc, b"precisePayloads", &[])), Some(1));
    h.call(owner_b, b"removeMethod:", &[h.key(b"finalize:")]);
    let system_vt = h.named(b"SystemVt");
    let original = h.binding(system_vt, b"gc");
    h.bind(system_vt, b"gc", h.hooks.method(b"fixed", sentinel));
    let source = h.q.string(b"[Module load: #systemGc]").unwrap();
    let loaded = h.call(h.named(b"Codegen"), b"eval:", &[source]);
    h.q.check_result(loaded).unwrap();
    assert_eq!(as_size(h.call(h.gc, b"precisePayloads", &[])), Some(1));
    assert_eq!(h.call(system, b"gc", &[]), sentinel);
    h.bind(system_vt, b"gc", original);
    let source = h.q.string(b"[System preciseGc: false]").unwrap();
    assert_eq!(h.call(h.named(b"Codegen"), b"eval:", &[source]), system);
    assert_eq!(as_size(h.call(h.gc, b"precisePayloads", &[])), Some(0));
}
