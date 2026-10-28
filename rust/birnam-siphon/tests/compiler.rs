//! Retained compiler acceptance groups; Oop migration remains pending.
//! General source/control/lifetime acceptance lives in compiler_source.rs.
#[path = "compiler/inline_return.rs"]
mod inline_return;
#[path = "compiler/numeric_predicate.rs"]
mod numeric_predicate;
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use core::{ffi::c_void, ptr};
use std::{cell::Cell, os::unix::ffi::OsStrExt};
use support::*;
struct H {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    tokens: Vec<usize>,
    floor: CompiledFloor,
}
impl H {
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
    unsafe fn keep(&mut self, o: Oop) -> Oop {
        self.tokens.push(unsafe { retain(self.id, self.roots, o) });
        o
    }
    unsafe fn string(&mut self, b: &[u8]) -> Oop {
        let v = unsafe {
            object(send(
                self.id,
                self.named(b"String"),
                b"fromBytes:",
                &[Operand::bytes(b)],
            ))
        };
        unsafe { self.keep(v) }
    }
    unsafe fn eval(&mut self, b: &[u8]) -> Oop {
        unsafe {
            let text = self.string(b);
            let value = object(
                self.id
                    .send_named(self.named(b"Codegen"), b"eval:", &[Operand::object(text)])
                    .unwrap_or_else(|e| panic!("eval {}: {e:?}", String::from_utf8_lossy(b))),
            );
            self.keep(value)
        }
    }
    unsafe fn clear(&mut self) {
        for t in self.tokens.drain(..) {
            unsafe {
                release(self.id, self.roots, t);
            }
        }
    }
    unsafe fn collect(&self) {
        unsafe {
            collect(self.id, self.named(b"GC"));
            send(self.id, self.named(b"TCC"), b"checkpoint", &[]);
        }
    }
    unsafe fn native_bytes(&self) -> usize {
        unsafe {
            send(self.id, self.named(b"TCC"), b"liveNativeBytes", &[])
                .as_word()
                .unwrap()
                .checked_sub(self.floor.bytes)
                .expect("compiled core bytes remain live")
        }
    }
    unsafe fn units(&self) -> usize {
        unsafe {
            send(self.id, self.named(b"TCC"), b"liveNativeUnits", &[])
                .as_word()
                .unwrap()
                .checked_sub(self.floor.units)
                .expect("compiled core units remain live")
        }
    }
    unsafe fn bind(&self, n: &[u8], o: Oop) {
        unsafe {
            send(
                self.id,
                self.ns,
                b"at:put:",
                &[
                    Operand::object(self.id.intern_name(n).unwrap()),
                    Operand::object(o),
                ],
            );
        }
    }
    unsafe fn publish(&self, n: &[u8], o: Oop) {
        unsafe {
            send(
                self.id,
                self.named(b"Globals"),
                b"at:put:",
                &[
                    Operand::object(self.id.intern_name(n).unwrap()),
                    Operand::object(o),
                ],
            );
        }
    }
    unsafe fn compile(&mut self, b: &[u8]) -> Oop {
        unsafe {
            let text = self.string(b);
            let v = object(send(
                self.id,
                self.named(b"Codegen"),
                b"compile:",
                &[Operand::object(text)],
            ));
            self.keep(v)
        }
    }
    unsafe fn block(&mut self, method: Oop) -> Oop {
        unsafe {
            let captures = object(send(self.id, self.named(b"Vector"), b"new", &[]));
            self.keep(captures);
            let block = object(send(
                self.id,
                self.named(b"Block"),
                b"fromMethod:captures:",
                &[Operand::object(method), Operand::object(captures)],
            ));
            self.keep(block)
        }
    }
    unsafe fn integer(&self, o: Oop) -> i64 {
        unsafe {
            send(
                self.id,
                self.named(b"Integer"),
                b"wordOf:",
                &[Operand::object(o)],
            )
            .as_word()
            .unwrap_or_else(|| panic!("not Integer: {:?}", send(self.id, o, b"kind", &[])))
                as i64
        }
    }
}
#[test]
#[ignore = "requires native prefix"]
fn compiler_optimizations_preserve_current_language_behavior() {
    unsafe {
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        optional_modules(id, ns, &["store", "relational"]);
        let roots = object(send(
            id,
            ns,
            b"at:",
            &[Operand::object(id.intern_name(b"Roots").unwrap())],
        ));
        let mut h = H {
            id,
            ns,
            roots,
            tokens: vec![],
            floor: CompiledFloor::pin(id, ns, roots),
        };
        numeric_predicate::check(&mut h);
        inline_return::check(&mut h);
        h.clear();
        h.collect();
        assert_eq!(h.units(), 0);
        assert_eq!(
            h.native_bytes(),
            0,
            "actual libtcc relocated bytes reclaimed"
        );
    }
}

// Shared helpers for the two remaining legacy compiler groups.
unsafe fn lookup(h: &H, r: Oop, name: &[u8]) -> NativeEntry {
    unsafe {
        send(
            h.id,
            h.id.receiver,
            b"bind:to:",
            &[
                Operand::object(h.id.intern_name(name).unwrap()),
                Operand::object(r),
            ],
        )
        .as_entry()
        .unwrap()
    }
}
unsafe fn restore(h: &H, vt: Oop, name: &[u8], entry: NativeEntry) {
    unsafe { install(h.id, vt, name, entry.method.unwrap(), entry.context) }
}
unsafe fn vt(h: &H, value: Oop) -> Oop {
    unsafe {
        object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(value)],
        ))
    }
}
struct Count {
    id: Protocol,
    behavior: Oop,
    count: usize,
}
unsafe extern "C" fn count(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &mut *c.cast::<Count>();
        let [value, _, _] = borrowed_slice(a, n).unwrap() else {
            panic!("inventory")
        };
        if send(p.id, p.id.receiver, b"behaviorOf:", &[*value]).as_object() == Some(p.behavior) {
            p.count += 1;
        }
        Outcome::loaded(Operand::unit())
    }
}
unsafe fn instances(h: &H, behavior: Oop) -> usize {
    unsafe {
        let mut c = Count {
            id: h.id,
            behavior,
            count: 0,
        };
        support::inventory(h.id, count, ptr::from_mut(&mut c).cast());
        c.count
    }
}
unsafe fn run(h: &H, block: Oop, bounded: bool) -> Oop {
    unsafe {
        let machine = object(send(h.id, h.named(b"Machine"), b"new", &[]));
        let pin = retain(h.id, h.roots, machine);
        let result = if bounded {
            let budget = send(
                h.id,
                h.named(b"Integer"),
                b"fromWord:",
                &[Operand::word(100)],
            );
            object(send(
                h.id,
                machine,
                b"runBlock:budget:",
                &[Operand::object(block), budget],
            ))
        } else {
            object(send(h.id, machine, b"runBlock:", &[Operand::object(block)]))
        };
        release(h.id, h.roots, pin);
        result
    }
}
