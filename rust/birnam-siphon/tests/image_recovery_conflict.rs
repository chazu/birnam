//! Investigation: image compensation after an intervening live dictionary write.
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

struct Conflict {
    id: Protocol,
    original: NativeEntry,
    selector: Oop,
    marker: Oop,
    globals: Oop,
    db: Oop,
    side_key: Oop,
    gc: Oop,
    calls: Cell<usize>,
}
unsafe extern "C" fn marker(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::object(c.cast()))
}
unsafe extern "C" fn conflict(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Conflict>();
        let args = std::slice::from_raw_parts(a, n);
        let result = p.original.method.unwrap()(p.original.context, r, a, n);
        if result.status != 0 || args[2].as_object() != Some(p.selector) {
            return result;
        }
        p.calls.set(p.calls.get() + 1);
        let vt = object(send(p.id, p.id.receiver, b"behaviorOf:", &[args[1]]));
        send(
            p.id,
            vt,
            b"addMethod",
            &[
                args[2],
                Operand::entry(NativeEntry::new(marker, p.marker.cast())),
            ],
        );
        send(
            p.id,
            p.globals,
            b"at:put:",
            &[Operand::object(p.side_key), Operand::object(p.marker)],
        );
        send(
            p.id,
            p.db,
            b"assert:attr:value:",
            &[
                Operand::object(p.side_key),
                Operand::object(p.side_key),
                Operand::object(p.marker),
            ],
        );
        collect(p.id, p.gc);
        Outcome::failed(881, 73)
    }
}
#[test]
#[ignore = "production modules; characterizes known image rollback conflict"]
fn image_compensation_characterizes_intervening_dictionary_write() {
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
        h.eval(b"[Reflect defineMethod: #imageConflictProbe on: Integer as: [:self | #saved]]");
        let dir =
            std::env::temp_dir().join(format!("birnam-image-conflict-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = h.text(dir.join("probe.image").as_os_str().as_bytes());
        let system = h.named(b"System");
        assert_eq!(h.call(system, b"saveImage:", &[path]), h.named(b"true"));
        h.eval(b"[Reflect defineMethod: #imageConflictProbe on: Integer as: [:self | #before]]");
        let codegen = h.named(b"Codegen");
        let original = lookup(&h, codegen, b"installPortableImageBinding:on:as:source:");
        let nested = h.keep(id.intern_name(b"nested").unwrap());
        let mut probe = Box::new(Conflict {
            id,
            original,
            selector: h.keep(id.intern_name(b"imageConflictProbe").unwrap()),
            marker: nested,
            globals: h.named(b"Globals"),
            db: h.named(b"db"),
            side_key: h.keep(id.intern_name(b"ImageConflictIndependentWrite").unwrap()),
            gc: h.named(b"GC"),
            calls: Cell::new(0),
        });
        for rebuilt in [false, true] {
            replace(
                &h,
                codegen,
                b"installPortableImageBinding:on:as:source:",
                NativeEntry::new(conflict, (&mut *probe as *mut Conflict).cast()),
            );
            let result = id.send_named(
                system,
                if std::env::var_os("BIRNAM_IMAGE_STARTUP").is_some() {
                    b"startImage:rebuild:"
                } else { b"loadImage:rebuild:" },
                &[
                    Operand::object(path),
                    Operand::object(h.named(if rebuilt { b"true" } else { b"false" })),
                ],
            );
            replace(
                &h,
                codegen,
                b"installPortableImageBinding:on:as:source:",
                original,
            );
            let failure = result.unwrap_err();
            assert_eq!((failure.status, failure.detail), (881, 73));
            let got = h.call(h.integer(1), b"imageConflictProbe", &[]);
            eprintln!(
                "image-conflict rebuilt={rebuilt} callbacks={} nested_survives={} old_restored={}",
                probe.calls.get(),
                got == nested,
                got == id.intern_name(b"before").unwrap()
            );
            let global = h.call(probe.globals, b"at:", &[probe.side_key]);
            let fact = h.call(
                probe.db,
                b"currentValue:attr:",
                &[probe.side_key, probe.side_key],
            );
            eprintln!(
                "image-conflict unrelated_global_survives={} explicit_fact_survives={}",
                global == nested,
                fact == nested
            );
            assert_eq!(probe.calls.get(), 1);
            let expected = if std::env::var_os("BIRNAM_EXPECT_IMAGE_CONFLICT_PRESERVED").is_some() {
                nested
            } else {
                id.intern_name(b"before").unwrap()
            };
            let expected_side =
                if std::env::var_os("BIRNAM_EXPECT_IMAGE_CONFLICT_PRESERVED").is_some() {
                    nested
                } else {
                    h.named(b"nil")
                };
            assert_eq!(global, expected_side, "unrelated Global write must survive");
            assert_eq!(fact, expected_side, "explicit user fact must survive");
            assert_eq!(
                got, expected,
                "image compensation must preserve an intervening dictionary edit"
            );
            probe.calls.set(0);
        }
        std::fs::remove_dir_all(dir).unwrap();
        h.clear(0);
    }
}
