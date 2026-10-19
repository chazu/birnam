#![allow(dead_code)]
use birnam_abi::{
    id_protocol::{Method, NativeEntry, Operand, Protocol},
    Oop,
};
use core::ffi::c_void;
use std::{os::unix::ffi::OsStrExt, path::PathBuf};
pub fn directory() -> PathBuf {
    PathBuf::from(
        std::env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").expect("production module directory"),
    )
}
pub unsafe fn id() -> Protocol {
    birnam_siphon::load(
        directory()
            .join("libbirnam_id.dylib")
            .as_os_str()
            .as_bytes(),
    )
    .unwrap()
}
pub unsafe fn send(id: Protocol, receiver: Oop, name: &[u8], args: &[Operand]) -> Operand {
    unsafe { id.send_named(receiver, name, args) }
        .unwrap_or_else(|e| panic!("{}: {e:?}", String::from_utf8_lossy(name)))
}
pub fn object(value: Operand) -> Oop {
    value.as_object().unwrap()
}
pub unsafe fn load(id: Protocol, name: &str, args: &[Operand]) -> Oop {
    let path = directory().join(format!("libbirnam_{name}.dylib"));
    let mut request = vec![Operand::bytes(path.as_os_str().as_bytes())];
    request.extend_from_slice(args);
    object(unsafe { id.send(id.receiver, id.load, &request) }.unwrap())
}
pub unsafe fn behavior(id: Protocol) -> Oop {
    object(unsafe { send(id, id.receiver, b"delegated", &[]) })
}
pub unsafe fn allocate(id: Protocol, behavior: Oop, bytes: usize) -> Oop {
    object(unsafe { send(id, behavior, b"allocate", &[Operand::word(bytes)]) })
}
pub unsafe fn install(
    id: Protocol,
    behavior: Oop,
    name: &[u8],
    method: Method,
    context: *mut c_void,
) {
    unsafe {
        send(
            id,
            behavior,
            b"addMethod",
            &[
                Operand::object(id.intern_name(name).unwrap()),
                Operand::entry(NativeEntry::new(method, context)),
            ],
        );
    }
}
pub unsafe fn retain(id: Protocol, roots: Oop, value: Oop) -> usize {
    unsafe { send(id, roots, b"retain:", &[Operand::object(value)]) }
        .as_word()
        .unwrap()
}
pub unsafe fn release(id: Protocol, roots: Oop, token: usize) {
    assert_eq!(
        unsafe { send(id, roots, b"release:", &[Operand::word(token)]) }.as_word(),
        Some(1)
    );
}
pub unsafe fn present(id: Protocol, value: Oop) -> bool {
    unsafe {
        send(
            id,
            id.receiver,
            b"allocationAt:",
            &[Operand::word(value.addr())],
        )
    }
    .as_object()
        == Some(value)
}
pub unsafe fn collect(id: Protocol, gc: Oop) -> usize {
    unsafe { send(id, gc, b"collectPrecisely", &[]) }
        .as_word()
        .unwrap()
}

// Install a test receiver method for one synchronous inventory. Id is already
// rooted; this adds no allocations to physical-accounting measurements.
pub unsafe fn inventory(id: Protocol, method: Method, context: *mut c_void) {
    unsafe {
        install(
            id,
            id.receiver,
            b"allocation:bytes:identity:",
            method,
            context,
        );
        send(
            id,
            id.receiver,
            b"allocationsDo:",
            &[Operand::object(id.receiver)],
        );
        send(
            id,
            id.receiver,
            b"removeMethod:",
            &[Operand::object(
                id.intern_name(b"allocation:bytes:identity:").unwrap(),
            )],
        );
    }
}
pub unsafe fn visitor(id: Protocol, method: Method, context: *mut c_void) -> Operand {
    unsafe {
        let vt = behavior(id);
        install(id, vt, b"visit:", method, context);
        Operand::object(allocate(id, vt, 0))
    }
}

