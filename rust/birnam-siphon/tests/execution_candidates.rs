//! Pending automatic numeric interception; explicit candidates and recovery use Oops.
#[path = "execution_machines/native_tier.rs"]
mod native_tier;
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use birnam_artifact_codec::stackless_selection::{
    self as selection, Disposition, ImplementationKind, RootKind, SelectionSpec,
};
use core::{ffi::c_void, ptr};
use std::{cell::Cell, os::unix::ffi::OsStrExt};
use support::*;
struct H {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    gc: Oop,
    tokens: Vec<usize>,
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
            let named = |name: &[u8]| {
                object(send(
                    id,
                    ns,
                    b"at:",
                    &[Operand::object(id.intern_name(name).unwrap())],
                ))
            };
            Self {
                id,
                ns,
                roots: named(b"Roots"),
                gc: named(b"GC"),
                tokens: vec![],
            }
        }
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
    unsafe fn keep(&mut self, v: Oop) -> Oop {
        self.tokens.push(unsafe { retain(self.id, self.roots, v) });
        v
    }
    unsafe fn integer(&self, i: usize) -> Operand {
        unsafe {
            send(
                self.id,
                self.named(b"Integer"),
                b"fromWord:",
                &[Operand::word(i)],
            )
        }
    }
    unsafe fn value(&self, v: Operand) -> usize {
        unsafe {
            send(self.id, self.named(b"Integer"), b"wordOf:", &[v])
                .as_word()
                .unwrap()
        }
    }
    unsafe fn vector(&mut self, values: &[Oop]) -> Oop {
        unsafe {
            let mut v = object(send(self.id, self.named(b"Vector"), b"new", &[]));
            self.keep(v);
            for &value in values {
                v = object(send(self.id, v, b"add:", &[Operand::object(value)]));
                self.keep(v);
            }
            v
        }
    }
    unsafe fn method(&mut self, fixture: &str, mode: usize, owner: Oop) -> Oop {
        unsafe {
            let bytes = std::fs::read(format!(
                "{}/../../tests/fixtures/stackless-s2/frame-descriptor-{fixture}.bin",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap();
            let directory = 64 + 8 * 32;
            let offset =
                u64::from_le_bytes(bytes[directory + 16..directory + 24].try_into().unwrap())
                    as usize;
            let revision = bytes[offset..offset + 32].try_into().unwrap();
            let spec = SelectionSpec {
                source_revision: revision,
                root_kind: RootKind::MethodOperation,
                root_index: 0,
                path: if fixture == "source-map-block" {
                    &[2, 1]
                } else {
                    &[]
                },
                implementation_kind: if fixture == "source-map-block" {
                    ImplementationKind::BlockBody
                } else {
                    ImplementationKind::NamedAdapter
                },
                disposition: Disposition::ResumableRequired,
            };
            let selection = selection::encode_artifact_selection(&[revision], &[spec]).unwrap();
            let method = load(
                self.id,
                "execution_machine_fixture",
                &[
                    Operand::object(self.ns),
                    Operand::bytes(&bytes),
                    Operand::bytes(&selection),
                    Operand::object(owner),
                    Operand::word(mode),
                ],
            );
            self.keep(method)
        }
    }
    unsafe fn machine(&mut self) -> Oop {
        unsafe {
            let m = object(send(self.id, self.named(b"Machine"), b"new", &[]));
            self.keep(m)
        }
    }
    unsafe fn block(&mut self, method: Oop, captures: &[Oop]) -> Oop {
        unsafe {
            let values = self.vector(captures);
            let v = object(send(
                self.id,
                self.named(b"Block"),
                b"fromMethod:captures:",
                &[Operand::object(method), Operand::object(values)],
            ));
            self.keep(v)
        }
    }
    unsafe fn invoke(&self, method: Oop, receiver: Oop) -> Oop {
        unsafe {
            object(send(
                self.id,
                method,
                b"invokeOn:",
                &[Operand::object(receiver)],
            ))
        }
    }
    unsafe fn bind(&self, behavior: Oop, selector: Oop, method: Oop) {
        unsafe {
            send(
                self.id,
                behavior,
                b"addMethod:as:",
                &[Operand::object(method), Operand::object(selector)],
            );
        }
    }
    unsafe fn release_all(&mut self) {
        for token in self.tokens.drain(..) {
            unsafe { release(self.id, self.roots, token) }
        }
    }
}
#[test]
#[ignore = "requires native prefix; Oop migration pending"]
fn production_execution_candidates_preserve_semantics_and_lifetimes() {
    unsafe {
        let mut h = H::new();
        if std::env::var_os("BIRNAM_EXECUTION_NATIVE_SEED").is_some() {
            native_tier::numeric(&mut h);
            return;
        }
        native_tier::numeric(&mut h);
        h.release_all();
    }
}
