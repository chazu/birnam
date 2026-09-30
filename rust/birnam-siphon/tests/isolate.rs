//! Copied-value conversion through live owners, including collecting replacements.
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
}

#[test]
#[ignore = "requires production modules"]
fn isolate_value_conversion_uses_owner_messages() {
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
        let h = H {
            id,
            ns,
            roots: named(b"Roots"),
            gc: named(b"GC"),
            tokens: RefCell::new(Vec::new()),
        };
        let iv = named(b"IntegerVt");
        let selector = id.intern_name(b"fromWord:").unwrap();
        let original = send(
            id,
            id.receiver,
            b"bindingOf:in:",
            &[Operand::object(selector), Operand::object(iv)],
        )
        .as_entry()
        .unwrap();
        let alias = id.intern_name(b"isolateTestFromWord:").unwrap();
        send(
            id,
            iv,
            b"addMethod",
            &[Operand::object(alias), Operand::entry(original)],
        );
        let mut forward = Forward {
            id,
            gc: h.gc,
            alias,
        };
        install(
            id,
            iv,
            b"fromWord:",
            collecting,
            (&mut forward as *mut Forward).cast(),
        );
        let isolate = named(b"Isolate");
        let value = h.eval(b"#[nil true false 42 1.25 \"binary\" #symbol #[7 8]]");
        let encoded = h.call(isolate, b"encode:", &[value]);
        let bytes = h.bytes(encoded);
        assert_eq!(&bytes[..5], &[7, 0, 0, 0, 8]);
        let restored = h.call(isolate, b"decode:", &[encoded]);
        assert_eq!(h.number(h.call(restored, b"size", &[])), 8);
        collect(id, h.gc);
        assert_eq!(h.bytes(h.call(restored, b"at:", &[h.int(5)])), b"binary");
        assert_eq!(h.bytes(h.call(isolate, b"encode:", &[restored])), bytes);
        let map = h.call(named(b"Map"), b"new", &[]);
        let map = h.call(map, b"at:put:", &[h.string(b"key"), value]);
        let encoded_map = h.call(isolate, b"encode:", &[map]);
        let map2 = h.call(isolate, b"decode:", &[encoded_map]);
        assert_eq!(
            h.bytes(h.call(isolate, b"encode:", &[map2])),
            h.bytes(encoded_map)
        );
        let block = h.eval(b"[| 1]");
        let failure = h.call(isolate, b"encode:", &[block]);
        collect(id, h.gc);
        assert_eq!(h.call(failure, b"isError", &[]), named(b"true"));
        assert_eq!(
            h.call(failure, b"class", &[]),
            id.intern_name(b"encode").unwrap()
        );
        assert_eq!(
            h.call(failure, b"kind", &[]),
            id.intern_name(b"nonTransferable").unwrap()
        );
        assert_eq!(
            h.bytes(h.call(failure, b"message", &[])),
            b"NonTransferable"
        );
        let malformed = h.keep(object(send(
            id,
            named(b"Bytes"),
            b"fromBytes:",
            &[Operand::bytes(&[7, 255, 255, 255, 255])],
        )));
        let failure = h.call(isolate, b"decode:", &[malformed]);
        assert_eq!(
            h.bytes(h.call(failure, b"message", &[])),
            b"ContainerTooLarge"
        );
        let cycle = h.eval(b"#[1]");
        let vt = named(b"VectorVt");
        let at = id.intern_name(b"at:").unwrap();
        let old_at = send(
            id,
            id.receiver,
            b"bindingOf:in:",
            &[Operand::object(at), Operand::object(vt)],
        )
        .as_entry()
        .unwrap();
        let mut cycle_context = (id, h.gc, cycle);
        install(
            id,
            vt,
            b"at:",
            cyclic,
            (&mut cycle_context as *mut (Protocol, Oop, Oop)).cast(),
        );
        let failure = h.call(isolate, b"encode:", &[cycle]);
        send(
            id,
            vt,
            b"addMethod",
            &[Operand::object(at), Operand::entry(old_at)],
        );
        assert_eq!(h.bytes(h.call(failure, b"message", &[])), b"CyclicValue");
        // A real owner failure must propagate without becoming a language codec error.
        install(id, iv, b"fromWord:", failing, std::ptr::null_mut());
        let result = id
            .send_named(isolate, b"decode:", &[Operand::object(encoded)])
            .unwrap_err();
        assert_eq!(result.detail, 813);
        send(
            id,
            iv,
            b"addMethod",
            &[Operand::object(selector), Operand::entry(original)],
        );
        send(id, iv, b"removeMethod", &[Operand::object(alias)]);
        reply_construction_failure(&h);
        for t in h.tokens.borrow_mut().drain(..) {
            release(id, h.roots, t)
        }
        collect(id, h.gc);
    }
}

unsafe fn reply_construction_failure(h: &H) {
    unsafe {
        let peer = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../rust/birnam-abi/tests/fixtures/id-protocol-v1/isolate_reply.py");
        let previous = std::env::var_os("BIRNAM_WORKER_EXECUTABLE");
        std::env::set_var("BIRNAM_WORKER_EXECUTABLE", &peer);
        let handle = h.call(
            h.named(b"Isolate"),
            b"start:selector:image:",
            &[
                h.string(b"Peer"),
                h.string(b"echo:"),
                h.string(peer.as_os_str().as_bytes()),
            ],
        );
        match previous {
            Some(v) => std::env::set_var("BIRNAM_WORKER_EXECUTABLE", v),
            None => std::env::remove_var("BIRNAM_WORKER_EXECUTABLE"),
        }
        assert_eq!(
            h.call(handle, b"state", &[]),
            h.id.intern_name(b"ready").unwrap()
        );
        let request = h.call(handle, b"submit:timeout:", &[h.named(b"nil"), h.int(5000)]);
        let vt = h.named(b"VectorVt");
        let selector = h.id.intern_name(b"new").unwrap();
        let original = send(
            h.id,
            h.id.receiver,
            b"bindingOf:in:",
            &[Operand::object(selector), Operand::object(vt)],
        )
        .as_entry()
        .unwrap();
        install(h.id, vt, b"new", failing, std::ptr::null_mut());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let result = loop {
            match h
                .id
                .send_named(handle, b"pollRequest:", &[Operand::object(request)])
            {
                Err(e) => break e,
                Ok(v) => assert_eq!(v.as_object(), Some(h.named(b"nil"))),
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        send(
            h.id,
            vt,
            b"addMethod",
            &[Operand::object(selector), Operand::entry(original)],
        );
        assert_eq!(result.detail, 813);
        assert_eq!(
            h.call(handle, b"state", &[]),
            h.id.intern_name(b"exited").unwrap()
        );
    }
}
unsafe extern "C" fn failing(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(1, 813)
}

unsafe extern "C" fn cyclic(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe {
        let (id, gc, value) = *c.cast::<(Protocol, Oop, Oop)>();
        collect(id, gc);
        Outcome::loaded(Operand::object(value))
    }
}
