//! Capture borrowing is conditional on the current reader and grant bindings.
mod support;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use birnam_artifact_codec::portable_program::{NodeSpec, ProgramSpec, StatementSpec, encode};
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
    gc: Oop,
    tokens: RefCell<Vec<usize>>,
    _floor: CompiledFloor,
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
    unsafe fn keep(&self, value: Oop) -> Oop {
        self.tokens
            .borrow_mut()
            .push(unsafe { retain(self.id, self.roots, value) });
        value
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
    unsafe fn vector(&self, values: &[Oop]) -> Oop {
        unsafe {
            let mut v = self.call(self.named(b"Vector"), b"new", &[]);
            for &value in values {
                v = self.call(v, b"add:", &[value]);
            }
            v
        }
    }
    unsafe fn admit(&self, bytes: &[u8]) -> Oop {
        unsafe {
            let b = self.keep(object(send(
                self.id,
                self.named(b"Bytes"),
                b"fromBytes:",
                &[Operand::bytes(bytes)],
            )));
            self.call(self.named(b"CodeObject"), b"fromCanonicalBytes:", &[b])
        }
    }
    unsafe fn bits(&self, value: Oop) -> usize {
        unsafe { send(self.id, value, b"bits", &[]).as_word().unwrap() }
    }
}
unsafe fn binding(h: &H, r: Oop, name: &[u8]) -> NativeEntry {
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
unsafe fn vtable(h: &H, r: Oop) -> Oop {
    unsafe {
        object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(r)],
        ))
    }
}
unsafe fn set(h: &H, vt: Oop, name: &[u8], entry: NativeEntry) {
    unsafe {
        send(
            h.id,
            vt,
            b"addMethod",
            &[
                Operand::object(h.id.intern_name(name).unwrap()),
                Operand::entry(entry),
            ],
        );
    }
}
struct Hook {
    id: Protocol,
    gc: Oop,
    word: Oop,
    original: NativeEntry,
    behavior: Oop,
    calls: Cell<usize>,
    first: Cell<Oop>,
    mutate: Cell<bool>,
}
unsafe extern "C" fn deny_prepare(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    let h = unsafe { &*c.cast::<Hook>() };
    h.calls.set(h.calls.get() + 1);
    Outcome::failed(901, 7)
}
unsafe extern "C" fn fresh(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let h = &*c.cast::<Hook>();
        let [index] = borrowed_slice(a, n).unwrap() else {
            panic!("capture index")
        };
        let index = index.as_word().unwrap();
        if index == 1 {
            collect(h.id, h.gc);
            assert!(
                present(h.id, h.first.get()),
                "fresh result from earlier projection must be retained"
            );
        }
        let value = object(h.id.send_named(h.word, b"fromBits:", &[Operand::word(101 + index)])?);
        if index == 0 {
            h.first.set(value);
        }
        h.calls.set(h.calls.get() + 1);
        Ok(Operand::object(value))
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
unsafe extern "C" fn grant_wrapper(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let h = unsafe { &*c.cast::<Hook>() };
    let answer = unsafe { (h.original.method.unwrap())(h.original.context, r, a, n) };
    // The canonical grant must decline when this wrapper is installed. It may
    // mutate after querying, so treating its reply as a canonical grant is unsafe.
    assert_eq!(answer.value.as_word(), Some(0));
    if h.mutate.get() {
        unsafe { install(h.id, h.behavior, b"captureWordAt:", fresh, c) }
    };
    answer
}
unsafe extern "C" fn unbound(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(6, 0)
}
unsafe extern "C" fn bad_mode(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::word(3))
}
unsafe extern "C" fn fail(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(812, 19)
}

