//! Source and source-less Block materialization through production Oop owners.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use birnam_artifact_codec::portable_program::{NodeSpec, ProgramSpec, StatementSpec, encode};
use birnam_host_support::Context;
use std::{cell::RefCell, path::Path};
struct H<'a> {
    host: &'a Context,
    roots: Oop,
    gc: Oop,
    retain_key: Oop,
    release_key: Oop,
    tokens: RefCell<Vec<Oop>>,
    hooks: oop::hooks::Hooks<'a>,
    fixture: Oop,
    floor_units: usize,
    floor_bytes: usize,
}
impl H<'_> {
    fn named(&self, n: &[u8]) -> Oop {
        self.host.named(n).unwrap()
    }
    fn key(&self, n: &[u8]) -> Oop {
        self.hooks.symbol(n)
    }
    fn retain(&self, v: Oop) -> Oop {
        unsafe { self.host.id.send(self.roots, self.retain_key, &[v]) }.unwrap()
    }
    fn release(&self, t: Oop) {
        unsafe { self.host.id.send(self.roots, self.release_key, &[t]) }.unwrap();
    }
    fn keep(&self, v: Oop) -> Oop {
        let token = self.retain(v);
        self.tokens.borrow_mut().push(token);
        v
    }
    fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        self.keep(self.host.send(r, n, a).unwrap())
    }
    fn bits(&self, v: Oop) -> usize {
        self.host.scope().word_bits(v).unwrap()
    }
    fn word(&self, n: usize) -> Oop {
        if let Some(v) = size(n) {
            v
        } else {
            self.call(
                self.named(b"Word"),
                b"fromUnsignedLow:high:",
                &[size(n & 0xffffffff).unwrap(), size(n >> 32).unwrap()],
            )
        }
    }
    fn int(&self, n: i64) -> Oop {
        self.call(
            self.named(b"Integer"),
            b"fromWord:",
            &[self.word(n as usize)],
        )
    }
    fn number(&self, v: Oop) -> usize {
        self.bits(self.call(self.named(b"Integer"), b"wordOf:", &[v]))
    }
    fn encoded(&self, b: &[u8]) -> Oop {
        let values = b
            .iter()
            .map(|&v| size(v as usize).unwrap())
            .collect::<Vec<_>>();
        let input = self.call(self.host.id.root, b"sequence", &values);
        self.call(self.named(b"Bytes"), b"fromBytes:", &[input])
    }
    fn string(&self, b: &[u8]) -> Oop {
        self.call(self.named(b"String"), b"fromBytes:", &[self.encoded(b)])
    }
    fn bytes(&self, v: Oop) -> Vec<u8> {
        self.host
            .scope()
            .read_bytes(self.call(v, b"bytes", &[]))
            .unwrap()
    }
    fn bind(&self, n: &[u8], v: Oop) {
        self.call(self.host.namespace, b"at:put:", &[self.key(n), v]);
    }
    fn clear_from(&self, i: usize) {
        let tokens = self.tokens.borrow_mut().split_off(i);
        for token in tokens {
            self.release(token);
        }
    }
    fn vector(&self, values: &[Oop]) -> Oop {
        let mut v = self.call(self.named(b"Vector"), b"new", &[]);
        for &x in values {
            v = self.call(v, b"add:", &[x]);
        }
        v
    }
    fn kind(&self, r: Oop, k: &[u8]) {
        let actual = self.call(r, b"kind", &[]);
        assert_eq!(
            actual,
            self.key(k),
            "actual kind: {}",
            String::from_utf8_lossy(&self.bytes(self.call(actual, b"printString", &[])))
        );
    }
    fn count(&self, r: Oop, n: &[u8], c: usize) {
        assert_eq!(self.number(self.call(r, n, &[])), c);
    }
    fn eval(&self, s: &[u8]) -> Oop {
        let v = self.call(self.named(b"Codegen"), b"eval:", &[self.string(s)]);
        self.host.scope().check_result(v).unwrap();
        v
    }
    fn admit(&self, b: &[u8]) -> Oop {
        self.call(
            self.named(b"CodeObject"),
            b"fromCanonicalBytes:",
            &[self.encoded(b)],
        )
    }
    fn materialize(&self, c: Oop, captures: &[Oop], resumable: bool) -> Oop {
        self.call(
            self.named(b"Codegen"),
            if resumable {
                b"materializeResumable:captures:"
            } else {
                b"materializeSynchronous:captures:"
            },
            &[c, self.vector(captures)],
        )
    }
    fn units(&self) -> usize {
        as_size(self.call(self.named(b"TCC"), b"liveNativeUnits", &[]))
            .unwrap()
            .checked_sub(self.floor_units)
            .expect("compiled core units remain live")
    }
    fn native_bytes(&self) -> usize {
        as_size(self.call(self.named(b"TCC"), b"liveNativeBytes", &[]))
            .unwrap()
            .checked_sub(self.floor_bytes)
            .expect("compiled core bytes remain live")
    }
    fn collect(&self) {
        self.call(self.gc, b"collectPrecisely", &[]);
        self.call(self.named(b"TCC"), b"checkpoint", &[]);
    }
    fn method(&self, n: &[u8], c: Oop) -> Oop {
        self.call(
            self.call(self.fixture, b"methodAt:", &[self.key(n)]),
            b"withContext:",
            &[c],
        )
    }
}
impl Drop for H<'_> {
    fn drop(&mut self) {
        self.clear_from(0);
    }
}
fn binding(h: &H<'_>, v: Oop, n: &[u8]) -> Oop {
    h.call(v, b"lookup", &[h.key(n)])
}
fn set(h: &H<'_>, v: Oop, n: &[u8], m: Oop) {
    h.call(v, b"addMethod", &[h.key(n), m]);
}
fn behavior_of(h: &H<'_>, v: Oop) -> Oop {
    h.call(h.host.id.root, b"behaviorOf:", &[v])
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
#[test]
#[ignore = "requires production Oop modules with TCC resource metrics"]
fn portable_materialization_preserves_identity_control_and_lifetime() {
    let host = oop::bootstrap();
    let floor = host.scope();
    let gc = host.named(b"GC").unwrap();
    floor
        .call(gc, b"precisePayloads:", &[size(1).unwrap()])
        .unwrap();
    host.global(&floor, b"Module").unwrap();
    let lang = host.named(b"Lang").unwrap();
    for n in [
        b"buildGrammar".as_slice(),
        b"initializeGrammar",
        b"grammar",
        b"parse:",
        b"furthestFail",
    ] {
        floor
            .call(host.id.root, b"bind:to:", &[host.intern(n).unwrap(), lang])
            .unwrap();
    }
    floor.call(gc, b"collectPrecisely", &[]).unwrap();
    let tcc = host.named(b"TCC").unwrap();
    floor.call(tcc, b"checkpoint", &[]).unwrap();
    let floor_units = as_size(floor.call(tcc, b"liveNativeUnits", &[]).unwrap()).unwrap();
    let floor_bytes = as_size(floor.call(tcc, b"liveNativeBytes", &[]).unwrap())
        .expect("TCC resource-metrics is required");
    let path = std::env::temp_dir().join(format!(
        "birnam-materialization-full-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/materialization.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = host.load_native(&floor, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let h = H {
        host: &host,
        roots: host.roots,
        gc,
        retain_key: host.intern(b"retain:").unwrap(),
        release_key: host.intern(b"release:").unwrap(),
        tokens: RefCell::new(vec![]),
        hooks: oop::hooks::Hooks::new(&host),
        fixture,
        floor_units,
        floor_bytes,
    };
    let nil = h.named(b"nil");
    let codegen = h.named(b"Codegen");
    // Portable source and owned capture storage are independent.
    let plain_method = h.hooks.method(b"fixed", nil);
    let plain_block = h.call(
        h.named(b"Block"),
        b"fromMethod:captures:",
        &[plain_method, h.vector(&[])],
    );
    for plain in [h.named(b"Block"), plain_block] {
        assert_eq!(h.call(plain, b"codeObject", &[]), nil);
        assert_eq!(h.call(plain, b"captureAt:", &[h.int(0)]), nil);
    }
    assert_eq!(h.call(h.named(b"Block"), b"captureCount", &[]), nil);
    assert_eq!(h.number(h.call(plain_block, b"captureCount", &[])), 0);
    let marker = h.call(h.named(b"Object"), b"clone", &[]);
    let native_captures = h.call(
        h.named(b"Block"), b"fromMethod:captures:",
        &[plain_method, h.vector(&[marker])],
    );
    h.collect();
    assert_eq!(h.call(native_captures, b"codeObject", &[]), nil);
    assert_eq!(h.number(h.call(native_captures, b"captureCount", &[])), 1);
    assert_eq!(h.call(native_captures, b"captureAt:", &[h.int(0)]), marker);
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
        h.hooks.method(b"fixed", nil),
    );
    assert_eq!(
        h.bytes(h.call(code_factory, b"canonicalBytesOf:", &[code])),
        fixture
    );
    // Bootstrap and authored error construction can materialize ordinary
    // programs too. Measure this fixture from an empty cache.
    h.call(codegen, b"evictMaterializations", &[]);
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
    let private = h.call(h.host.id.root, b"ensurePrivateVtableFor:", &[first]);
    assert_ne!(behavior_of(&h, second), private);
    let original = binding(&h, private, b"value");
    let replacement = h.int(999);
    set(&h, private, b"value", h.hooks.method(b"fixed", replacement));
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
        let boxed_index = h.call(h.named(b"Word"), b"fromBits:", &[h.word(0)]);
        assert_eq!(h.call(block, b"captureWordAt:", &[boxed_index]), held);
        assert_eq!(
            h.call(block, b"captureAt:", &[h.string(b"not an index")]),
            nil
        );
        let method = h.call(block, b"methodFor:", &[h.key(b"value")]);
        assert_eq!(h.call(method, b"invokeOn:", &[block]), held);
        for wrong_receiver in [nil, h.string(b"not a Block capture layout")] {
            if resumable {
                // Oop invocation preserves the native return; a language Machine
                // converts it at its own pending call, without another wrapper.
                assert_eq!(
                    unsafe {
                        h.host
                            .id
                            .send(method, h.key(b"invokeOn:"), &[wrong_receiver])
                    }
                    .unwrap_err(),
                    birnam_abi::object_reference(h.host.id.invalid)
                );
                h.bind(b"PortableInspectMethod", method);
                h.bind(b"PortableInspectReceiver", wrong_receiver);
                let result = h.call(
                    codegen,
                    b"eval:",
                    &[h.string(b"[PortableInspectMethod invokeOn: PortableInspectReceiver]")],
                );
                h.kind(result, b"nativeInvalidRequest");
                h.bind(b"PortableInspectMethod", nil);
                h.bind(b"PortableInspectReceiver", nil);
            } else {
                assert_eq!(h.call(method, b"invokeOn:", &[wrong_receiver]), nil);
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
    // Error data in a valid capture must not be mistaken for a refused read.
    let captured_error = h.call(h.named(b"Error"), b"kind:", &[h.key(b"doesNotUnderstand")]);
    for resumable in [false, true] {
        let block = h.materialize(capture_code, &[captured_error], resumable);
        assert_eq!(h.call(block, b"value", &[]), captured_error);
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
            h.bits(h.call(h.named(b"Float"), b"bitsOf:", &[value])),
            0x7ff0000000000042
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
        let timing = h.hooks.context(&[
            captures,
            before,
            after,
            size(0).unwrap(),
            h.key(b"originalCaptureAt:"),
            h.key(b"originalPortableCompile:"),
        ]);
        set(&h, vector_vt, b"at:", h.method(b"timedAt", timing));
        set(&h, tcc_vt, b"compile:", h.method(b"timedCompile", timing));
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
        assert_eq!(h.hooks.count(timing, 3), 1);
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
        let result = h.host.send(machine, b"runBlock:", &[block]);
        if resumable {
            assert_eq!(result.unwrap(), machine);
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
        (b"BlockVt", b"captureWordAt:"),
        (b"BlockVt", b"captureWord:of:"),
        (b"IntegerVt", b"fromWord:"),
    ] {
        let vt = if family == b"BlockVt" {
            behavior_of(&h, h.named(b"Block"))
        } else {
            h.named(family)
        };
        let original = binding(&h, vt, name);
        let alias_name = [b"portableOriginal".as_slice(), name].concat();
        let alias = h.keep(h.key(&alias_name));
        set(&h, vt, &alias_name, original);
        let state = h
            .hooks
            .context(&[h.gc, alias, size(0).unwrap(), std::ptr::null_mut()]);
        set(&h, vt, name, h.method(b"forward", state));
        forwards.push((vt, name, original, state));
    }
    h.call(codegen, b"evictMaterializations", &[]);
    for resumable in [false, true] {
        let block = h.materialize(capture_code, &[h.int(91)], resumable);
        assert_eq!(h.number(h.call(block, b"value", &[])), 91);
    }
    for (_, _, _, f) in &forwards {
        assert!(h.hooks.count(*f, 2) > 0);
    }
    let canonical_forward = forwards
        .iter()
        .find(|(_, n, _, _)| *n == b"canonicalBytesOf:")
        .unwrap()
        .3;
    let fault = h.hooks.fault(73);
    h.hooks.put(canonical_forward, 3, fault);
    let captures = h.vector(&[]);
    assert_eq!(
        unsafe {
            h.host.id.send(
                codegen,
                h.key(b"materializeResumable:captures:"),
                &[code, captures],
            )
        }
        .unwrap_err(),
        fault
    );
    h.hooks.put(canonical_forward, 3, std::ptr::null_mut());
    for (vt, name, original, _) in forwards {
        set(&h, vt, name, original);
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
        set(&h, vt, selector, h.hooks.method(b"fixed", nil));
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
            let receiver_token = h.retain(receiver);
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
            let method = h.call(block, b"methodFor:", &[h.key(b"value:")]);
            let method_token = h.retain(method);
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
            let behavior = h.call(h.host.id.root, b"ensurePrivateVtableFor:", &[receiver]);
            let alias = h.keep(h.key(b"retainedMethodAlias:"));
            h.host
                .send(behavior, b"addMethod:as:", &[method, alias])
                .unwrap();
            h.clear_from(0);
            h.release(method_token);
            h.collect();
            assert_eq!(h.units(), receiver_units + 1);
            assert_eq!(
                h.number(h.call(receiver, b"retainedMethodAlias:", &[h.int(89)])),
                89
            );
            assert_eq!(h.number(h.call(receiver, b"value:", &[h.int(89)])), 19);
            h.host.send(behavior, b"removeMethod:", &[alias]).unwrap();
            h.clear_from(0);
            h.collect();
            assert_eq!(h.units(), receiver_units);
            assert_eq!(h.native_bytes(), receiver_bytes);
            h.release(receiver_token);
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
            let cell = h.eval(b"[Cell with: nil]");
            h.call(cell, b"set:", &[h.int(87)]);
            let inner = h.call(outer, b"value:", &[cell]);
            let alias = h.call(inner, b"clone", &[]);
            assert_ne!(inner, alias);
            let token = h.retain(inner);
            let alias_token = h.retain(alias);
            h.call(codegen, b"evictMaterializations", &[]);
            h.clear_from(0);
            h.collect();
            assert_eq!(h.units(), baseline + 1);
            assert_eq!(h.call(inner, b"value", &[]), cell);
            assert_eq!(h.call(alias, b"value", &[]), cell);
            h.call(cell, b"set:", &[h.int(91 + cycle)]);
            assert_eq!(h.number(h.call(cell, b"value", &[])), 91 + cycle as usize);
            h.clear_from(0);
            h.release(token);
            h.collect();
            // A direct clone alias alone still owns the native context and
            // the shared mutable capture after every cache has been evicted.
            assert_eq!(h.units(), baseline + 1);
            let held = h.call(alias, b"value", &[]);
            assert_eq!(held, cell);
            assert_eq!(h.number(h.call(held, b"value", &[])), 91 + cycle as usize);
            h.clear_from(0);
            h.release(alias_token);
            h.collect();
            assert_eq!(h.units(), baseline);
        }
    }
}
