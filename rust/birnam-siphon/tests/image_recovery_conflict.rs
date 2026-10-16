//! Failed live image loads retain applied and callback writes without inverses.
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
    source: Option<Oop>,
    source_name: Oop,
    codegen: Oop,
    roots: Oop,
    published: Cell<Oop>,
    held: RefCell<Vec<usize>>,
    error: Option<Oop>,
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
        let target = object(args[1]);
        p.published.set(target);
        p.held.borrow_mut().push(retain(p.id, p.roots, target));
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
        if let Some(source) = p.source {
            assert_eq!(send(p.id, p.codegen, b"recordModuleSource:named:",
                &[Operand::object(source), Operand::object(p.source_name)]).as_object(), Some(source));
        }
        collect(p.id, p.gc);
        p.error.map_or_else(|| Outcome::failed(881, 73), |error| Outcome::loaded(Operand::object(error)))
    }
}
#[test]
#[ignore = "production native modules"]
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
        let authored = std::env::var_os("BIRNAM_IMAGE_AUTHORED").is_some();
        let late = std::env::var_os("BIRNAM_IMAGE_LATE_GLOBALS").is_some();
        if authored {
            let image = h.text(directory().join("birnam-tools.image").as_os_str().as_bytes());
            assert_eq!(h.call(h.named(b"System"), b"startImageMapped:", &[image]), h.named(b"true"));
        }
        if late {
            h.eval(b"let ImageConflictLate := [Object clone]; [Reflect defineMethod: #imageConflictProbe on: ImageConflictLate as: [:self | #saved]]");
        } else {
            h.eval(b"[Reflect defineMethod: #imageConflictProbe on: Integer as: [:self | #saved]]");
        }
        let dir =
            std::env::temp_dir().join(format!("birnam-image-conflict-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = h.text(dir.join("probe.image").as_os_str().as_bytes());
        let system = h.named(b"System");
        assert_eq!(h.call(system, b"saveImage:", &[path]), h.named(b"true"));
        if late {
            h.eval(b"[Reflect defineMethod: #imageConflictProbe on: ImageConflictLate as: [:self | #before]]");
        } else {
            h.eval(b"[Reflect defineMethod: #imageConflictProbe on: Integer as: [:self | #before]]");
        }
        let codegen = h.named(b"Codegen");
        let original = lookup(&h, codegen, b"installPortableImageBinding:on:as:source:");
        let nested = if std::env::var_os("BIRNAM_IMAGE_BOUND_NIL").is_some() { h.named(b"nil") }
            else { h.keep(id.intern_name(b"nested").unwrap()) };
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
            source: if authored { None } else { Some(h.text(b"[Module named: #imageConflictSource requires: #[]]; true")) },
            source_name: h.keep(id.intern_name(b"imageConflictSource").unwrap()),
            codegen,
            roots,
            published: Cell::new(std::ptr::null_mut()),
            held: RefCell::new(vec![]),
            error: if std::env::var_os("BIRNAM_IMAGE_RETURN_ERROR").is_some() {
                Some(h.call(h.named(b"Error"),b"kind:",&[h.keep(id.intern_name(b"originalImageFailure").unwrap())]))
            } else { None },
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
            if let Some(error) = probe.error {
                assert_eq!(result.unwrap().as_object(),Some(error),"ordinary Error identity must survive failed loading");
            } else {
                let failure = result.unwrap_err();
                assert_eq!((failure.status, failure.detail), (881, 73));
            }
            // The callback edits the restored heap target. A partial load need
            // not have switched the Global that previously named the old object.
            let got = h.call(probe.published.get(), b"imageConflictProbe", &[]);
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
            if std::env::var_os("BIRNAM_IMAGE_STARTUP").is_none() {
                let report = h.call(system, b"imageSaveReport", &[]);
                assert_eq!(h.call(report, b"status", &[]), id.intern_name(b"partialLoad").unwrap());
            }
            assert_eq!(h.call(system, b"imageCompositionIdentity", &[]), h.named(b"nil"));
            let expected = nested;
            let expected_side = nested;
            assert_eq!(global, expected_side, "unrelated Global write must survive");
            assert_eq!(h.call(probe.globals, b"includes:", &[probe.side_key]), h.named(b"true"), "bound nil must remain present");
            assert_eq!(h.call(probe.db, b"hasValue:attr:", &[probe.side_key,probe.side_key]), h.named(b"true"));
            if let Some(source) = probe.source {
                assert_eq!(h.call(codegen,b"moduleSourceFor:",&[probe.source_name]), source, "source introduced by callback must survive");
            }
            assert_eq!(fact, expected_side, "explicit user fact must survive");
            assert_eq!(
                got, expected,
                "failed loading must preserve an intervening dictionary edit"
            );
            probe.calls.set(0);
        }
        for token in probe.held.borrow_mut().drain(..) { release(id, roots, token); }
        std::fs::remove_dir_all(dir).unwrap();
        h.clear(0);
    }
}