/// Pin the compiled core while testing temporary code lifetimes. Image tests
/// restore its original Lang bindings before checking that fixture units retire.
pub struct CompiledFloor {
    pub units: usize,
    pub bytes: usize,
    id: Protocol,
    roots: Oop,
    tokens: Vec<usize>,
    lang_bindings: Vec<(Oop, Oop, NativeEntry)>,
}
impl CompiledFloor {
    pub unsafe fn pin(id: Protocol, ns: Oop, roots: Oop) -> Self {
        unsafe { Self::pin_with_module(id, ns, roots, true) }
    }
    pub unsafe fn pin_lang_only(id: Protocol, ns: Oop, roots: Oop) -> Self {
        unsafe {
            let mut floor = Self::pin_with_module(id, ns, roots, false);
            let named = |name: &[u8]| {
                object(send(
                    id,
                    ns,
                    b"at:",
                    &[Operand::object(id.intern_name(name).unwrap())],
                ))
            };
            // Remove this test process's original manager methods to measure
            // Lang's exact unit baseline. Reloading the compiled manager below
            // restores the normal floor without retaining an obsolete factory.
            let module = object(send(
                id,
                named(b"Globals"),
                b"at:",
                &[Operand::object(id.intern_name(b"Module").unwrap())],
            ));
            let vt = object(send(
                id,
                id.receiver,
                b"behaviorOf:",
                &[Operand::object(module)],
            ));
            let mut selectors = Vec::new();
            for index in 0..1024 {
                let value = send(
                    id,
                    id.receiver,
                    b"selectorAt:in:",
                    &[Operand::word(index), Operand::object(vt)],
                );
                let Some(selector) = value.as_object() else {
                    break;
                };
                selectors.push(selector);
            }
            for selector in selectors {
                send(
                    id,
                    named(b"Store"),
                    b"removeMethod:on:",
                    &[Operand::object(selector), Operand::object(vt)],
                );
            }
            collect(id, named(b"GC"));
            send(id, named(b"TCC"), b"checkpoint", &[]);
            floor.units = send(id, named(b"TCC"), b"liveNativeUnits", &[])
                .as_word()
                .unwrap();
            floor.bytes = send(id, named(b"TCC"), b"liveNativeBytes", &[])
                .as_word()
                .unwrap();
            let path = object(send(
                id,
                named(b"String"),
                b"fromBytes:",
                &[Operand::bytes(
                    directory()
                        .join("libbirnam_modules.dylib")
                        .as_os_str()
                        .as_bytes(),
                )],
            ));
            let token = retain(id, roots, path);
            let result = object(send(
                id,
                named(b"Codegen"),
                b"load:",
                &[Operand::object(path)],
            ));
            assert_ne!(result, named(b"nil"));
            release(id, roots, token);
            floor
        }
    }
    unsafe fn pin_with_module(id: Protocol, ns: Oop, roots: Oop, pin_module: bool) -> Self {
        unsafe {
            let named = |name: &[u8]| {
                object(send(
                    id,
                    ns,
                    b"at:",
                    &[Operand::object(id.intern_name(name).unwrap())],
                ))
            };
            let mut tokens = Vec::new();
            let globals = named(b"Globals");
            let module = object(send(
                id,
                globals,
                b"at:",
                &[Operand::object(id.intern_name(b"Module").unwrap())],
            ));
            if pin_module {
                tokens.push(retain(id, roots, module));
            }
            let lang = named(b"Lang");
            let vt = object(send(
                id,
                id.receiver,
                b"behaviorOf:",
                &[Operand::object(lang)],
            ));
            tokens.push(retain(id, roots, vt));
            let mut lang_bindings = Vec::new();
            for name in [
                b"buildGrammar".as_slice(),
                b"initializeGrammar",
                b"grammar",
                b"parse:",
                b"furthestFail",
            ] {
                let selector = id.intern_name(name).unwrap();
                let entry = send(
                    id,
                    id.receiver,
                    b"bindingOf:in:",
                    &[Operand::object(selector), Operand::object(vt)],
                )
                .as_entry()
                .unwrap();
                let method = object(send(
                    id,
                    named(b"Execution"),
                    b"methodOfEntry:",
                    &[Operand::entry(entry)],
                ));
                tokens.push(retain(id, roots, method));
                lang_bindings.push((vt, selector, entry));
            }
            collect(id, named(b"GC"));
            send(id, named(b"TCC"), b"checkpoint", &[]);
            let units = send(id, named(b"TCC"), b"liveNativeUnits", &[])
                .as_word()
                .unwrap();
            let bytes = send(id, named(b"TCC"), b"liveNativeBytes", &[])
                .as_word()
                .unwrap();
            assert!(units > 0, "compiled Lang was not exercised");
            Self {
                units,
                bytes,
                id,
                roots,
                tokens,
                lang_bindings,
            }
        }
    }
    pub unsafe fn restore_lang(&self) {
        unsafe {
            for &(vt, selector, entry) in &self.lang_bindings {
                send(
                    self.id,
                    vt,
                    b"addMethod",
                    &[Operand::object(selector), Operand::entry(entry)],
                );
            }
        }
    }
}
impl Drop for CompiledFloor {
    fn drop(&mut self) {
        for token in self.tokens.drain(..) {
            unsafe {
                release(self.id, self.roots, token);
            }
        }
    }
}