#[test]
#[ignore = "requires rebuilt production modules"]
fn capture_reads_borrow_owned_edges_but_retain_replaced_fresh_results() {
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
        let roots = named(b"Roots");
        let h = H {
            id,
            ns,
            roots,
            gc: named(b"GC"),
            tokens: RefCell::new(Vec::new()),
            _floor: CompiledFloor::pin(id, ns, roots),
        };
        let bytes = encode(ProgramSpec {
            parameters: &[],
            captures: &[b"a", b"b"],
            lets: &[],
            statements: &[StatementSpec::Expression { root_node: 0 }],
            nodes: &[NodeSpec::LexicalRead(0)],
        })
        .unwrap();
        let code = h.admit(&bytes);
        let a = h.keep(object(send(
            id,
            h.named(b"Word"),
            b"fromBits:",
            &[Operand::word(7)],
        )));
        let b = h.keep(object(send(
            id,
            h.named(b"Word"),
            b"fromBits:",
            &[Operand::word(8)],
        )));
        let block = h.call(
            h.named(b"Codegen"),
            b"materializeSynchronous:captures:",
            &[code, h.vector(&[a, b])],
        );
        let vt = vtable(&h, block);
        let factory = h.named(b"Block");
        let factory_vt = vtable(&h, factory);
        let reader = binding(&h, block, b"captureWordAt:");
        let grant = binding(&h, block, b"captureReadMode");
        assert_eq!(
            send(
                id,
                block,
                b"captureReadMode",
                &[]
            )
            .as_word(),
            Some(1)
        );
        assert_eq!(h.call(block, b"value", &[]), a);
        assert_eq!(id.send_named(block, b"captureReadMode", &[Operand::word(0)]).unwrap_err().status, 1);
        // The normal route does not consult the factory query at all.
        let fallback_grant = binding(&h, factory, b"captureReadModeOf:");
        set(&h, factory_vt, b"captureReadModeOf:", NativeEntry::new(fail, std::ptr::null_mut()));
        assert_eq!(h.call(block, b"value", &[]), a);
        set(&h, factory_vt, b"captureReadModeOf:", fallback_grant);
        let mut hook = Hook {
            id,
            gc: h.gc,
            word: h.named(b"Word"),
            original: grant,
            behavior: vt,
            calls: Cell::new(0),
            first: Cell::new(std::ptr::null_mut()),
            mutate: Cell::new(false),
        };
        let context = (&raw mut hook).cast();
        // Neither the direct nor factory Word-indexed default reader requests
        // allocation admission. Collector mutation checks remain on allocating ops.
        let gcvt = vtable(&h, h.gc);
        let prepare = binding(&h, h.gc, b"prepareAllocation");
        set(
            &h,
            gcvt,
            b"prepareAllocation",
            NativeEntry::new(deny_prepare, context),
        );
        assert_eq!(
            send(id, block, b"captureWordAt:", &[Operand::word(0)]).as_object(),
            Some(a)
        );
        assert_eq!(
            send(
                id,
                h.named(b"Block"),
                b"captureWord:of:",
                &[Operand::word(1), Operand::object(block)]
            )
            .as_object(),
            Some(b)
        );
        assert_eq!(h.call(block, b"value", &[]), a);
        assert_eq!(hook.calls.get(), 0);
        set(&h, gcvt, b"prepareAllocation", prepare);
        for mutate_after_grant in [false, true] {
            hook.calls.set(0);
            hook.mutate.set(mutate_after_grant);
            hook.first.set(std::ptr::null_mut());
            if mutate_after_grant {
                set(
                    &h,
                    vt,
                    b"captureReadMode",
                    NativeEntry::new(grant_wrapper, context),
                );
            } else {
                set(&h, vt, b"captureWordAt:", NativeEntry::new(fresh, context));
            }
            let result = h.call(block, b"value", &[]);
            assert_eq!(h.bits(result), 101);
            assert_eq!(hook.calls.get(), 2);
            collect(id, h.gc);
            assert!(present(id, result));
            set(&h, vt, b"captureReadMode", grant);
            set(&h, vt, b"captureWordAt:", reader);
        }
        // Removing the direct query must not bypass a replaced direct reader.
        set(&h, vt, b"captureReadMode", NativeEntry::new(unbound, std::ptr::null_mut()));
        hook.calls.set(0);
        hook.first.set(std::ptr::null_mut());
        set(&h, vt, b"captureWordAt:", NativeEntry::new(fresh, context));
        assert_eq!(h.bits(h.call(block, b"value", &[])), 101);
        assert_eq!(hook.calls.get(), 2);
        set(&h, vt, b"captureWordAt:", reader);
        set(&h, vt, b"captureReadMode", grant);
        set(&h, vt, b"captureReadMode", NativeEntry::new(fail, std::ptr::null_mut()));
        let failure = id.send_named(block, b"value", &[]).unwrap_err();
        assert_eq!((failure.status, failure.detail), (812, 19));
        set(&h, vt, b"captureReadMode", grant);
        set(&h, vt, b"captureReadMode", NativeEntry::new(bad_mode, std::ptr::null_mut()));
        assert_eq!(id.send_named(block, b"value", &[]).unwrap_err().status, 1);
        set(&h, vt, b"captureReadMode", grant);
        // A forwarding Id proof query cannot grant borrowing from an old
        // answer and then replace the reader. The request fences that query's
        // own binding, so its canonical implementation returns zero.
        let matcher = binding(&h, id.receiver, b"nativeBindingsMatch:");
        hook.original = matcher;
        hook.mutate.set(true);
        hook.calls.set(0);
        set(
            &h,
            id.receiver,
            b"nativeBindingsMatch:",
            NativeEntry::new(grant_wrapper, context),
        );
        let result = h.call(block, b"value", &[]);
        assert_eq!(h.bits(result), 101);
        assert_eq!(hook.calls.get(), 2);
        set(&h, id.receiver, b"nativeBindingsMatch:", matcher);
        set(&h, vt, b"captureWordAt:", reader);
        set(
            &h,
            vt,
            b"captureWordAt:",
            NativeEntry::new(fail, std::ptr::null_mut()),
        );
        let failure = id.send_named(block, b"value", &[]).unwrap_err();
        assert_eq!((failure.status, failure.detail), (812, 19));
        set(&h, vt, b"captureWordAt:", reader);
        // Once the native caller's temporary capture roots disappear, the live
        // Block remains the lifetime cover. Precise GC must retain both edges.
        let fresh_a = allocate(id, behavior(id), 0);
        let ta = retain(id, roots, fresh_a);
        let fresh_b = allocate(id, behavior(id), 0);
        let tb = retain(id, roots, fresh_b);
        let method = h.call(block, b"methodFor:", &[id.intern_name(b"value").unwrap()]);
        let nil = h.named(b"nil");
        assert_eq!(send(id, nil, b"captureReadMode", &[]).as_word(), Some(2));
        assert_eq!(h.call(method, b"invokeOn:", &[nil]), nil);
        let only_owner = h.keep(object(send(
            id,
            h.named(b"Block"),
            b"fromMethod:captureOperands:",
            &[
                Operand::object(method),
                Operand::object(fresh_a),
                Operand::object(fresh_b),
            ],
        )));
        release(id, roots, ta);
        release(id, roots, tb);
        collect(id, h.gc);
        assert!(present(id, fresh_a) && present(id, fresh_b));
        assert_eq!(h.call(only_owner, b"value", &[]), fresh_a);
        assert_eq!(
            send(
                id,
                factory,
                b"captureReadModeOf:",
                &[Operand::object(h.named(b"nil"))]
            )
            .as_word(),
            Some(2)
        );
        for token in h.tokens.borrow_mut().drain(..) {
            release(id, roots, token);
        }
    }
}
