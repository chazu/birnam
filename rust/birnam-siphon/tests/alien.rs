//! Production Alien calls, buffers, callbacks and reclamation.
mod support;
use birnam_abi::{
    Oop, borrowed_bytes,
    id_protocol::{Operand, Outcome, Protocol},
};

use core::{cell::RefCell, ffi::c_void};
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
    unsafe fn bind(&self, name: &[u8], o: Oop) {
        unsafe {
            send(
                self.id,
                self.ns,
                b"at:put:",
                &[
                    Operand::object(self.id.intern_name(name).unwrap()),
                    Operand::object(o),
                ],
            );
        }
    }
    unsafe fn bytes(&self, o: Oop) -> Vec<u8> {
        unsafe {
            let b = send(self.id, o, b"bytes", &[]).as_bytes().unwrap();
            borrowed_bytes(b.pointer, b.length).unwrap().to_vec()
        }
    }
}

struct Forward {
    id: Protocol,
    gc: Oop,
    alias: Oop,
}
unsafe extern "C" fn collecting(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let f = &*c.cast::<Forward>();
        collect(f.id, f.gc);
        f.id.send(r, f.alias, birnam_abi::borrowed_slice(a, n).unwrap())
            .map_or_else(|e| e, Outcome::loaded)
    }
}
impl H {
    unsafe fn int(&self, w: usize) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                self.named(b"Integer"),
                b"fromWord:",
                &[Operand::word(w)],
            )))
        }
    }
    unsafe fn eval(&self, b: &[u8]) -> Oop {
        unsafe { self.call(self.named(b"Codegen"), b"eval:", &[self.string(b)]) }
    }
    unsafe fn compiler_call(&self, selector: &[u8], args: &[Oop]) -> Oop {
        unsafe {
            // Collect during FFI marshalling, not every allocation in the authored compiler.
            // Compiler collection safety is covered by the native_compilers host seam test.
            let integer_vt = self.named(b"IntegerVt");
            let factory_selector = self.id.intern_name(b"fromWord:").unwrap();
            let original_selector = self.id.intern_name(b"alienTestFromWord:").unwrap();
            let collecting_entry = send(
                self.id,
                self.id.receiver,
                b"bindingOf:in:",
                &[
                    Operand::object(factory_selector),
                    Operand::object(integer_vt),
                ],
            )
            .as_entry()
            .unwrap();
            let plain_entry = send(
                self.id,
                self.id.receiver,
                b"bindingOf:in:",
                &[
                    Operand::object(original_selector),
                    Operand::object(integer_vt),
                ],
            )
            .as_entry()
            .unwrap();
            send(
                self.id,
                integer_vt,
                b"addMethod",
                &[
                    Operand::object(factory_selector),
                    Operand::entry(plain_entry),
                ],
            );
            let result = self.call(self.named(b"Alien"), selector, args);
            send(
                self.id,
                integer_vt,
                b"addMethod",
                &[
                    Operand::object(factory_selector),
                    Operand::entry(collecting_entry),
                ],
            );
            result
        }
    }
    unsafe fn wrap(&self, name: &[u8], sig: &[u8], selector: &[u8], target: Oop) {
        unsafe {
            let r = self.compiler_call(
                b"wrap:signature:as:on:",
                &[
                    self.string(name),
                    self.string(sig),
                    self.id.intern_name(selector).unwrap(),
                    target,
                ],
            );
            if r != target {
                panic!(
                    "wrap {} failed: {:?}",
                    String::from_utf8_lossy(name),
                    self.bytes(self.call(r, b"message", &[]))
                )
            }
        }
    }
}
#[test]
#[ignore = "requires production modules"]
fn alien_calls_callbacks_and_owned_memory() {
    unsafe {
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        optional_modules(id, ns, &["store"]);
        let named = |n: &[u8]| {
            object(send(
                id,
                ns,
                b"at:",
                &[Operand::object(id.intern_name(n).unwrap())],
            ))
        };
        let h = H {
            id,
            ns,
            roots: named(b"Roots"),
            gc: named(b"GC"),
            tokens: RefCell::new(Vec::new()),
        };
        let floor = CompiledFloor::pin(id, ns, h.roots);
        let integer_vt = named(b"IntegerVt");
        let old = send(
            id,
            id.receiver,
            b"bindingOf:in:",
            &[
                Operand::object(id.intern_name(b"fromWord:").unwrap()),
                Operand::object(integer_vt),
            ],
        )
        .as_entry()
        .unwrap();
        let alias = id.intern_name(b"alienTestFromWord:").unwrap();
        send(
            id,
            integer_vt,
            b"addMethod",
            &[Operand::object(alias), Operand::entry(old)],
        );
        let mut f = Forward {
            id,
            gc: h.gc,
            alias,
        };
        install(
            id,
            integer_vt,
            b"fromWord:",
            collecting,
            (&mut f as *mut Forward).cast(),
        );
        let alien = h.named(b"Alien");
        let nil = h.named(b"nil");
        let integer = h.named(b"Integer");
        let fixture = h.string(
            directory()
                .join("libbirnam_alien_fixture.dylib")
                .as_os_str()
                .as_bytes(),
        );
        assert_eq!(h.call(alien, b"dlopen:", &[fixture]), h.named(b"true"));
        h.wrap(b"strlen", b"l s", b"foreignLength", h.named(b"String"));
        assert_eq!(
            h.number(h.call(h.string(b"hello"), b"foreignLength", &[])),
            5
        );
        h.wrap(b"alien_add", b"l l l", b"foreignAdd:", integer);
        assert_eq!(
            h.number(h.call(h.int(17), b"foreignAdd:", &[h.int(25)])),
            42
        );
        h.wrap(b"alien_pair", b"{ll} l", b"foreignPair", integer);
        let pair = h.call(h.int(20), b"foreignPair", &[]);
        assert_eq!(h.number(h.call(pair, b"at:", &[h.int(1)])), 21);
        h.wrap(b"alien_sum", b"l {ll}", b"foreignSum", h.named(b"Vector"));
        assert_eq!(h.number(h.call(pair, b"foreignSum", &[])), 41);
        h.wrap(
            b"alien_unsigned",
            b"u u",
            b"foreignUnsigned",
            h.named(b"Word"),
        );
        let max = h.keep(object(send(
            id,
            h.named(b"Word"),
            b"fromBits:",
            &[Operand::word(usize::MAX)],
        )));
        let max2 = h.call(max, b"foreignUnsigned", &[]);
        assert_eq!(send(id, max2, b"bits", &[]).as_word(), Some(usize::MAX));
        h.wrap(
            b"alien_unsigned_pair",
            b"{u} u",
            b"foreignUnsignedPair",
            h.named(b"Word"),
        );
        let pair = h.call(max, b"foreignUnsignedPair", &[]);
        let value = h.call(pair, b"at:", &[h.int(0)]);
        assert_eq!(send(id, value, b"bits", &[]).as_word(), Some(usize::MAX));
        h.wrap(b"alien_double", b"d d", b"foreignDouble", h.named(b"Float"));
        let input = h.keep(object(send(
            id,
            h.named(b"Float"),
            b"fromBits:",
            &[Operand::word(1.25f64.to_bits() as usize)],
        )));
        let result = h.call(input, b"foreignDouble", &[]);
        assert_eq!(
            send(
                id,
                h.named(b"Float"),
                b"bitsOf:",
                &[Operand::object(result)]
            )
            .as_word(),
            Some(2.5f64.to_bits() as usize)
        );
        let memory = h.call(alien, b"alloc:", &[h.int(32)]);
        assert_eq!(h.number(h.call(memory, b"peekIntAt:", &[h.int(1)])), 0);
        h.call(memory, b"pokeInt:at:", &[h.int(1234), h.int(1)]);
        assert_eq!(h.number(h.call(memory, b"peekIntAt:", &[h.int(1)])), 1234);
        assert_eq!(h.call(memory, b"free", &[]), nil);
        assert_eq!(h.call(memory, b"isNull", &[]), h.named(b"true"));
        assert_eq!(
            h.call(h.call(memory, b"free", &[]), b"kind", &[]),
            id.intern_name(b"alreadyFreed").unwrap()
        );
        h.wrap(b"alien_call", b"l l p", b"foreignCall:", integer);
        let block = h.eval(b"[:x | [GC collectPrecisely]; [x + 1]]");
        assert_eq!(h.number(h.call(block, b"value:", &[h.int(4)])), 5);
        let cb = h.compiler_call(b"callback:signature:", &[block, h.string(b"l l")]);
        assert_eq!(h.number(h.call(h.int(41), b"foreignCall:", &[cb])), 42);
        collect(id, h.gc);
        assert_eq!(h.number(h.call(h.int(9), b"foreignCall:", &[cb])), 10);
        h.bind(b"AlienTestCallback", cb);
        let recursive=h.eval(b"[:x | [[x = 0] ifTrue: [| 10] ifFalse: [| [[[x - 1] foreignCall: AlienTestRecursive] + 1]]]]");
        let cb2 = h.compiler_call(b"callback:signature:", &[recursive, h.string(b"l l")]);
        h.bind(b"AlienTestRecursive", cb2);
        assert_eq!(h.number(h.call(h.int(3), b"foreignCall:", &[cb2])), 13);
        h.call(alien, b"releaseCallback:", &[cb2]);
        let releasing=h.eval(b"[:x | [Alien releaseCallback: AlienRelease]; [GC collectPrecisely]; [TCC checkpoint]; [x + 2]]");
        let cb3 = h.compiler_call(b"callback:signature:", &[releasing, h.string(b"l l")]);
        h.bind(b"AlienRelease", cb3);
        assert_eq!(h.number(h.call(h.int(40), b"foreignCall:", &[cb3])), 42);
        assert_eq!(h.call(cb3, b"isNull", &[]), h.named(b"true"));
        h.bind(b"AlienRelease", nil);
        h.bind(b"AlienTestCallback", nil);
        h.bind(b"AlienTestRecursive", nil);
        assert_eq!(h.call(alien, b"releaseCallback:", &[cb]), nil);
        assert_eq!(h.call(cb, b"isNull", &[]), h.named(b"true"));
        assert_eq!(
            h.call(h.call(alien, b"dlclose:", &[fixture]), b"kind", &[]),
            id.intern_name(b"inUse").unwrap()
        );
        // Remove wrappers and temporary roots. Only compiled core units may remain.
        for (target, sel) in [
            (h.named(b"String"), b"foreignLength".as_slice()),
            (integer, b"foreignAdd:"),
            (integer, b"foreignPair"),
            (h.named(b"Vector"), b"foreignSum"),
            (h.named(b"Word"), b"foreignUnsigned"),
            (h.named(b"Word"), b"foreignUnsignedPair"),
            (h.named(b"Float"), b"foreignDouble"),
            (integer, b"foreignCall:"),
        ] {
            let vt = object(send(
                id,
                id.receiver,
                b"behaviorOf:",
                &[Operand::object(target)],
            ));
            h.call(
                h.named(b"Store"),
                b"removeMethod:on:",
                &[id.intern_name(sel).unwrap(), vt],
            );
        }
        send(
            id,
            integer_vt,
            b"addMethod",
            &[
                Operand::object(id.intern_name(b"fromWord:").unwrap()),
                Operand::entry(old),
            ],
        );
        send(id, integer_vt, b"removeMethod", &[Operand::object(alias)]);
        for t in h.tokens.borrow_mut().drain(..) {
            release(id, h.roots, t)
        }
        collect(id, h.gc);
        send(id, h.named(b"TCC"), b"checkpoint", &[]);
        collect(id, h.gc);
        send(id, h.named(b"TCC"), b"checkpoint", &[]);
        assert_eq!(
            send(id, h.named(b"TCC"), b"liveNativeUnits", &[]).as_word(),
            Some(floor.units)
        );
        let fixture = h.string(
            directory()
                .join("libbirnam_alien_fixture.dylib")
                .as_os_str()
                .as_bytes(),
        );
        assert_eq!(h.call(alien, b"dlclose:", &[fixture]), h.named(b"true"));
    }
}