// Tests that intentionally discard a complete world must clear source bindings
// through their owner. Clearing Store no longer implicitly clears Globals.
pub unsafe fn clear_source_bindings(id: Protocol, ns: Oop) {
    unsafe {
        let named = |name: &[u8]| object(send(id, ns, b"at:", &[Operand::object(id.intern_name(name).unwrap())]));
        let roots = named(b"Roots"); let globals = named(b"Globals"); let integer = named(b"Integer");
        // Restore resident authored policy only at teardown. Pinning its old
        // captured state across image loads would keep an entire obsolete world
        // alive and serialize it again on every resave.
        let path = object(send(id, named(b"String"), b"fromBytes:", &[Operand::bytes(
            directory().join("libbirnam_globalsOwner.dylib").as_os_str().as_bytes()
        )]));
        let path_token = retain(id, roots, path);
        let result = object(send(id, named(b"Codegen"), b"load:", &[Operand::object(path)]));
        assert_ne!(result, named(b"nil"));
        release(id, roots, path_token);
        let names = object(send(id, globals, b"bindingNames", &[]));
        let token = retain(id, roots, names);
        let count = object(send(id, names, b"size", &[]));
        let count = send(id, integer, b"wordOf:", &[Operand::object(count)]).as_word().unwrap();
        for index in 0..count {
            let index = object(send(id, integer, b"fromWord:", &[Operand::word(index)]));
            let held = retain(id, roots, index);
            let name = object(send(id, names, b"at:", &[Operand::object(index)]));
            assert_eq!(object(send(id, globals, b"remove:", &[Operand::object(name)])), globals);
            release(id, roots, held);
        }
        release(id, roots, token);
    }
}

/// Load the actual authored record protocols for native Generation tests. These
/// sources are evaluated without registering extra compilation records, so each
/// test still controls its source projection explicitly.
pub unsafe fn lifecycle_records(id: Protocol, ns: Oop) {
    unsafe {
        let named = |name: &[u8]| object(send(id, ns, b"at:", &[Operand::object(id.intern_name(name).unwrap())]));
        for bytes in [include_bytes!("../../../../modules/stdlib.birnam").as_slice(), include_bytes!("../../../../modules/candidate-policy.birnam").as_slice(), include_bytes!("../../../../modules/compiled-unit.birnam").as_slice()] {
            let source = object(send(id, named(b"String"), b"fromBytes:", &[Operand::bytes(bytes)]));
            let token = retain(id, named(b"Roots"), source);
            let result = object(send(id, named(b"Codegen"), b"evalPersisting:", &[Operand::object(source)]));
            let kept = retain(id, named(b"Roots"), result);
            assert_eq!(send(id, result, b"isError", &[]).as_object(), Some(named(b"false")));
            release(id, named(b"Roots"), kept);
            release(id, named(b"Roots"), token);
        }
    }
}
