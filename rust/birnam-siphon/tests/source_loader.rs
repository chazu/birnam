//! Source loading and the authored Module manager through production owners.
mod support;
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
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
    unsafe fn named(&self, name: &[u8]) -> Oop {
        unsafe {
            object(send(
                self.id,
                self.ns,
                b"at:",
                &[Operand::object(self.id.intern_name(name).unwrap())],
            ))
        }
    }
    unsafe fn keep(&self, o: Oop) -> Oop {
        self.tokens
            .borrow_mut()
            .push(unsafe { retain(self.id, self.roots, o) });
        o
    }
    unsafe fn string(&self, b: &[u8]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                self.named(b"String"),
                b"fromBytes:",
                &[Operand::bytes(b)],
            )))
        }
    }
    unsafe fn call(&self, o: Oop, n: &[u8], a: &[Oop]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                o,
                n,
                &a.iter().copied().map(Operand::object).collect::<Vec<_>>(),
            )))
        }
    }
    unsafe fn error(&self, o: Oop, kind: &[u8], message: &[u8]) {
        unsafe {
            assert_eq!(
                self.call(o, b"kind", &[]),
                self.id.intern_name(kind).unwrap()
            );
            let m = self.call(o, b"message", &[]);
            let b = send(self.id, m, b"bytes", &[]).as_bytes().unwrap();
            assert_eq!(borrowed_bytes(b.pointer, b.length).unwrap(), message);
        }
    }
}

