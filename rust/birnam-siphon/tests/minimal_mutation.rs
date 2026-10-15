//! Existing direct Method/dictionary protocol on a floor without Store or Generation.
mod support;
use birnam_abi::{
    Oop,
    id_protocol::{Operand, Outcome, Protocol},
};
use std::os::unix::ffi::OsStrExt;
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    path::PathBuf,
};
use support::*;

struct H {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    tokens: RefCell<Vec<usize>>,
}
unsafe fn mount(id: Protocol, module: &str, ns: Oop) {
    let path = directory().join(format!("libbirnam_{module}.dylib"));
    unsafe {
        id.send(
            id.receiver,
            id.load,
            &[
                Operand::bytes(path.as_os_str().as_bytes()),
                Operand::object(ns),
            ],
        )
    }
    .unwrap_or_else(|e| panic!("load {module}: {e:?}"));
}
impl H {
    unsafe fn keep(&self, o: Oop) -> Oop {
        self.tokens
            .borrow_mut()
            .push(unsafe { retain(self.id, self.roots, o) });
        o
    }
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
    unsafe fn call(&self, r: Oop, name: &[u8], args: &[Oop]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                r,
                name,
                &args
                    .iter()
                    .copied()
                    .map(Operand::object)
                    .collect::<Vec<_>>(),
            )))
        }
    }
    unsafe fn integer(&self, value: usize) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                self.named(b"Integer"),
                b"fromWord:",
                &[Operand::word(value)],
            )))
        }
    }
    unsafe fn number(&self, value: Oop) -> usize {
        unsafe {
            send(
                self.id,
                self.named(b"Integer"),
                b"wordOf:",
                &[Operand::object(value)],
            )
            .as_word()
            .unwrap()
        }
    }
    unsafe fn publish(&self, name: &[u8], value: Oop) {
        unsafe {
            send(
                self.id,
                self.ns,
                b"at:put:",
                &[
                    Operand::object(self.id.intern_name(name).unwrap()),
                    Operand::object(value),
                ],
            );
        }
    }
    unsafe fn clear_from(&self, floor: usize) {
        for token in self.tokens.borrow_mut().drain(floor..).rev() {
            unsafe {
                release(self.id, self.roots, token);
            }
        }
    }
}

