//! Source and source-less Block materialization through production Id owners.
mod support;
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
use birnam_artifact_codec::portable_program::{NodeSpec, ProgramSpec, StatementSpec, encode};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
};
use std::os::unix::ffi::OsStrExt;
use support::*;
struct H {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    gc: Oop,
    tokens: RefCell<Vec<usize>>,
    floor: CompiledFloor,
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
unsafe fn binding(h: &H, vt: Oop, name: &[u8]) -> NativeEntry {
    unsafe {
        send(
            h.id,
            h.id.receiver,
            b"bindingOf:in:",
            &[
                Operand::object(h.id.intern_name(name).unwrap()),
                Operand::object(vt),
            ],
        )
        .as_entry()
        .unwrap()
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
unsafe fn behavior_of(h: &H, value: Oop) -> Oop {
    unsafe {
        object(send(
            h.id,
            h.id.receiver,
            b"behaviorOf:",
            &[Operand::object(value)],
        ))
    }
}
impl H {
    unsafe fn int(&self, n: i64) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                self.named(b"Integer"),
                b"fromWord:",
                &[Operand::word(n as usize)],
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
    unsafe fn kind(&self, r: Oop, kind: &[u8]) {
        unsafe {
            assert_eq!(
                self.call(r, b"kind", &[]),
                self.id.intern_name(kind).unwrap()
            );
        }
    }
    unsafe fn count(&self, r: Oop, selector: &[u8], count: usize) {
        unsafe {
            assert_eq!(self.number(self.call(r, selector, &[])), count);
        }
    }
    unsafe fn clear_from(&self, index: usize) {
        let tokens = self.tokens.borrow_mut().split_off(index);
        for token in tokens {
            unsafe {
                release(self.id, self.roots, token);
            }
        }
    }
}
impl H {
    unsafe fn eval(&self, source: &[u8]) -> Oop {
        unsafe { self.call(self.named(b"Codegen"), b"eval:", &[self.string(source)]) }
    }
    unsafe fn encoded(&self, bytes: &[u8]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                self.named(b"Bytes"),
                b"fromBytes:",
                &[Operand::bytes(bytes)],
            )))
        }
    }
    unsafe fn admit(&self, bytes: &[u8]) -> Oop {
        unsafe {
            self.call(
                self.named(b"CodeObject"),
                b"fromCanonicalBytes:",
                &[self.encoded(bytes)],
            )
        }
    }
    unsafe fn materialize(&self, code: Oop, captures: &[Oop], resumable: bool) -> Oop {
        unsafe {
            self.call(
                self.named(b"Codegen"),
                if resumable {
                    b"materializeResumable:captures:"
                } else {
                    b"materializeSynchronous:captures:"
                },
                &[code, self.vector(captures)],
            )
        }
    }
    unsafe fn units(&self) -> usize {
        unsafe {
            send(self.id, self.named(b"TCC"), b"liveNativeUnits", &[])
                .as_word()
                .unwrap().checked_sub(self.floor.units).expect("compiled core units remain live")
        }
    }
    unsafe fn native_bytes(&self) -> usize {
        unsafe {
            send(self.id, self.named(b"TCC"), b"liveNativeBytes", &[])
                .as_word()
                .unwrap().checked_sub(self.floor.bytes).expect("compiled core bytes remain live")
        }
    }
    unsafe fn collect(&self) {
        unsafe {
            collect(self.id, self.gc);
            send(self.id, self.named(b"TCC"), b"checkpoint", &[]);
        }
    }
}
struct Forward {
    id: Protocol,
    gc: Oop,
    alias: Oop,
    calls: Cell<usize>,
    fail: Cell<bool>,
}
unsafe extern "C" fn forward(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let f = &*c.cast::<Forward>();
        f.calls.set(f.calls.get() + 1);
        if f.fail.get() {
            return Err(Outcome::failed(881, 73));
        }
        let a = borrowed_slice(a, n).unwrap();
        let copy = a
            .first()
            .and_then(|a| a.as_bytes())
            .map(|b| borrowed_bytes(b.pointer, b.length).unwrap().to_vec());
        f.id.send_named(f.gc, b"collectPrecisely", &[])?;
        if let Some(bytes) = copy {
            f.id.send(r, f.alias, &[Operand::bytes(&bytes)])
        } else {
            f.id.send(r, f.alias, a)
        }
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
struct CaptureTiming {
    id: Protocol,
    target: Oop,
    before: Oop,
    after: Oop,
    compiled: Cell<bool>,
    original_at: Oop,
    original_compile: Oop,
}
unsafe extern "C" fn timed_at(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let t = unsafe { &*c.cast::<CaptureTiming>() };
    if r == t.target {
        Outcome::loaded(Operand::object(if t.compiled.get() {
            t.after
        } else {
            t.before
        }))
    } else {
        unsafe { t.id.send(r, t.original_at, borrowed_slice(a, n).unwrap()) }
            .map_or_else(|e| e, Outcome::loaded)
    }
}
unsafe extern "C" fn timed_compile(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    let t = unsafe { &*c.cast::<CaptureTiming>() };
    t.compiled.set(true);
    unsafe {
        t.id.send(r, t.original_compile, borrowed_slice(a, n).unwrap())
    }
    .map_or_else(|e| e, Outcome::loaded)
}
fn literal_program(value: i64) -> Vec<u8> {
    encode(ProgramSpec {
        parameters: &[],
        captures: &[],
        lets: &[],
        statements: &[StatementSpec::Expression { root_node: 0 }],
        nodes: &[NodeSpec::Integer(value)],
    })
    .unwrap()
}
unsafe extern "C" fn constant(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::loaded(Operand::object(c.cast()))
}
#[test]
#[ignore = "requires production native modules with TCC resource metrics"]
fn portable_materialization_preserves_identity_control_and_lifetime() {
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
        let h = H {
            id,
            ns,
            roots: named(b"Roots"),
            gc: named(b"GC"),
            tokens: RefCell::new(Vec::new()),
            floor: CompiledFloor::pin(id, ns, named(b"Roots")),
        };
        let nil = h.named(b"nil");
        let codegen = h.named(b"Codegen");
        // Historical PortableCode inspection declines Blocks without canonical code.
        let plain_method = h.call(codegen, b"compile:", &[h.string(b"42")]);
        let plain_block = h.call(
            h.named(b"Block"),
            b"fromMethod:captures:",
            &[plain_method, h.vector(&[])],
        );
        for plain in [h.named(b"Block"), plain_block] {
            assert_eq!(h.call(plain, b"codeObject", &[]), nil);
            assert_eq!(h.call(plain, b"captureCount", &[]), nil);
            assert_eq!(h.call(plain, b"captureAt:", &[h.int(0)]), nil);
        }
        let code_factory = h.named(b"CodeObject");
        let fixture = include_bytes!("../../../tests/fixtures/portable-code-v1/program-send.bin");
        let code = h.admit(fixture);
        let canonical = h.call(code_factory, b"canonicalBytesOf:", &[code]);
        assert_eq!(behavior_of(&h, canonical), h.named(b"BytesVt"));
        assert_eq!(h.bytes(canonical), fixture);
        assert_eq!(h.call(code_factory, b"canonicalBytesOf:", &[nil]), nil);
        let malformed = h.admit(b"not a program");
        h.kind(malformed, b"invalidPortableProgram");
        let inspect = binding(&h, h.named(b"CodeObjectVt"), b"program");
        set(
            &h,
            h.named(b"CodeObjectVt"),
            b"program",
            NativeEntry::new(constant, nil.cast()),
        );
        assert_eq!(
            h.bytes(h.call(code_factory, b"canonicalBytesOf:", &[code])),
            fixture
        );
        for resumable in [false, true] {
            let block = h.materialize(code, &[], resumable);
            assert_eq!(h.number(h.call(block, b"value", &[])), 42);
            assert_eq!(h.call(block, b"codeObject", &[]), code);
            h.count(block, b"captureCount", 0);
            assert_eq!(h.call(block, b"captureAt:", &[h.int(0)]), nil);
            let cloned = h.call(block, b"clone", &[]);
            assert_ne!(cloned, block);
            assert_eq!(h.call(cloned, b"codeObject", &[]), code);
            assert_eq!(h.number(h.call(cloned, b"value", &[])), 42);
        }
        set(&h, h.named(b"CodeObjectVt"), b"program", inspect);
        h.count(codegen, b"materializationCount", 2);
        let second = h.admit(fixture);
        assert_ne!(second, code);
        for resumable in [false, true] {
            let block = h.materialize(second, &[], resumable);
            assert_eq!(h.call(block, b"codeObject", &[]), second);
        }
        h.count(codegen, b"materializationCount", 2);
        let before = h.units();
        assert_eq!(h.number(h.call(codegen, b"evictMaterializations", &[])), 2);
        h.collect();
        assert_eq!(h.units(), before);

        // Source compilation emits one CodeObject per body, shared by its instances.
        let factory = h.eval(b"[:held | [| held]]");
        let first = h.call(factory, b"value:", &[h.int(11)]);
        let second = h.call(factory, b"value:", &[h.int(29)]);
        assert_eq!(
            h.call(first, b"codeObject", &[]),
            h.call(second, b"codeObject", &[])
        );
        h.count(first, b"captureCount", 1);
        assert_eq!(h.number(h.call(first, b"captureAt:", &[h.int(0)])), 11);
        assert_eq!(h.number(h.call(second, b"value", &[])), 29);
        // Source Blocks share canonical code but keep their own Method behavior.
        // An edit survives eviction without changing another instance.
        let private = h.keep(behavior_of(&h, first));
        assert_ne!(behavior_of(&h, second), private);
        let original = binding(&h, private, b"value");
        let replacement = h.int(999);
        set(
            &h,
            private,
            b"value",
            NativeEntry::new(constant, replacement.cast()),
        );
        assert_eq!(h.call(first, b"value", &[]), replacement);
        assert_eq!(h.number(h.call(second, b"value", &[])), 29);
        h.call(codegen, b"evictMaterializations", &[]);
        h.collect();
        let third = h.call(factory, b"value:", &[h.int(37)]);
        assert_ne!(behavior_of(&h, third), private);
        assert_eq!(
            h.call(third, b"codeObject", &[]),
            h.call(first, b"codeObject", &[])
        );
        assert_eq!(h.call(first, b"value", &[]), replacement);
        assert_eq!(h.number(h.call(third, b"value", &[])), 37);
        set(&h, private, b"value", original);
        assert_eq!(h.number(h.call(first, b"value", &[])), 11);
        assert_eq!(h.number(h.call(second, b"value", &[])), 29);
        assert_eq!(h.number(h.call(third, b"value", &[])), 37);
        let capture_code = h.call(first, b"codeObject", &[]);
        for resumable in [false, true] {
            let held = h.vector(&[h.int(101)]);
            let block = h.materialize(capture_code, &[held], resumable);
            assert_eq!(h.call(block, b"value", &[]), held);
            assert_eq!(h.call(block, b"captureAt:", &[h.int(0)]), held);
            assert_eq!(h.call(block, b"captureAt:", &[h.int(-1)]), nil);
            assert_eq!(h.call(block, b"captureAt:", &[h.int(1)]), nil);
            let method = h.call(block, b"methodFor:", &[id.intern_name(b"value").unwrap()]);
            assert_eq!(h.call(method, b"invokeOn:", &[block]), held);
            for wrong_receiver in [nil, h.string(b"not a Block capture layout")] {
                let result = h.call(method, b"invokeOn:", &[wrong_receiver]);
                if resumable {
                    h.kind(result, b"nativeInvalidRequest");
                } else {
                    assert_eq!(result, nil);
                }
            }
            // Execution's language frame entry is variadic-tolerant (Design
            // 60E section 6); the synchronous entry rejects a count mismatch.
            let result = h.call(
                method,
                b"invokeOn:arguments:",
                &[block, h.vector(&[h.int(1)])],
            );
            assert_eq!(result, if resumable { held } else { nil });

        }
        // Root arity and nested captures work for both dispositions.
        let parameter = h.eval(b"[:x | [x + 7]]");
        let parameter_code = h.call(parameter, b"codeObject", &[]);
        for resumable in [false, true] {
            let block = h.materialize(parameter_code, &[], resumable);
            assert_eq!(h.number(h.call(block, b"value:", &[h.int(35)])), 42);
            let outer = h.materialize(h.call(factory, b"codeObject", &[]), &[], resumable);
            let inner = h.call(outer, b"value:", &[h.int(52)]);
            assert_eq!(h.number(h.call(inner, b"value", &[])), 52);
        }
        // Exact float payloads, binary strings and let bindings survive both emitters.
        for resumable in [false, true] {
            let nan = h.admit(include_bytes!(
                "../../../tests/fixtures/portable-code-v1/program-signalling-nan.bin"
            ));
            let block = h.materialize(nan, &[], resumable);
            let value = h.call(block, b"value", &[]);
            assert_eq!(
                send(id, h.named(b"Float"), b"bitsOf:", &[Operand::object(value)]).as_word(),
                Some(0x7ff0000000000042)
            );
            let binary = encode(ProgramSpec {
                parameters: &[],
                captures: &[],
                lets: &[],
                statements: &[StatementSpec::Expression { root_node: 0 }],
                nodes: &[NodeSpec::String(b"a\0\xffz")],
            })
            .unwrap();
            let block = h.materialize(h.admit(&binary), &[], resumable);
            assert_eq!(h.bytes(h.call(block, b"value", &[])), b"a\0\xffz");
            let lets = h.admit(include_bytes!(
                "../../../tests/fixtures/portable-code-v1/program-let-scope.bin"
            ));
            let block = h.materialize(lets, &[h.int(99)], resumable);
            assert_eq!(h.number(h.call(block, b"value:", &[h.int(88)])), 7);
        }
        // Retain the historical capture-read order around compilation. A live
        // Vector getter changes when TCC receives compile:; no owner is bypassed.
        let vector_vt = h.named(b"VectorVt");
        let tcc_vt = behavior_of(&h, h.named(b"TCC"));
        let old_at = binding(&h, vector_vt, b"at:");
        let old_compile = binding(&h, tcc_vt, b"compile:");
        set(&h, vector_vt, b"originalCaptureAt:", old_at);
        set(&h, tcc_vt, b"originalPortableCompile:", old_compile);
        for resumable in [false, true] {
            h.call(codegen, b"evictMaterializations", &[]);
            let before = h.int(12);
            let after = h.int(34);
            let captures = h.vector(&[before]);
            let mut timing = CaptureTiming {
                id,
                target: captures,
                before,
                after,
                compiled: Cell::new(false),
                original_at: h.keep(id.intern_name(b"originalCaptureAt:").unwrap()),
                original_compile: h.keep(id.intern_name(b"originalPortableCompile:").unwrap()),
            };
            let context = (&mut timing as *mut CaptureTiming).cast();
            set(&h, vector_vt, b"at:", NativeEntry::new(timed_at, context));
            set(
                &h,
                tcc_vt,
                b"compile:",
                NativeEntry::new(timed_compile, context),
            );
            let block = h.call(
                codegen,
                if resumable {
                    b"materializeResumable:captures:"
                } else {
                    b"materializeSynchronous:captures:"
                },
                &[capture_code, captures],
            );
            set(&h, vector_vt, b"at:", old_at);
            set(&h, tcc_vt, b"compile:", old_compile);
            assert!(timing.compiled.get());
            assert_eq!(
                h.call(block, b"value", &[]),
                if resumable { before } else { after }
            );
        }
        // Errors preserve the old nil/lastError protocol.
        assert_eq!(h.materialize(nil, &[], false), nil);
        assert_eq!(
            h.bytes(h.call(codegen, b"lastError", &[])),
            b"RustCodegen materialization needs an admitted CodeObject"
        );
        assert_eq!(h.materialize(capture_code, &[], true), nil);
        assert_eq!(
            h.bytes(h.call(codegen, b"lastError", &[])),
            b"RustCodegen resumable capture count disagrees with CodeObject"
        );
        h.materialize(code, &[], false);
        assert_eq!(h.call(codegen, b"lastError", &[]), nil);

        // An explicit active Machine distinguishes the synchronous barrier from
        // the resumable path. The latter can park and continue after eviction.
        let control = h.eval(b"[| [PortableControl park]; 73]");
        let control_code = h.call(control, b"codeObject", &[]);
        for resumable in [false, true] {
            let machine = h.call(h.named(b"Machine"), b"new", &[]);
            h.bind(b"PortableControl", machine);
            let block = h.materialize(control_code, &[], resumable);
            let result = id.send_named(machine, b"runBlock:", &[Operand::object(block)]);
            if resumable {
                assert_eq!(object(result.unwrap()), machine);
                h.call(codegen, b"evictMaterializations", &[]);
                h.collect();
                assert_eq!(h.number(h.call(machine, b"resumeWith:", &[nil])), 73);
            } else {
                // Native boundaries reject suspension, preserving the current
                // Execution failure rather than parking a synchronous caller.
                assert!(result.is_err());
            }
            h.bind(b"PortableControl", nil);
            h.call(machine, b"destroy", &[]);
        }

        // Dependencies may collect, including during generated initialization.
        let mut forwards = Vec::new();
        for (family, name) in [
            (b"BytesVt".as_slice(), b"fromBytes:".as_slice()),
            (b"CodeObjectVt", b"fromCanonicalBytes:"),
            (b"CodeObjectVt", b"canonicalBytesOf:"),
            (b"BlockVt", b"fromMethod:codeObject:captures:"),
            (b"BlockVt", b"captureAt:of:"),
            (b"IntegerVt", b"fromWord:"),
        ] {
            let vt = if family == b"BlockVt" {
                behavior_of(&h, h.named(b"Block"))
            } else {
                h.named(family)
            };
            let original = binding(&h, vt, name);
            let alias_name = [b"portableOriginal".as_slice(), name].concat();
            let alias = h.keep(id.intern_name(&alias_name).unwrap());
            set(&h, vt, &alias_name, original);
            let f = Box::into_raw(Box::new(Forward {
                id,
                gc: h.gc,
                alias,
                calls: Cell::new(0),
                fail: Cell::new(false),
            }));
            set(&h, vt, name, NativeEntry::new(forward, f.cast()));
            forwards.push((vt, name, original, f));
        }
        h.call(codegen, b"evictMaterializations", &[]);
        for resumable in [false, true] {
            let block = h.materialize(capture_code, &[h.int(91)], resumable);
            assert_eq!(h.number(h.call(block, b"value", &[])), 91);
        }
        for (_, _, _, f) in &forwards {
            assert!((**f).calls.get() > 0);
        }
        let canonical_forward = forwards
            .iter()
            .find(|(_, n, _, _)| *n == b"canonicalBytesOf:")
            .unwrap()
            .3;
        (*canonical_forward).fail.set(true);
        let captures = h.vector(&[]);
        let failure = id
            .send_named(
                codegen,
                b"materializeResumable:captures:",
                &[Operand::object(code), Operand::object(captures)],
            )
            .unwrap_err();
        assert_eq!((failure.status, failure.detail), (881, 73));
        (*canonical_forward).fail.set(false);
        for (vt, name, original, f) in forwards {
            set(&h, vt, name, original);
            drop(Box::from_raw(f));
        }

        // Failed admission/construction publishes no cache entry and releases
        // the compiled Unit after the temporary roots go away.
        h.call(codegen, b"evictMaterializations", &[]);
        h.clear_from(0);
        h.collect();
        assert_eq!(h.units(), 0);
        for (vt, selector) in [
            (h.named(b"CodeObjectVt"), b"fromCanonicalBytes:".as_slice()),
            (
                behavior_of(&h, h.named(b"Block")),
                b"fromMethod:codeObject:captures:",
            ),
        ] {
            let code = h.admit(fixture);
            let original = binding(&h, vt, selector);
            set(&h, vt, selector, NativeEntry::new(constant, nil.cast()));
            for resumable in [false, true] {
                assert_eq!(h.materialize(code, &[], resumable), nil);
                h.count(codegen, b"materializationCount", 0);
            }
            set(&h, vt, selector, original);
            h.clear_from(0);
            h.collect();
            assert_eq!(h.units(), 0);
        }
        // Full-byte/disposition cache is bounded and uses least-recently-used
        // eviction. Unit counts distinguish a hit from a replacement compile.
        for i in 0..64 {
            let block = h.materialize(h.admit(&literal_program(i)), &[], false);
            assert_eq!(h.number(h.call(block, b"value", &[])), i as usize);
            h.clear_from(0);
        }
        h.collect();
        h.count(codegen, b"materializationCount", 64);
        assert_eq!(h.units(), 64);
        h.materialize(h.admit(&literal_program(0)), &[], false); // make 0 most recent
        assert_eq!(h.units(), 64);
        h.materialize(h.admit(&literal_program(64)), &[], false); // evict 1
        h.clear_from(0);
        h.collect();
        assert_eq!(h.units(), 64);
        h.materialize(h.admit(&literal_program(0)), &[], false);
        assert_eq!(h.units(), 64); // 0 was retained
        h.materialize(h.admit(&literal_program(1)), &[], false);
        assert_eq!(h.units(), 65); // 1 needs recompilation; retired Unit awaits GC
        h.count(codegen, b"materializationCount", 64);

        // Only the nested Block survives: cache, outer Block and source Unit
        // references disappear. Its compiled Unit must survive until it does.
        h.call(codegen, b"evictMaterializations", &[]);
        h.clear_from(0);
        h.collect();
        let baseline = h.units();
        assert_eq!(baseline, 0);
        // A reflected Method owns anonymous code independently of its original
        // Block. Transfer that ownership to an alias on a different Block, then
        // remove the final binding. Repeat across source and both materializers.
        for disposition in 0..3 {
            for _ in 0..3 {
                let receiver = h.eval(b"[:argument | 19]");
                let receiver_token = retain(id, h.roots, receiver);
                h.call(codegen, b"evictMaterializations", &[]);
                h.clear_from(0);
                h.collect();
                let receiver_units = h.units();
                let receiver_bytes = h.native_bytes();
                assert!(receiver_units > 0 && receiver_bytes > 0);
                let source = h.eval(b"[:argument | argument]");
                let block = if disposition == 0 {
                    source
                } else {
                    let code = h.call(source, b"codeObject", &[]);
                    h.materialize(code, &[], disposition == 2)
                };
                let method = h.call(block, b"methodFor:", &[id.intern_name(b"value:").unwrap()]);
                let method_token = retain(id, h.roots, method);
                h.call(codegen, b"evictMaterializations", &[]);
                h.clear_from(0);
                h.collect();
                assert_eq!(h.units(), receiver_units + 1);
                assert!(h.native_bytes() > receiver_bytes);
                assert_eq!(
                    h.number(h.call(
                        method,
                        b"invokeOn:arguments:",
                        &[receiver, h.vector(&[h.int(73)])]
                    )),
                    73
                );
                let behavior = h.keep(behavior_of(&h, receiver));
                let alias = h.keep(id.intern_name(b"retainedMethodAlias:").unwrap());
                send(
                    id,
                    behavior,
                    b"addMethod:as:",
                    &[Operand::object(method), Operand::object(alias)],
                );
                h.clear_from(0);
                release(id, h.roots, method_token);
                h.collect();
                assert_eq!(h.units(), receiver_units + 1);
                assert_eq!(
                    h.number(h.call(receiver, b"retainedMethodAlias:", &[h.int(89)])),
                    89
                );
                assert_eq!(h.number(h.call(receiver, b"value:", &[h.int(89)])), 19);
                send(id, behavior, b"removeMethod:", &[Operand::object(alias)]);
                h.clear_from(0);
                h.collect();
                assert_eq!(h.units(), receiver_units);
                assert_eq!(h.native_bytes(), receiver_bytes);
                release(id, h.roots, receiver_token);
                h.collect();
                assert_eq!(h.units(), baseline);
                assert_eq!(h.native_bytes(), 0);
            }
        }
        for resumable in [false, true] {
            for cycle in 0..3 {
                let source = h.eval(b"[:held | [| held]]");
                let co = h.call(source, b"codeObject", &[]);
                let outer = h.materialize(co, &[], resumable);
                let cell = h.eval(b"[db cellFor: [Object clone] attr: #retainedMutableCapture]");
                h.call(cell, b"set:", &[h.int(87)]);
                let inner = h.call(outer, b"value:", &[cell]);
                let alias = h.call(inner, b"clone", &[]);
                assert_ne!(inner, alias);
                let token = retain(id, h.roots, inner);
                let alias_token = retain(id, h.roots, alias);
                h.call(codegen, b"evictMaterializations", &[]);
                h.clear_from(0);
                h.collect();
                assert_eq!(h.units(), baseline + 1);
                assert_eq!(h.call(inner, b"value", &[]), cell);
                assert_eq!(h.call(alias, b"value", &[]), cell);
                h.call(cell, b"set:", &[h.int(91 + cycle)]);
                assert_eq!(h.number(h.call(cell, b"value", &[])), 91 + cycle as usize);
                h.clear_from(0);
                release(id, h.roots, token);
                h.collect();
                // A direct clone alias alone still owns the native context and
                // the shared mutable capture after every cache has been evicted.
                assert_eq!(h.units(), baseline + 1);
                let held = h.call(alias, b"value", &[]);
                assert_eq!(held, cell);
                assert_eq!(h.number(h.call(held, b"value", &[])), 91 + cycle as usize);
                h.clear_from(0);
                release(id, h.roots, alias_token);
                h.collect();
                assert_eq!(h.units(), baseline);
            }
        }
    }
}