impl H {
    unsafe fn eval(&self, source: &[u8]) -> Oop {
        unsafe { self.call(self.named(b"Codegen"), b"eval:", &[self.string(source)]) }
    }
}
#[test]
#[ignore = "requires production native modules"]
fn source_loading_uses_live_owners() {
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
            ns,
            roots: named(b"Roots"),
            gc: named(b"GC"),
            tokens: RefCell::new(Vec::new()),
        };
        let codegen = h.named(b"Codegen");
        let system = h.named(b"System");
        let nil = h.named(b"nil");
        let temp =
            std::env::temp_dir().join(format!("birnam-source-loader-{}", std::process::id()));
        std::fs::create_dir_all(&temp).unwrap();
        let manager = temp.join("modules.birnam");
        std::fs::write(&manager, include_bytes!("../../../src/id/modules.birnam")).unwrap();
        let result = h.call(
            system,
            b"load:",
            &[h.string(manager.as_os_str().as_bytes())],
        );
        assert_ne!(h.call(result, b"isError", &[]), h.named(b"true"));
        assert_eq!(h.eval(b"[Module isLoaded: #modules]"), h.named(b"true"));
        h.eval(b"[Module named: #a requires: #[]]; [Module named: #b requires: #[#a]]");
        let order = h.eval(b"[Module loadOrder: #[#b]]");
        assert_eq!(
            h.call(order, b"at:", &[h.eval(b"0")]),
            id.intern_name(b"a").unwrap()
        );
        assert_eq!(
            h.call(order, b"at:", &[h.eval(b"1")]),
            id.intern_name(b"b").unwrap()
        );
        collect(id, h.gc);
        assert_eq!(h.eval(b"[Module isRegistered: #b]"), h.named(b"true"));

        let module = h.eval(b"Module");
        assert_eq!(
            send(
                id,
                ns,
                b"at:",
                &[Operand::object(id.intern_name(b"Module").unwrap())]
            )
            .kind(),
            native::UNIT
        );
        assert_ne!(
            h.call(
                codegen,
                b"moduleSourceFor:",
                &[id.intern_name(b"modules").unwrap()]
            ),
            nil
        );
        let gold = h.string(b"[Module named: #gold requires: #[#dep]]");
        let revision = h.call(codegen, b"sourceRevisionOf:", &[gold]);
        let metadata = h.call(codegen, b"moduleMetadataOf:", &[gold]);
        assert_eq!(h.number(h.call(metadata, b"size", &[])), 5);
        assert_eq!(
            h.at(metadata, 0),
            id.intern_name(b"moduleMetadata").unwrap()
        );
        assert_eq!(h.at(metadata, 1), id.intern_name(b"gold").unwrap());
        assert_eq!(h.at(h.at(metadata, 2), 0), id.intern_name(b"dep").unwrap());
        assert_eq!(h.bytes(h.at(metadata, 4)), h.bytes(h.at(revision, 6)));
        assert_eq!(h.call(codegen, b"moduleMetadataOf:", &[nil]), nil);
        let assets = h.call(codegen, b"embeddedFilesOf:", &[nil]);
        assert_eq!(h.number(h.call(assets, b"size", &[])), 0);
        assert_eq!(
            h.call(codegen, b"moduleMetadataOf:", &[h.string(b"42")]),
            nil
        );
        assert_eq!(h.number(h.call(revision, b"size", &[])), 7);
        assert_eq!(
            h.at(revision, 0),
            id.intern_name(b"sourceRevision").unwrap()
        );
        assert_eq!(h.at(revision, 1), id.intern_name(b"gold").unwrap());
        assert_eq!(h.at(h.at(revision, 2), 0), id.intern_name(b"dep").unwrap());
        assert_eq!(h.at(revision, 3), gold);
        assert_eq!(
            h.bytes(h.at(revision, 4)),
            b"c8c54eaf253655e27e4d4cee9b55fc09f044f3761c8c67ebcbbe4017768c1710"
        );
        assert_eq!(
            h.bytes(h.at(revision, 5)),
            b"14a1c4f166b7504907b76034d87e440d2c26d8498b8776f872fbeab3ada986ad"
        );
        assert_eq!(
            h.bytes(h.at(revision, 6)),
            b"92749bc6da04d6045dc5a0eb386d10ad952542a70ca6754c431c1d2b8bbeed9f"
        );

        for bad in [
            b"42".as_slice(),
            b"[Module named: #a requires: #[]]; [Module named: #b requires: #[]]",
            b"[Module named: #a requires: #[]]; [Reflect defineMethod: #foo on: Object as: [| 1]]",
            b"[Module named: #a requires: #[]]; [Reflect defineMethod: #foo on: Object as: [:s | 1]]; [Reflect removeMethod: #foo on: Object]",
        ] { assert_eq!(h.call(codegen, b"sourceRevisionOf:", &[h.string(bad)]), nil); }
        assert_eq!(
            h.call(
                codegen,
                b"recordModuleSource:named:",
                &[gold, id.intern_name(b"wrong").unwrap()]
            ),
            nil
        );
        assert_eq!(
            h.call(
                codegen,
                b"recordModuleSource:named:",
                &[gold, id.intern_name(b"gold").unwrap()]
            ),
            gold
        );
        let docs = h.call(codegen, b"recordedMethodSources", &[]);
        assert!(
            h.bytes(h.at(docs, 0))
                .starts_with(b"[Reflect defineMethod: #named:requires:")
        );
        let equal = h.string(b"[Module named: #gold requires: #[#dep]]");
        assert_ne!(equal, gold);
        assert_eq!(
            h.call(
                codegen,
                b"recordModuleSource:named:",
                &[equal, id.intern_name(b"gold").unwrap()]
            ),
            equal
        );
        assert_eq!(
            h.call(
                codegen,
                b"moduleSourceFor:",
                &[id.intern_name(b"gold").unwrap()]
            ),
            gold
        );

        let path = temp.join("example.birnam");
        let path_object = h.string(path.as_os_str().as_bytes());
        std::fs::write(
            &path,
            b"[Module named: #example requires: #[]]; let loadedAnswer := 42; [Object clone]",
        )
        .unwrap();
        let loaded = h.call(system, b"load:", &[path_object]);
        assert_ne!(loaded, nil);
        assert_eq!(h.number(h.eval(b"loadedAnswer")), 42);
        // The authored manager lives in Globals. The loader deliberately does
        // not bridge Namespace and Globals to mark this module loaded.
        assert_eq!(h.eval(b"[Module isLoaded: #example]"), h.named(b"false"));
        let registered = h.call(
            codegen,
            b"moduleSourceFor:",
            &[id.intern_name(b"example").unwrap()],
        );
        assert_eq!(h.bytes(registered), std::fs::read(&path).unwrap());
        send(
            id,
            ns,
            b"at:put:",
            &[
                Operand::object(id.intern_name(b"Module").unwrap()),
                Operand::object(module),
            ],
        );
        h.call(system, b"load:", &[path_object]);
        assert_eq!(h.eval(b"[Module isLoaded: #example]"), h.named(b"true"));
        // A duplicate-source nil response still allows ordinary evaluation.
        std::fs::write(
            &path,
            b"[Module named: #example requires: #[]]; let loadedAnswer := 43; 43",
        )
        .unwrap();
        assert_eq!(h.number(h.call(system, b"load:", &[path_object])), 43);
        assert_eq!(
            h.call(
                codegen,
                b"moduleSourceFor:",
                &[id.intern_name(b"example").unwrap()]
            ),
            registered
        );
        assert_eq!(h.call(codegen, b"lastError", &[]), nil);

        std::fs::write(&path, b"nil").unwrap();
        assert_eq!(h.call(system, b"load:", &[path_object]), nil);
        assert_eq!(h.call(codegen, b"lastError", &[]), nil);
        std::fs::write(&path, b"[").unwrap();
        h.error(
            h.call(system, b"load:", &[path_object]),
            b"syntaxError",
            b"source file failed to compile",
        );
        assert!(
            String::from_utf8_lossy(&h.bytes(h.call(codegen, b"lastError", &[])))
                .contains("syntax error")
        );
        std::fs::write(
            &path,
            b"[Module named: #failed requires: #[]]; [Error kind: #testFailure message: \"kept\"]",
        )
        .unwrap();
        h.error(
            h.call(system, b"load:", &[path_object]),
            b"testFailure",
            b"kept",
        );
        assert_ne!(
            h.call(
                codegen,
                b"moduleSourceFor:",
                &[id.intern_name(b"failed").unwrap()]
            ),
            nil
        );
        assert_eq!(h.eval(b"[Module isLoaded: #failed]"), h.named(b"false"));
        std::fs::remove_file(&path).unwrap();
        h.error(
            h.call(system, b"load:", &[path_object]),
            b"ioError",
            b"could not read source file",
        );
        assert_eq!(h.call(system, b"load:", &[]), nil);
        assert_eq!(h.call(system, b"load:", &[nil]), nil);
        h.error(
            h.call(system, b"load:", &[h.string(b"\xff")]),
            b"ioError",
            b"could not read source file",
        );

        // No caller root survives this registration; Codegen traces the original.
        let retained = object(send(
            id,
            h.named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(b"[Module named: #retained requires: #[]]")],
        ));
        assert_eq!(
            object(send(
                id,
                codegen,
                b"recordModuleSource:named:",
                &[
                    Operand::object(retained),
                    Operand::object(id.intern_name(b"retained").unwrap())
                ]
            )),
            retained
        );
        collect(id, h.gc);
        assert!(present(id, retained));
        assert_eq!(
            h.call(
                codegen,
                b"moduleSourceFor:",
                &[id.intern_name(b"retained").unwrap()]
            ),
            retained
        );
        exercise_callbacks(&h, &temp);
        assert_eq!(
            h.call(codegen, b"sealInitialGeneration", &[]),
            h.named(b"true")
        );
        assert_eq!(
            h.call(
                codegen,
                b"recordModuleSource:named:",
                &[gold, id.intern_name(b"gold").unwrap()]
            ),
            nil
        );
        assert!(
            String::from_utf8_lossy(&h.bytes(h.call(codegen, b"lastError", &[])))
                .contains("module introduction")
        );
        std::fs::write(&path, b"[Module named: #afterSeal requires: #[]]; 99").unwrap();
        assert_eq!(h.number(h.call(system, b"load:", &[path_object])), 99);
        assert_eq!(
            h.call(
                codegen,
                b"moduleSourceFor:",
                &[id.intern_name(b"afterSeal").unwrap()]
            ),
            nil
        );
        std::fs::remove_dir_all(&temp).unwrap();
    }
}

