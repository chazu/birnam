//! Image save must reject an intervening dictionary edit without replay or file replacement.
mod support;
use birnam_abi::{
    Oop,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    os::unix::ffi::OsStrExt,
};
use support::*;
struct H {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    tokens: RefCell<Vec<usize>>,
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
    unsafe fn keep(&self, v: Oop) -> Oop {
        self.tokens
            .borrow_mut()
            .push(unsafe { retain(self.id, self.roots, v) });
        v
    }
    unsafe fn call(&self, r: Oop, n: &[u8], args: &[Oop]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                r,
                n,
                &args
                    .iter()
                    .copied()
                    .map(Operand::object)
                    .collect::<Vec<_>>(),
            )))
        }
    }
    unsafe fn integer(&self, n: usize) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                self.named(b"Integer"),
                b"fromWord:",
                &[Operand::word(n)],
            )))
        }
    }
    unsafe fn text(&self, b: &[u8]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                self.named(b"String"),
                b"fromBytes:",
                &[Operand::bytes(b)],
            )))
        }
    }
    unsafe fn eval(&self, b: &[u8]) -> Oop {
        unsafe {
            let result = self.call(self.named(b"Codegen"), b"eval:", &[self.text(b)]);
            assert_eq!(self.call(result, b"isError", &[]), self.named(b"false"));
            result
        }
    }
    unsafe fn clear(&self, mark: usize) {
        for t in self.tokens.borrow_mut().drain(mark..).rev() {
            unsafe { release(self.id, self.roots, t) }
        }
    }
}

unsafe fn lookup(h: &H, receiver: Oop, selector: &[u8]) -> NativeEntry {
    unsafe {
        // Keep source-backed entries alive while their dictionary binding is
        // temporarily replaced and the callback collects.
        h.call(
            receiver,
            b"methodFor:",
            &[h.keep(h.id.intern_name(selector).unwrap())],
        );
        send(
            h.id,
            h.id.receiver,
            b"bind:to:",
            &[
                Operand::object(h.id.intern_name(selector).unwrap()),
                Operand::object(receiver),
            ],
        )
        .as_entry()
        .unwrap()
    }
}
unsafe fn replace(h: &H, receiver: Oop, selector: &[u8], entry: NativeEntry) {
    unsafe {
        let vt = h.call(h.id.receiver, b"ensurePrivateVtableFor:", &[receiver]);
        install(h.id, vt, selector, entry.method.unwrap(), entry.context);
    }
}

struct Change<'a> {
    h: &'a H,
    original: NativeEntry,
    calls: Cell<usize>,
    value: Oop,
    selector: Oop,
}
unsafe extern "C" fn value(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::object(c.cast()))
}
unsafe extern "C" fn change(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Change<'_>>();
        let result = p.original.method.unwrap()(p.original.context, r, a, n);
        if result.status != 0 {
            return result;
        }
        p.h.keep(object(result.value));
        p.calls.set(p.calls.get() + 1);
        let vt = p.h.named(b"IntegerVt");
        install(p.h.id, vt, b"snapshotIntervened", value, p.value.cast());
        collect(p.h.id, p.h.named(b"GC"));
        result
    }
}
#[test]
#[ignore = "production modules"]
fn snapshot_does_not_publish_stale_code_or_erase_the_intervening_edit() {
    unsafe {
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        let roots = object(send(
            id,
            ns,
            b"at:",
            &[Operand::object(id.intern_name(b"Roots").unwrap())],
        ));
        let h = H {
            id,
            ns,
            roots,
            tokens: RefCell::new(vec![]),
        };
        h.eval(b"[Reflect defineMethod: #snapshotOwned on: Integer as: [:self | 41]]");
        let path = std::env::temp_dir().join(format!(
            "birnam-snapshot-stamp-{}.image",
            std::process::id()
        ));
        let filename = h.text(path.as_os_str().as_bytes());
        let system = h.named(b"System");
        assert_eq!(h.call(system, b"saveImage:", &[filename]), h.named(b"true"));
        let bytes = std::fs::read(&path).unwrap();
        let codegen = h.named(b"Codegen");
        let original = lookup(&h, codegen, b"compilePortableImageCode:bindings:selection:");
        let selector = h.keep(id.intern_name(b"snapshotIntervened").unwrap());
        let answer = h.integer(73);
        let mut p = Box::new(Change {
            h: &h,
            original,
            calls: Cell::new(0),
            value: answer,
            selector,
        });
        replace(
            &h,
            codegen,
            b"compilePortableImageCode:bindings:selection:",
            NativeEntry::new(change, (&mut *p as *mut Change<'_>).cast()),
        );
        let outcome = h.call(system, b"saveImage:", &[filename]);
        replace(
            &h,
            codegen,
            b"compilePortableImageCode:bindings:selection:",
            original,
        );
        assert_eq!(outcome, h.named(b"nil"));
        assert_eq!(p.calls.get(), 1);
        assert_eq!(h.call(h.integer(1), b"snapshotIntervened", &[]), answer);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        send(
            id,
            h.named(b"IntegerVt"),
            b"removeMethod:",
            &[Operand::object(p.selector)],
        );
        std::fs::remove_file(path).unwrap();
        h.clear(0);
    }
}
