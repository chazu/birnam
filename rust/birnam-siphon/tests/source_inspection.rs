//! Static observations preserve source identity and do not publish or retain inputs.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{self as abi, Oop};
use birnam_host_support::{Context, Scope};
use oop::hooks::Hooks;
use std::{cell::RefCell, os::unix::ffi::OsStrExt};
struct H<'a> {
    context: &'a Context,
    tokens: RefCell<Vec<Scope<'a>>>,
}
impl H<'_> {
    fn named(&self, n: &[u8]) -> Oop {
        self.context
            .named(n)
            .unwrap_or_else(|e| panic!("{}: {e}", String::from_utf8_lossy(n)))
    }
    fn keep(&self, value: Oop) -> Oop {
        let q = self.context.scope();
        q.keep(value).unwrap();
        self.tokens.borrow_mut().push(q);
        value
    }
    fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        self.keep(self.context.send(r, n, a).unwrap())
    }
    fn text(&self, b: &[u8]) -> Oop {
        let q = self.context.scope();
        let value = q.string(b).unwrap();
        self.tokens.borrow_mut().push(q);
        value
    }
    fn num(&self, v: usize) -> Oop {
        self.call(
            self.named(b"Integer"),
            b"fromWord:",
            &[abi::size(v).unwrap()],
        )
    }
    fn word(&self, o: Oop) -> usize {
        let q = self.context.scope();
        q.word_bits(q.call(self.named(b"Integer"), b"wordOf:", &[o]).unwrap())
            .unwrap()
    }
    fn at(&self, o: Oop, i: usize) -> Oop {
        self.call(o, b"at:", &[self.num(i)])
    }
    fn bytes(&self, o: Oop) -> Vec<u8> {
        let q = self.context.scope();
        q.read_bytes(q.call(o, b"bytes", &[]).unwrap()).unwrap()
    }
    fn clear(&self, n: usize) {
        self.tokens.borrow_mut().truncate(n);
    }
}
const MODULE: &[u8] = b"[Module named: #inspectionProbe requires: #[#dependency]]";
const ASSETS:&[u8]=b"[System embedFile: \"a.bin\"]; [| [System embedFile: \"b.bin\"]]; [System embedFile: \"a.bin\"]";
#[test]
#[ignore = "requires production modules"]
fn temporary_source_inspection_does_not_publish_or_retain() {
    let context = oop::bootstrap();
    let id = context.id;
    let ns = context.namespace;
    let h = H {
        context: &context,
        tokens: RefCell::new(vec![]),
    };
    let hooks = Hooks::new(&context);
    let send = |r, n: &[u8], a: &[Oop]| context.send(r, n, a).unwrap();
    let collect = || send(h.named(b"GC"), b"collectPrecisely", &[]);
    let q = context.scope();
    let module = context.global(&q, b"Module").unwrap();
    q.call(
        module,
        b"libDir:",
        &[q.string(context.resources.lib_dir.as_os_str().as_bytes())
            .unwrap()],
    )
    .unwrap();
    let loaded = q
        .call(
            context.named(b"Codegen").unwrap(),
            b"load:",
            &[q.string(
                context
                    .resources
                    .lib_dir
                    .join("libbirnam_store.dylib")
                    .as_os_str()
                    .as_bytes(),
            )
            .unwrap()],
        )
        .unwrap();
    q.check_result(loaded).unwrap();
    assert_ne!(loaded, context.named(b"nil").unwrap());
    q.call(module, b"markLoaded:", &[context.intern(b"store").unwrap()])
        .unwrap();
    let cg = h.named(b"Codegen");
    let store = h.named(b"Store");
    let gc = h.named(b"GC");
    let nil = h.named(b"nil");
    send(gc, b"precisePayloads:", &[abi::size(1).unwrap()]);
    let store_vt = h.named(b"StoreVt");
    let lang_vt = h.call(id.root, b"behaviorOf:", &[h.named(b"Lang")]);
    let write_key = hooks.symbol(b"assert:attr:value:");
    let parse_key = hooks.symbol(b"parse:");
    let writes_original = hooks.capture(store_vt, write_key);
    let parses_original = hooks.capture(lang_vt, parse_key);
    let writes = hooks.context(&[
        writes_original,
        gc,
        abi::size(0).unwrap(),
        std::ptr::null_mut(),
    ]);
    let parses = hooks.context(&[
        parses_original,
        gc,
        abi::size(0).unwrap(),
        std::ptr::null_mut(),
    ]);
    send(
        store_vt,
        b"addMethod",
        &[write_key, hooks.method(b"observed", writes)],
    );
    send(
        lang_vt,
        b"addMethod",
        &[parse_key, hooks.method(b"observed", parses)],
    );
    let mark = h.tokens.borrow().len();
    for calls in [2, 0, 1, 10, 30, 30] {
        let facts_before = h.word(h.call(store, b"retainedFactCount", &[]));
        h.clear(mark);
        collect();
        let before = abi::as_size(send(id.root, b"allocationCount", &[])).unwrap();
        let parses_before = hooks.count(parses, 2);
        for _ in 0..calls {
            let source = h.text(MODULE);
            let same = h.text(MODULE);
            assert_ne!(source, same);
            for source in [source, same] {
                let m = h.call(cg, b"moduleMetadataOf:", &[source]);
                assert_eq!(h.word(h.call(m, b"size", &[])), 5);
                assert_eq!(h.at(m, 0), context.intern(b"moduleMetadata").unwrap());
                assert_eq!(h.at(m, 1), context.intern(b"inspectionProbe").unwrap());
                assert_eq!(h.at(h.at(m, 2), 0), context.intern(b"dependency").unwrap());
                assert_eq!(h.bytes(h.at(m, 3)).len(), 64);
            }
            let paths = h.call(cg, b"embeddedFilesOf:", &[h.text(ASSETS)]);
            assert_eq!(h.word(h.call(paths, b"size", &[])), 2);
            assert_eq!(h.bytes(h.at(paths, 0)), b"a.bin");
            assert_eq!(h.bytes(h.at(paths, 1)), b"b.bin");
            h.clear(mark);
        }
        let facts_after = h.word(h.call(store, b"retainedFactCount", &[]));
        h.clear(mark);
        collect();
        let after = abi::as_size(send(id.root, b"allocationCount", &[])).unwrap();
        eprintln!(
            "source inspection {calls}: objects={before}->{after} facts={facts_before}->{facts_after} writes={}",
            hooks.count(writes, 2)
        );
        assert_eq!(facts_after, facts_before, "inspection published facts");
        assert_eq!(hooks.count(writes, 2), 0);
        assert_eq!(
            hooks.count(parses, 2) - parses_before,
            calls * 3,
            "inspection must use current parser"
        );
        if calls != 2 {
            assert_eq!(
                after, before,
                "temporary inspection retained language objects"
            );
        }
    }
    // Nil, valid non-module and malformed source have no publication side effect.
    assert_eq!(h.call(cg, b"moduleMetadataOf:", &[nil]), nil);
    assert_eq!(h.call(cg, b"moduleMetadataOf:", &[h.text(b"42")]), nil);
    h.call(cg, b"clearCompilationError", &[]);
    assert_eq!(h.call(cg, b"moduleMetadataOf:", &[h.text(b"[broken")]), nil);
    assert_eq!(
        h.bytes(h.call(cg, b"lastError", &[])),
        b"compiled source metadata is missing; rebuild the artifact or image"
    );
    assert_eq!(
        h.word(h.call(h.call(cg, b"embeddedFilesOf:", &[nil]), b"size", &[])),
        0
    );
    assert_eq!(hooks.count(writes, 2), 0);
    // Preparing returns an ordinary owned value; retaining just the String is
    // not a hidden metadata root. Neither preparation nor registration needs db.
    let store_root = h.keep(store);
    let db = h.keep(h.named(b"db"));
    send(ns, b"at:put:", &[context.intern(b"Store").unwrap(), nil]);
    send(ns, b"at:put:", &[context.intern(b"db").unwrap(), nil]);
    let mark = h.tokens.borrow().len();
    for count in [2, 0, 1, 30, 30] {
        collect();
        let before = abi::as_size(send(id.root, b"allocationCount", &[])).unwrap();
        for _ in 0..count {
            let source = h.text(MODULE);
            let record = h.call(cg, b"analyzeSourceMetadata:", &[source]);
            assert_eq!(h.call(cg, b"sourceTextOf:", &[record]), source);
            assert_ne!(h.call(cg, b"compiledSourceMetadataFor:", &[record]), nil);
            assert_eq!(h.call(cg, b"compiledSourceMetadataFor:", &[source]), nil);
            h.clear(mark);
        }
        collect();
        let after = abi::as_size(send(id.root, b"allocationCount", &[])).unwrap();
        eprintln!("owned compilations {count}: objects={before}->{after}");
        if count != 2 {
            assert_eq!(after, before, "unregistered compilation retained objects")
        }
    }
    let source = h.text(b"[Module named: #retainedProbe requires: #[]]");
    let record = h.call(cg, b"analyzeSourceMetadata:", &[source]);
    assert_eq!(
        h.at(record, 0),
        context.intern(b"compiledSourceV1").unwrap()
    );
    assert_eq!(h.at(record, 1), source);
    assert_eq!(
        hooks.count(writes, 2),
        0,
        "preparation must not publish facts"
    );
    let cg_vt = h.call(id.root, b"behaviorOf:", &[cg]);
    let getter_key = hooks.symbol(b"retainedSourceMetadataOf:");
    let getter_original = hooks.capture(cg_vt, getter_key);
    let getter = hooks.context(&[
        getter_original,
        gc,
        abi::size(0).unwrap(),
        std::ptr::null_mut(),
    ]);
    send(
        cg_vt,
        b"addMethod",
        &[getter_key, hooks.method(b"observed", getter)],
    );
    assert_ne!(h.call(cg, b"compiledSourceMetadataFor:", &[record]), nil);
    assert_eq!(
        hooks.count(getter, 2),
        1,
        "compiler bypassed the current metadata accessor"
    );
    send(cg_vt, b"addMethod", &[getter_key, getter_original]);
    let parsed = hooks.count(parses, 2);
    hooks.put(parses, 3, hooks.fault(99301));
    assert_ne!(h.call(cg, b"moduleMetadataOf:", &[record]), nil);
    assert_eq!(
        h.word(h.call(h.call(cg, b"embeddedFilesOf:", &[record]), b"size", &[])),
        0
    );
    assert_eq!(h.call(cg, b"analyzeSourceMetadata:", &[record]), record);
    let name = h.keep(context.intern(b"retainedProbe").unwrap());
    assert_eq!(
        h.call(cg, b"recordModuleSource:named:", &[record, name]),
        source
    );
    assert_eq!(h.call(cg, b"moduleSourceFor:", &[name]), source);
    assert_eq!(h.call(cg, b"compiledSourceFor:", &[source]), record);
    assert_eq!(
        h.call(
            cg,
            b"sourceTextOf:",
            &[h.at(h.call(cg, b"sourceRevisionOf:", &[record]), 7)]
        ),
        source
    );
    assert_ne!(h.call(cg, b"compiledSourceMetadataFor:", &[source]), nil);
    assert_eq!(
        hooks.count(parses, 2),
        parsed,
        "retained records must not reparse"
    );
    let fresh = h.text(b"[Module named: #retainedProbe requires: #[]]");
    assert_eq!(
        h.call(cg, b"compiledSourceMetadataFor:", &[fresh]),
        nil,
        "equal text must not merge source identity"
    );
    let missing = h.call(cg, b"compiledSource:metadata:", &[source, nil]);
    assert_eq!(h.call(cg, b"compiledSourceMetadataFor:", &[missing]), nil);
    let malformed = h.call(h.named(b"Vector"), b"new", &[]);
    let bad = h.call(cg, b"compiledSource:metadata:", &[source, malformed]);
    assert!(
        context
            .send(cg, b"compiledSourceMetadataFor:", &[bad])
            .is_err()
    );
    assert_eq!(
        hooks.count(parses, 2),
        parsed,
        "missing/malformed retained metadata must not parse"
    );
    // Registry replacement selects a new immutable value; earlier revisions keep
    // their original record and source identity. Use a copied descriptor.
    let snapshot = h.call(cg, b"sourceRevisionOf:", &[record]);
    let mut replacement = h.call(h.named(b"Vector"), b"new", &[]);
    for i in 0..3 {
        replacement = h.call(replacement, b"add:", &[h.at(record, i)]);
    }
    assert_ne!(replacement, record);
    assert_eq!(
        h.call(cg, b"replaceCompiledSource:named:", &[replacement, name]),
        source
    );
    assert_eq!(h.call(cg, b"compiledSourceFor:", &[source]), replacement);
    assert_eq!(h.at(snapshot, 7), record);
    assert_eq!(hooks.count(writes, 2), 0);
    send(
        ns,
        b"at:put:",
        &[context.intern(b"Store").unwrap(), store_root],
    );
    send(ns, b"at:put:", &[context.intern(b"db").unwrap(), db]);
    // The former fact key is now independent user data, never consulted/removed.
    let key = h.keep(context.intern(b"compiledSourceMetadata").unwrap());
    h.call(store, b"assert:attr:value:", &[source, key, malformed]);
    assert_ne!(h.call(cg, b"compiledSourceMetadataFor:", &[source]), nil);
    assert_eq!(
        h.call(store, b"currentValue:attr:", &[source, key]),
        malformed
    );
    assert_eq!(hooks.count(writes, 2), 1);
    let before = hooks.count(parses, 2);
    assert!(
        context
            .send(cg, b"analyzeSourceMetadata:", &[source])
            .is_err(),
        "explicit text analysis must use the current parser even for registered text"
    );
    assert_eq!(hooks.count(parses, 2), before + 1);
    send(store_vt, b"addMethod", &[write_key, writes_original]);
    send(lang_vt, b"addMethod", &[parse_key, parses_original]);
    h.clear(0);
}