impl H {
    unsafe fn bytes(&self, o: Oop) -> Vec<u8> {
        unsafe {
            let b = send(self.id, o, b"bytes", &[]).as_bytes().unwrap();
            borrowed_bytes(b.pointer, b.length).unwrap().to_vec()
        }
    }
    unsafe fn number(&self, o: Oop) -> usize {
        unsafe {
            send(
                self.id,
                self.named(b"Integer"),
                b"wordOf:",
                &[Operand::object(o)],
            )
            .as_word()
            .unwrap()
        }
    }
    unsafe fn at(&self, o: Oop, i: usize) -> Oop {
        unsafe {
            let i = self.keep(object(send(
                self.id,
                self.named(b"Integer"),
                b"fromWord:",
                &[Operand::word(i)],
            )));
            self.call(o, b"at:", &[i])
        }
    }
}

unsafe fn binding(id: Protocol, vt: Oop, selector: &[u8]) -> NativeEntry {
    unsafe {
        send(
            id,
            id.receiver,
            b"bindingOf:in:",
            &[
                Operand::object(id.intern_name(selector).unwrap()),
                Operand::object(vt),
            ],
        )
        .as_entry()
        .unwrap()
    }
}
unsafe fn set(id: Protocol, vt: Oop, selector: &[u8], entry: NativeEntry) {
    unsafe {
        send(
            id,
            vt,
            b"addMethod",
            &[
                Operand::object(id.intern_name(selector).unwrap()),
                Operand::entry(entry),
            ],
        );
    }
}
struct Forward {
    roots: Oop,
    id: Protocol,
    gc: Oop,
    alias: Oop,
    fail: Cell<bool>,
    calls: Cell<usize>,
}
unsafe extern "C" fn forward(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let hook = &*c.cast::<Forward>();
        hook.calls.set(hook.calls.get() + 1);
        let args = borrowed_slice(a, n).unwrap();
        let mut tokens = vec![retain(hook.id, hook.roots, r)];
        for value in args.iter().filter_map(|v| v.as_object()) {
            tokens.push(retain(hook.id, hook.roots, value));
        }
        collect(hook.id, hook.gc);
        let result = if hook.fail.get() {
            Outcome::failed(native::INVALID_REQUEST, 883)
        } else {
            hook.id
                .send(r, hook.alias, args)
                .map_or_else(|e| e, Outcome::loaded)
        };
        for token in tokens {
            release(hook.id, hook.roots, token);
        }
        result
    }
}
struct BytesHook {
    id: Protocol,
    gc: Oop,
    alias: Oop,
    trigger: Oop,
    codegen: Oop,
    nested: Oop,
    name: Oop,
    once: Cell<bool>,
    path: Oop,
    path_bytes: RefCell<Vec<u8>>,
}
unsafe extern "C" fn bytes_hook(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let hook = &*c.cast::<BytesHook>();
        collect(hook.id, hook.gc);
        if r == hook.path {
            return Outcome::loaded(Operand::bytes(&hook.path_bytes.borrow()));
        }
        if r == hook.trigger && !hook.once.replace(true) {
            hook.path_bytes.borrow_mut().fill(b'x');
            let recorded = send(
                hook.id,
                hook.codegen,
                b"recordModuleSource:named:",
                &[Operand::object(hook.nested), Operand::object(hook.name)],
            );
            assert_eq!(object(recorded), hook.nested);
        }
        hook.id
            .send(r, hook.alias, borrowed_slice(a, n).unwrap())
            .map_or_else(|e| e, Outcome::loaded)
    }
}
unsafe extern "C" fn reply(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::object(c))
}
unsafe fn exercise_callbacks(h: &H, temp: &std::path::Path) {
    unsafe {
        let id = h.id;
        let codegen = h.named(b"Codegen");
        let system = h.named(b"System");
        let nil = h.named(b"nil");
        let behavior_of = |o| object(send(id, id.receiver, b"behaviorOf:", &[Operand::object(o)]));
        let lang_vt = behavior_of(h.named(b"Lang"));
        let string_factory_vt = behavior_of(h.named(b"String"));
        let string_vt = h.named(b"StringVt");
        let codegen_vt = behavior_of(codegen);
        let system_vt = behavior_of(system);
        let mut wrappers = Vec::new();
        for (vt, selector, alias) in [
            (
                lang_vt,
                b"parse:".as_slice(),
                b"testSourceParse:".as_slice(),
            ),
            (string_factory_vt, b"fromBytes:", b"testSourceString:"),
            (codegen_vt, b"sourceRevisionOf:", b"testSourceRevision:"),
            (
                codegen_vt,
                b"recordModuleSource:named:",
                b"testSourceRecord:named:",
            ),
            (codegen_vt, b"evalPersisting:from:", b"testSourceEval:from:"),
        ] {
            let entry = binding(id, vt, selector);
            set(id, vt, alias, entry);
            let mut hook = Box::new(Forward {
                roots: h.roots,
                id,
                gc: h.gc,
                alias: id.intern_name(alias).unwrap(),
                fail: Cell::new(false),
                calls: Cell::new(0),
            });
            set(
                id,
                vt,
                selector,
                NativeEntry::new(forward, (&mut *hook as *mut Forward).cast()),
            );
            wrappers.push((vt, selector, alias, entry, hook));
        }
        let source = h.string(b"[Module named: #collected requires: #[]]; 67");
        assert_eq!(
            h.call(
                codegen,
                b"recordModuleSource:named:",
                &[source, id.intern_name(b"collected").unwrap()]
            ),
            source
        );
        let revision = h.call(codegen, b"sourceRevisionOf:", &[source]);
        assert_eq!(h.at(revision, 3), source);
        let path = temp.join("collecting.birnam");
        std::fs::write(&path, b"[Module named: #loadedCollected requires: #[]]; 68").unwrap();
        let path_object = h.string(path.as_os_str().as_bytes());
        assert_eq!(h.number(h.call(system, b"load:", &[path_object])), 68);
        // Reentry while comparing retained String contents must not hold a
        // registry borrow, suppress GC, or publish a duplicate registration.
        let old = h.call(
            codegen,
            b"moduleSourceFor:",
            &[id.intern_name(b"gold").unwrap()],
        );
        let equal = h.string(&h.bytes(old));
        let nested = h.string(b"[Module named: #reentrant requires: #[]]");
        let original_bytes = binding(id, string_vt, b"bytes");
        set(id, string_vt, b"testSourceBytes", original_bytes);
        let mut bytes = BytesHook {
            id,
            gc: h.gc,
            alias: id.intern_name(b"testSourceBytes").unwrap(),
            trigger: old,
            codegen,
            nested,
            name: h.keep(id.intern_name(b"reentrant").unwrap()),
            once: Cell::new(false),
            path: path_object,
            path_bytes: RefCell::new(path.as_os_str().as_bytes().to_vec()),
        };
        set(
            id,
            string_vt,
            b"bytes",
            NativeEntry::new(bytes_hook, (&mut bytes as *mut BytesHook).cast()),
        );
        assert_eq!(
            h.call(
                codegen,
                b"recordModuleSource:named:",
                &[equal, id.intern_name(b"gold").unwrap()]
            ),
            equal
        );
        assert!(bytes.once.get());
        assert_eq!(h.call(codegen, b"moduleSourceFor:", &[bytes.name]), nested);
        // Mutate the path during source inspection, after the loader copied it.
        // The original path Object still goes to Codegen; only the borrow changes.
        bytes.once.set(false);
        bytes
            .path_bytes
            .replace(path.as_os_str().as_bytes().to_vec());
        bytes.trigger = source;
        // Supply a selected source only for the file read; other String
        // construction still forwards through a send.
        let factory_entry = binding(id, string_factory_vt, b"fromBytes:");
        set(id, string_factory_vt, b"testSuppliedSource:", factory_entry);
        let mut supply = Supply {
            id,
            alias: id.intern_name(b"testSuppliedSource:").unwrap(),
            source,
            bytes: std::fs::read(&path).unwrap(),
        };
        set(
            id,
            string_factory_vt,
            b"fromBytes:",
            NativeEntry::new(supply_source, (&mut supply as *mut Supply).cast()),
        );
        assert_eq!(h.number(h.call(system, b"load:", &[path_object])), 67);
        assert!(bytes.once.get());
        set(id, string_factory_vt, b"fromBytes:", factory_entry);
        send(
            id,
            string_factory_vt,
            b"removeMethod:",
            &[Operand::object(
                id.intern_name(b"testSuppliedSource:").unwrap(),
            )],
        );
        set(id, string_vt, b"bytes", original_bytes);
        send(
            id,
            string_vt,
            b"removeMethod:",
            &[Operand::object(id.intern_name(b"testSourceBytes").unwrap())],
        );
        // Native dependency failures propagate without becoming language nil.
        wrappers[0].4.fail.set(true);
        let failure = id
            .send_named(system, b"load:", &[Operand::object(path_object)])
            .unwrap_err();
        assert_eq!(
            (failure.status, failure.detail),
            (native::INVALID_REQUEST, 883)
        );
        wrappers[0].4.fail.set(false);
        // An Error returned by registration is propagated by identity and prevents eval.
        let rejected = h.eval(b"[Error kind: #registrationRefused message: \"stop\"]");
        let record_entry = binding(id, codegen_vt, b"recordModuleSource:named:");
        let before = wrappers[4].4.calls.get();
        set(
            id,
            codegen_vt,
            b"recordModuleSource:named:",
            NativeEntry::new(reply, rejected),
        );
        assert_eq!(h.call(system, b"load:", &[path_object]), rejected);
        assert_eq!(wrappers[4].4.calls.get(), before);
        set(id, codegen_vt, b"recordModuleSource:named:", record_entry);
        for (vt, selector, alias, entry, hook) in wrappers {
            assert!(hook.calls.get() > 0);
            set(id, vt, selector, entry);
            send(
                id,
                vt,
                b"removeMethod:",
                &[Operand::object(id.intern_name(alias).unwrap())],
            );
        }
        // Compiler construction rejection stays distinct from an evaluated Error.
        let tcc_vt = behavior_of(h.named(b"TCC"));
        let compile_entry = binding(id, tcc_vt, b"compile:");
        let input = h.string(b"42");
        for result in [nil, rejected] {
            set(id, tcc_vt, b"compile:", NativeEntry::new(reply, result));
            assert_eq!(
                h.call(codegen, b"evalPersisting:from:", &[input, path_object]),
                nil
            );
            assert_eq!(
                h.bytes(h.call(codegen, b"lastError", &[])),
                b"RustCodegen generated C failed to compile"
            );
        }
        set(id, tcc_vt, b"compile:", compile_entry);
        assert_eq!(h.call(codegen, b"evalPersisting:from:", &[input, nil]), nil);
        assert_eq!(
            h.bytes(h.call(codegen, b"lastError", &[])),
            b"RustCodegen source path was not a String"
        );
        send(
            id,
            h.ns,
            b"at:put:",
            &[
                Operand::object(id.intern_name(b"Codegen").unwrap()),
                Operand::object(nil),
            ],
        );
        h.error(
            h.call(system, b"load:", &[path_object]),
            b"noCodegen",
            b"Codegen module is not loaded",
        );
        // File errors precede the missing-Codegen check.
        h.error(
            h.call(
                system,
                b"load:",
                &[h.string(temp.join("absent.birnam").as_os_str().as_bytes())],
            ),
            b"ioError",
            b"could not read source file",
        );
        send(
            id,
            h.ns,
            b"at:put:",
            &[
                Operand::object(id.intern_name(b"Codegen").unwrap()),
                Operand::object(codegen),
            ],
        );
        // Reinitialization must not overwrite an installed method replacement.
        let original = binding(id, system_vt, b"load:");
        set(id, system_vt, b"load:", NativeEntry::new(reply, nil));
        let loader = directory().join("libbirnam_source_loader.dylib");
        assert_eq!(
            id.send(
                id.receiver,
                id.load,
                &[
                    Operand::bytes(loader.as_os_str().as_bytes()),
                    Operand::object(h.ns)
                ]
            )
            .unwrap()
            .kind(),
            native::UNIT
        );
        assert_eq!(h.call(system, b"load:", &[path_object]), nil);
        set(id, system_vt, b"load:", original);
        h.call(codegen, b"evalPersisting:", &[h.string(b"nil")]);
    }
}

struct Supply {
    id: Protocol,
    alias: Oop,
    source: Oop,
    bytes: Vec<u8>,
}
unsafe extern "C" fn supply_source(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let s = &*c.cast::<Supply>();
        let args = borrowed_slice(a, n).unwrap();
        let bytes = args[0].as_bytes().unwrap();
        if borrowed_bytes(bytes.pointer, bytes.length).unwrap() == s.bytes {
            return Outcome::loaded(Operand::object(s.source));
        }
        s.id.send(r, s.alias, args)
            .map_or_else(|e| e, Outcome::loaded)
    }
}