impl H {
    unsafe fn program(&self, index: usize) -> Oop {
        unsafe {
            let root = PathBuf::from(std::env::var_os("BIRNAM_MUTATION_PROGRAMS").unwrap());
            let bytes = std::fs::read(root.join(format!("{index}.bin"))).unwrap();
            let bytes = self.keep(object(send(
                self.id,
                self.named(b"Bytes"),
                b"fromBytes:",
                &[Operand::bytes(&bytes)],
            )));
            let code = self.call(self.named(b"CodeObject"), b"fromCanonicalBytes:", &[bytes]);
            let captures = self.call(self.named(b"Vector"), b"new", &[]);
            self.call(
                self.named(b"Codegen"),
                b"materializeResumable:captures:",
                &[code, captures],
            )
        }
    }
    unsafe fn template(&self, arity: usize) -> Oop {
        unsafe {
            let source = self.call(
                self.named(b"NativeBackend"),
                b"named:synchronous:",
                &[self.integer(arity), self.named(b"false")],
            );
            let unit = self.call(self.named(b"TCC"), b"compile:", &[source]);
            self.call(unit, b"initializeWith:", &[self.named(b"nil")])
        }
    }
    unsafe fn epoch(&self) -> usize {
        unsafe {
            send(self.id, self.id.receiver, b"bindingEpoch", &[])
                .as_word()
                .unwrap()
        }
    }
}
struct Switch {
    id: Protocol,
    gc: Oop,
    vtable: Oop,
    selector: Oop,
    replacement: Oop,
    entered: Oop,
    body: Oop,
    calls: Cell<usize>,
}
unsafe extern "C" fn switch(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Switch>();
        p.calls.set(p.calls.get() + 1);
        send(
            p.id,
            p.vtable,
            b"addMethod:as:",
            &[Operand::object(p.replacement), Operand::object(p.selector)],
        );
        collect(p.id, p.gc);
        assert!(
            present(p.id, p.entered) && present(p.id, p.body),
            "entered Method and implementation must survive replacement and GC"
        );
        Outcome::loaded(Operand::unit())
    }
}
unsafe extern "C" fn constant(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::object(c.cast()))
}
#[test]
#[ignore = "scripts/minimal-mutation-test.py supplies an isolated floor and exported programs"]
fn direct_mutation_needs_no_store_generation_or_publication_owner() {
    unsafe {
        for module in ["store", "relational", "generation", "bootstrap"] {
            assert!(
                !directory()
                    .join(format!("libbirnam_{module}.dylib"))
                    .exists()
            );
        }
        let id = id();
        let roots = load(id, "root_registry", &[]);
        let ns = load(id, "namespace", &[Operand::object(roots)]);
        let h = H {
            id,
            ns,
            roots,
            tokens: RefCell::new(Vec::new()),
        };
        let prototype = object(send(id, id.receiver, b"objectPrototype", &[]));
        let vt = object(send(
            id,
            id.receiver,
            b"behaviorOf:",
            &[Operand::object(prototype)],
        ));
        h.publish(b"Object", prototype);
        h.publish(b"ObjectVt", vt);
        for module in ["boolean", "nil"] {
            mount(id, module, ns);
        }
        let gc = load(id, "collect", &[Operand::object(roots)]);
        h.publish(b"GC", gc);
        send(id, gc, b"activate", &[]);
        for module in [
            "object_model",
            "word",
            "bytes",
            "execution",
            "portable_code",
            "string",
            "integer",
        ] {
            mount(id, module, ns);
        }
        let collection = h.keep(object(send(id, vt, b"delegated", &[])));
        h.publish(b"Collection", collection);
        h.publish(b"CollectionVt", collection);
        let symbol = id.intern_name(b"Symbol").unwrap();
        h.publish(
            b"SymbolVt",
            object(send(
                id,
                id.receiver,
                b"behaviorOf:",
                &[Operand::object(symbol)],
            )),
        );
        for module in ["vector", "map", "tcc", "native_backend", "codegen_runtime"] {
            mount(id, module, ns);
        }
        // Namespace is a real binding owner, not a fake database. Only the compiler's
        // existing Globals-first read convention needs this alias in the small floor.
        h.publish(b"Globals", ns);

        for name in [
            b"Store".as_slice(),
            b"db",
            b"Relational",
            b"Generation",
            b"MethodPublication",
            b"Lang",
        ] {
            assert!(
                send(
                    id,
                    ns,
                    b"at:",
                    &[Operand::object(id.intern_name(name).unwrap())]
                )
                .as_object()
                .is_none()
            );
        }
        // No observer is needed for the floor's own dictionary invalidation.
        assert_eq!(
            send(id, id.receiver, b"methodChangesTo:", &[Operand::unit()]).kind(),
            birnam_abi::id_protocol::UNIT
        );
        let nil = h.named(b"nil");
        let selector = h.keep(id.intern_name(b"answer").unwrap());
        let template = h.template(0);
        let one = h.program(0);
        let two = h.program(1);
        let epoch = h.epoch();
        let first = h.call(template, b"withImplementation:", &[one]);
        let second = h.call(template, b"withImplementation:", &[two]);
        assert_ne!(first, second);
        assert_eq!(
            h.epoch(),
            epoch,
            "pure binding construction changes no live dictionary"
        );
        let parent = h.call(prototype, b"clone", &[]);
        let parent_vt = h.call(id.receiver, b"ensurePrivateVtableFor:", &[parent]);
        assert_eq!(
            h.call(parent_vt, b"addMethod:as:", &[first, selector]),
            first
        );
        let child = h.call(parent, b"clone", &[]);
        let child_vt = h.call(id.receiver, b"ensurePrivateVtableFor:", &[child]);
        assert_eq!(h.number(h.call(child, b"answer", &[])), 11);
        assert_eq!(
            send(
                id,
                id.receiver,
                b"bindingOf:in:",
                &[Operand::object(selector), Operand::object(child_vt)]
            )
            .kind(),
            birnam_abi::id_protocol::UNIT
        );
        h.call(child_vt, b"addMethod:as:", &[second, selector]);
        for _ in 0..32 {
            assert_eq!(h.number(h.call(child, b"answer", &[])), 22);
        }
        assert_eq!(h.number(h.call(parent, b"answer", &[])), 11);
        let captured = h.call(child, b"methodFor:", &[selector]);
        let before = h.epoch();
        assert!(
            send(id, child_vt, b"removeMethod:", &[Operand::object(selector)])
                .as_entry()
                .is_some()
        );
        assert!(h.epoch() > before);
        assert_eq!(h.number(h.call(child, b"answer", &[])), 11);
        let args = h.call(h.named(b"Vector"), b"new", &[]);
        assert_eq!(
            h.number(h.call(captured, b"invokeOn:arguments:", &[child, args])),
            22
        );
        let before = h.epoch();
        assert_eq!(
            send(id, child_vt, b"removeMethod:", &[Operand::object(selector)]).kind(),
            birnam_abi::id_protocol::UNIT
        );
        assert!(h.epoch() >= before); // existing epochs conservatively include removal attempts
        h.call(child_vt, b"addMethod:as:", &[second, selector]);
        let snapshot = h.epoch();
        h.call(child_vt, b"addMethod:as:", &[first, selector]);
        h.call(child_vt, b"addMethod:as:", &[second, selector]);
        assert!(
            h.epoch() > snapshot,
            "an explicit prepared operation can detect ABA without an authorizer"
        );
        // A retained outer Method still sends through its implementation's current value:.
        let implementation_vt = h.call(id.receiver, b"ensurePrivateVtableFor:", &[one]);
        let value = h.integer(33);
        install(id, implementation_vt, b"value:", constant, value.cast());
        assert_eq!(
            h.number(h.call(first, b"invokeOn:arguments:", &[child, args])),
            33
        );
        send(
            id,
            implementation_vt,
            b"removeMethod:",
            &[Operand::object(id.intern_name(b"value:").unwrap())],
        );
        assert_eq!(
            h.number(h.call(first, b"invokeOn:arguments:", &[child, args])),
            11
        );
        // Keep only the dictionary edge to this Method, then replace it from inside
        // its own body. Native context fields deliberately are not collector roots.
        let mark = h.tokens.borrow().len();
        let body = h.program(2);
        let entered = h.call(template, b"withImplementation:", &[body]);
        h.call(child_vt, b"addMethod:as:", &[entered, selector]);
        h.clear_from(mark);
        let mut state = Box::new(Switch {
            id,
            gc,
            vtable: child_vt,
            selector,
            replacement: second,
            entered,
            body,
            calls: Cell::new(0),
        });
        install(
            id,
            child_vt,
            b"replaceNow",
            switch,
            (&mut *state as *mut Switch).cast(),
        );
        assert_eq!(h.number(h.call(child, b"answer", &[])), 11);
        assert_eq!(state.calls.get(), 1);
        assert_eq!(h.number(h.call(child, b"answer", &[])), 22);
        collect(id, gc);
        assert!(
            !present(id, entered) && !present(id, body),
            "retired binding and body must collect after the entered call ends"
        );
        let unary = h.template(1);
        let body = h.program(3);
        let method = h.call(unary, b"withImplementation:", &[body]);
        let argument = h.call(prototype, b"clone", &[]);
        let unary_selector = h.keep(id.intern_name(b"echo:").unwrap());
        h.call(child_vt, b"addMethod:as:", &[method, unary_selector]);
        assert_eq!(h.call(child, b"echo:", &[argument]), argument);
        // No managed notification registration or hidden namespace owner appeared.
        for name in [
            b"Store".as_slice(),
            b"db",
            b"Relational",
            b"Generation",
            b"MethodPublication",
        ] {
            assert!(
                send(
                    id,
                    ns,
                    b"at:",
                    &[Operand::object(id.intern_name(name).unwrap())]
                )
                .as_object()
                .is_none()
            );
        }
        let _ = nil;
        h.clear_from(0);
        eprintln!(
            "minimal-mutation: direct install/remove, inheritance, warmed lookup, captured/entered Methods, live implementation sends, ABA and GC passed without Store/Relational/Generation"
        );
    }
}
