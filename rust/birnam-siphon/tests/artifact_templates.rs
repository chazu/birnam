//! Supplied artifact code, independent bindings, activation lifetime and live policy.
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
    unsafe fn number(&self, n: Oop) -> usize {
        unsafe {
            send(
                self.id,
                self.named(b"Integer"),
                b"wordOf:",
                &[Operand::object(n)],
            )
            .as_word()
            .unwrap()
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

unsafe fn units(h: &H) -> usize {
    unsafe {
        send(h.id, h.named(b"TCC"), b"liveNativeUnits", &[])
            .as_word()
            .unwrap()
    }
}
unsafe fn collect_units(h: &H) {
    unsafe {
        collect(h.id, h.named(b"GC"));
        send(h.id, h.named(b"TCC"), b"checkpoint", &[]);
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
struct Probe {
    id: Protocol,
    gc: Oop,
    answer: Oop,
    calls: Cell<usize>,
    failure: bool,
}
unsafe extern "C" fn probe(c: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Probe>();
        p.calls.set(p.calls.get() + 1);
        collect(p.id, p.gc);
        if p.failure {
            Outcome::failed(881, 17)
        } else {
            Outcome::loaded(Operand::object(p.answer))
        }
    }
}

struct Reentry {
    id: Protocol,
    original: NativeEntry,
    reflect: Oop,
    target: Oop,
    selector: Oop,
    body: Oop,
    gc: Oop,
    calls: Cell<usize>,
}

#[test]
#[ignore = "production modules"]
fn supplied_artifact_templates_share_code_without_sharing_binding_state() {
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
        let baseline = std::env::var_os("BIRNAM_ARTIFACT_TEMPLATE_BASELINE").is_some();
        h.call(h.named(b"MethodInvocation"), b"install", &[]);
        let mut source = "[[| let target := [Object clone];\n".to_string();
        for i in 0..16 {
            source.push_str(&format!("[Reflect defineMethod: #answer{i}: on: target as: [:self :argument | #[self argument {i}]]];\n"));
        }
        source.truncate(source.len() - 2); // last definition is the artifact result
        source.push_str("\n] value]");
        let dir =
            std::env::temp_dir().join(format!("birnam-artifact-templates-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let codegen = h.named(b"Codegen");
        let tcc = h.named(b"TCC");
        let reflect = h.named(b"Reflect");
        let backend = h.named(b"NativeBackend");
        let backend_entry = lookup(&h, backend, b"named:synchronous:");
        let error = h.call(
            h.named(b"Error"),
            b"kind:",
            &[h.keep(id.intern_name(b"suppliedTemplateRejected").unwrap())],
        );
        let mut rejected = Box::new(Probe {
            answer: error,
            calls: Cell::new(0),
            gc: h.named(b"GC"),
            id,
            failure: false,
        });
        let reject = NativeEntry::new(probe, (&mut *rejected as *mut Probe).cast());
        for synchronous in [false, true] {
            let path = h.text(
                dir.join(if synchronous {
                    "sync.dylib"
                } else {
                    "resume.dylib"
                })
                .as_os_str()
                .as_bytes(),
            );
            let result = h.call(
                codegen,
                b"compile:output:from:synchronousBodies:",
                &[
                    h.text(source.as_bytes()),
                    path,
                    path,
                    h.named(if synchronous { b"true" } else { b"false" }),
                ],
            );
            assert_eq!(h.call(result, b"isError", &[]), h.named(b"false"));
            // Loading must use supplied code even when source compilation policy
            // is now unusable. The source template cache is irrelevant here.
            replace(&h, backend, b"named:synchronous:", reject);
            collect_units(&h);
            let before = units(&h);
            let bytes_before = send(id, tcc, b"liveNativeBytes", &[]).as_word();
            let started = std::time::Instant::now();
            let target = h.call(codegen, b"load:", &[path]);
            let elapsed = started.elapsed();
            if h.call(target, b"isError", &[]) == h.named(b"true") {
                let message = h.call(target, b"message", &[]);
                let bytes = send(id, message, b"bytes", &[]).as_bytes().unwrap();
                eprintln!(
                    "load error: {}",
                    String::from_utf8_lossy(std::slice::from_raw_parts(
                        bytes.pointer,
                        bytes.length
                    ))
                );
                panic!("artifact load failed");
            }
            let added = units(&h) - before;
            let bytes_after = send(id, tcc, b"liveNativeBytes", &[]).as_word();
            assert_eq!(added, if baseline || synchronous { 17 } else { 2 });
            assert_eq!(rejected.calls.get(), 0);
            let argument = h.call(h.named(b"Object"), b"clone", &[]);
            let mut methods = vec![];
            for i in 0..16 {
                let selector = format!("answer{i}:");
                let method = h.call(
                    target,
                    b"methodFor:",
                    &[h.keep(id.intern_name(selector.as_bytes()).unwrap())],
                );
                assert!(!methods.contains(&method));
                methods.push(method);
                let value = h.call(target, selector.as_bytes(), &[argument]);
                assert_eq!(h.call(value, b"at:", &[h.integer(0)]), target);
                assert_eq!(h.call(value, b"at:", &[h.integer(1)]), argument);
                assert_eq!(h.number(h.call(value, b"at:", &[h.integer(2)])), i);
            }
            collect_units(&h);
            let value = h.call(target, b"answer15:", &[argument]);
            assert_eq!(h.number(h.call(value, b"at:", &[h.integer(2)])), 15);
            replace(&h, backend, b"named:synchronous:", backend_entry);
            eprintln!(
                "artifact-template synchronous={synchronous} units={added} native_bytes={:?} elapsed_ns={}",
                bytes_before.zip(bytes_after).map(|(a, b)| b - a),
                elapsed.as_nanos()
            );
            if baseline || synchronous {
                continue;
            }
            let hook = lookup(&h, reflect, b"methodForTemplate:implementation:");
            replace(&h, reflect, b"methodForTemplate:implementation:", reject);
            assert_eq!(h.call(codegen, b"load:", &[path]), error);
            assert_eq!(rejected.calls.get(), 16);
            replace(&h, reflect, b"methodForTemplate:implementation:", hook);
            rejected.calls.set(0);
            // Reenter the very same adapter on its first load. Its nested
            // template wins the optional cache; the outer call is not replayed.
            let nested_target = h.call(h.named(b"Object"), b"clone", &[]);
            let nested_body = h.eval(b"[:self :argument | argument]");
            let mut nested = Box::new(Reentry {
                id,
                original: lookup(&h, tcc, b"loadObject:"),
                reflect,
                target: nested_target,
                selector: h.keep(id.intern_name(b"nested:").unwrap()),
                body: nested_body,
                gc: h.named(b"GC"),
                calls: Cell::new(0),
            });
            // The first load is the artifact object itself; activate reentry
            // only from its first adapter load (second loader call).
            replace(
                &h,
                tcc,
                b"loadObject:",
                NativeEntry::new(artifact_reenter, (&mut *nested as *mut Reentry).cast()),
            );
            let loaded = h.call(codegen, b"load:", &[path]);
            replace(&h, tcc, b"loadObject:", nested.original);
            assert_eq!(nested.calls.get(), 3);
            assert_eq!(h.call(nested_target, b"nested:", &[argument]), argument);
            let answer = h.call(loaded, b"answer7:", &[argument]);
            assert_eq!(h.number(h.call(answer, b"at:", &[h.integer(2)])), 7);
        }
        std::fs::remove_dir_all(dir).unwrap();
        image_selection(&h, baseline);
        image_body_demand(&h);
        h.clear(0);
    }
}
unsafe extern "C" fn artifact_reenter(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    unsafe {
        let p = &*c.cast::<Reentry>();
        let call = p.calls.get();
        p.calls.set(call + 1);
        if call == 1 {
            if let Err(error) = p.id.send_named(
                p.reflect,
                b"defineMethod:on:as:",
                &[
                    Operand::object(p.selector),
                    Operand::object(p.target),
                    Operand::object(p.body),
                ],
            ) {
                return error;
            }
            collect(p.id, p.gc);
        }
        p.original.method.unwrap()(p.original.context, r, a, n)
    }
}

struct Emitter<'a> {
    h: &'a H,
    original: NativeEntry,
    distinct: bool,
    calls: Cell<usize>,
}
unsafe extern "C" fn emit_selected(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<Emitter<'_>>();
        let result = p.original.method.unwrap()(p.original.context, r, a, n);
        if result.status != 0 {
            return result;
        }
        let source = p.h.keep(object(result.value));
        let bytes = send(p.h.id, source, b"bytes", &[]).as_bytes().unwrap();
        let mut code = std::slice::from_raw_parts(bytes.pointer, bytes.length).to_vec();
        let index = p.calls.get();
        p.calls.set(index + 1);
        // Observable selected payload difference at identical arity. The extra
        // exported symbol survives object emission; forwarding semantics agree.
        if p.distinct {
            code.extend_from_slice(
                format!("\nconst int selected_payload_{index} = {index};\n").as_bytes(),
            );
        }
        Outcome::loaded(Operand::object(p.h.text(&code)))
    }
}
unsafe fn image_selection(h: &H, baseline: bool) {
    use birnam_artifact_codec::{
        code_binding_inventory as b, materialization_selection as m, portable_inventory as p,
    };
    unsafe {
        let programs = p::encode_closure(&[]).unwrap();
        let bindings = b::encode(&[
            b::BindingSpec {
                target: b::Field::HeapRef(0),
                selector: b"first:",
                implementation: b::Field::HeapRef(1),
                provenance: b::Field::Nil,
            },
            b::BindingSpec {
                target: b::Field::HeapRef(0),
                selector: b"second:",
                implementation: b::Field::HeapRef(2),
                provenance: b::Field::Nil,
            },
        ])
        .unwrap();
        let selection =
            m::encode(&programs, &bindings, &[m::Disposition::Resumable; 2], &[]).unwrap();
        let owned = |bytes: &[u8]| {
            h.keep(object(send(
                h.id,
                h.named(b"Bytes"),
                b"fromBytes:",
                &[Operand::bytes(bytes)],
            )))
        };
        let inputs = [owned(&programs), owned(&bindings), owned(&selection)];
        let backend = h.named(b"NativeBackend");
        let codegen = h.named(b"Codegen");
        let original = lookup(h, backend, b"named:synchronous:");
        for distinct in [false, true] {
            for mapped in [false, true] {
                let mut emitter = Box::new(Emitter {
                    h,
                    original,
                    distinct,
                    calls: Cell::new(0),
                });
                replace(
                    h,
                    backend,
                    b"named:synchronous:",
                    NativeEntry::new(emit_selected, (&mut *emitter as *mut Emitter<'_>).cast()),
                );
                let bundle = if mapped {
                    let bundle = h.call(
                        codegen,
                        b"compilePortableImageCode:bindings:selection:",
                        &inputs,
                    );
                    assert_eq!(emitter.calls.get(), 2);
                    Some(bundle)
                } else {
                    None
                };
                collect_units(h);
                let before = units(h);
                let result = if let Some(bundle) = bundle {
                    h.call(
                        codegen,
                        b"beginPortableImageMapped:programs:bindings:selection:",
                        &[bundle, inputs[0], inputs[1], inputs[2]],
                    )
                } else {
                    h.call(
                        codegen,
                        b"beginPortableImageRebuild:bindings:selection:",
                        &inputs,
                    )
                };
                assert_eq!(result, h.named(b"true"));
                assert_eq!(emitter.calls.get(), 2); // no regeneration on mapped load
                replace(h, backend, b"named:synchronous:", original);
                collect_units(h);
                let added = units(h) - before;
                assert_eq!(added, if baseline || distinct { 2 } else { 1 });
                assert_eq!(
                    h.call(codegen, b"cancelPortableImageUnit", &[]),
                    h.named(b"true")
                );
                collect_units(h);
                assert_eq!(units(h), before); // staging owns no permanent cache
                eprintln!(
                    "image-template mapped={mapped} distinct_payload={distinct} units={added}"
                );
            }
        }
    }
}

struct DemandProbe<'a> {
    h: &'a H,
    original: NativeEntry,
    calls: Cell<usize>,
    answer: Cell<Option<Oop>>,
}
unsafe extern "C" fn demand_probe(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        let p = &*c.cast::<DemandProbe<'_>>();
        p.calls.set(p.calls.get() + 1);
        // Neither GC root enumeration nor a rejected nested operation may find
        // a held RefCell borrow or destroy the outer request's pending input.
        collect_units(p.h);
        let nested = p.h.id.send_named(p.h.named(b"Codegen"), b"cancelPortableImageUnit", &[]);
        assert!(nested.is_err());
        if let Some(answer) = p.answer.get() {
            Outcome::loaded(Operand::object(answer))
        } else {
            p.original.method.unwrap()(p.original.context, r, a, n)
        }
    }
}
unsafe fn image_body_demand(h: &H) {
    use birnam_artifact_codec::{
        code_binding_inventory as b, materialization_selection as m, portable_inventory as p,
    };
    unsafe {
        let programs = p::encode_closure(&[include_bytes!("../../../tests/fixtures/portable-code-v1/program-empty.bin")]).unwrap();
        let bindings = b::encode(&[]).unwrap();
        let selection = m::encode(&programs, &bindings, &[], &[m::Disposition::Resumable]).unwrap();
        let owned = |bytes: &[u8]| h.keep(object(send(h.id, h.named(b"Bytes"), b"fromBytes:", &[Operand::bytes(bytes)])));
        let inputs = [owned(&programs), owned(&bindings), owned(&selection)];
        let codegen = h.named(b"Codegen");
        let tcc = h.named(b"TCC");
        let backend = h.named(b"NativeBackend");
        let emit_selector = b"emit:source:filename:persistent:synchronous:";
        let emitter = lookup(h, backend, emit_selector);
        let error = h.call(h.named(b"Error"), b"kind:message:", &[h.keep(h.id.intern_name(b"demandRejected").unwrap()), h.text(b"demand probe rejected")]);
        let fake_unit = h.eval(b"[Object clone]");
        let mut initializer = Box::new(Probe { id: h.id, gc: h.named(b"GC"), answer: h.named(b"nil"), calls: Cell::new(0), failure: false });
        replace(h, fake_unit, b"initialize", NativeEntry::new(probe, (&mut *initializer as *mut Probe).cast()));
        // Saving still emits both variants, so the supplied bundle serves either
        // future request without source compilation or a new image format.
        let bundle = h.call(codegen, b"compilePortableImageCode:bindings:selection:", &inputs);
        for mapped in [true, false] {
            let loader_selector: &[u8] = if mapped { b"loadObject:" } else { b"compile:" };
            let loader = lookup(h, tcc, loader_selector);
            let mut emit_probe = Box::new(DemandProbe { h, original: emitter, calls: Cell::new(0), answer: Cell::new(None) });
            let mut load_probe = Box::new(DemandProbe { h, original: loader, calls: Cell::new(0), answer: Cell::new(None) });
            replace(h, backend, emit_selector, NativeEntry::new(demand_probe, (&mut *emit_probe as *mut DemandProbe<'_>).cast()));
            collect_units(h);
            let before = units(h);
            let mark = h.tokens.borrow().len();
            let begin = || if mapped {
                h.call(codegen, b"beginPortableImageMapped:programs:bindings:selection:", &[bundle, inputs[0], inputs[1], inputs[2]])
            } else { h.call(codegen, b"beginPortableImageRebuild:bindings:selection:", &inputs) };
            assert_eq!(begin(), h.named(b"true"));
            // Replace after stage creation: first demand must resolve the live operation.
            replace(h, tcc, loader_selector, NativeEntry::new(demand_probe, (&mut *load_probe as *mut DemandProbe<'_>).cast()));
            collect_units(h);
            assert_eq!(units(h), before);
            assert_eq!(emit_probe.calls.get(), 0);
            assert_eq!(load_probe.calls.get(), 0);
            // A source rejection is observed on demand and leaves a retryable stage.
            let index = h.integer(0);
            let resumable = [index, h.named(b"false")];
            if !mapped {
                emit_probe.answer.set(Some(error));
                assert_eq!(h.call(codegen, b"portableImageBlockMethod:synchronous:", &resumable), h.named(b"nil"));
                assert_eq!(load_probe.calls.get(), 0);
                emit_probe.answer.set(None);
            }
            for answer in [error, fake_unit] {
                load_probe.answer.set(Some(answer));
                let result = h.call(codegen, b"portableImageBlockMethod:synchronous:", &resumable);
                assert_eq!(result, if answer == error { error } else { h.named(b"nil") });
                collect_units(h);
                assert_eq!(units(h), before);
            }
            load_probe.answer.set(Some(fake_unit));
            initializer.failure = true;
            let failed = h.id.send_named(codegen, b"portableImageBlockMethod:synchronous:",
                &resumable.map(Operand::object)).unwrap_err();
            assert_eq!((failed.status, failed.detail), (881, 17));
            initializer.failure = false;
            load_probe.answer.set(None);
            // Leave the first result protected only by the image stage while
            // collecting, then retain it through the repeated request below.
            let method = object(send(h.id, codegen, b"portableImageBlockMethod:synchronous:", &resumable.map(Operand::object)));
            let calls = load_probe.calls.get();
            let emitted = emit_probe.calls.get();
            collect_units(h);
            assert_eq!(units(h), before + 1);
            assert_eq!(h.call(codegen, b"portableImageBlockMethod:synchronous:", &resumable), method);
            assert_eq!(load_probe.calls.get(), calls);
            assert_eq!(emit_probe.calls.get(), emitted);
            // The other disposition remains independently available. A current
            // replacement is used for its first request, never a saved binding.
            load_probe.answer.set(Some(error));
            let synchronous = [index, h.named(b"true")];
            assert_eq!(h.call(codegen, b"portableImageBlockMethod:synchronous:", &synchronous), error);
            load_probe.answer.set(None);
            let sync = h.call(codegen, b"portableImageBlockMethod:synchronous:", &synchronous);
            assert_ne!(sync, method);
            assert_eq!(emit_probe.calls.get(), if mapped { 0 } else { load_probe.calls.get() + 1 });
            assert!(h.id.send_named(codegen, b"portableImageBlockMethod:synchronous:", &[Operand::object(h.integer(1)), Operand::object(h.named(b"false"))]).is_err());
            assert_eq!(h.call(codegen, b"cancelPortableImageUnit", &[]), h.named(b"true"));
            collect_units(h);
            assert_eq!(units(h), before + 2); // returned Methods retain their Units
            for method in [method, sync] {
                let captures = h.call(h.named(b"Vector"), b"new", &[]);
                let block = h.call(h.named(b"Block"), b"fromMethod:captures:", &[method, captures]);
                let machine = h.call(h.named(b"Machine"), b"new", &[]);
                let answer = h.call(machine, b"runBlock:", &[block]);
                assert_eq!(answer, h.named(b"nil"));
            }
            h.clear(mark);
            collect_units(h);
            assert_eq!(units(h), before);
            // Cancelling before any demand maps no body at all.
            assert_eq!(begin(), h.named(b"true"));
            assert_eq!(h.call(codegen, b"cancelPortableImageUnit", &[]), h.named(b"true"));
            collect_units(h);
            assert_eq!(units(h), before);
            replace(h, backend, emit_selector, emitter);
            replace(h, tcc, loader_selector, loader);
        }
        // Bundle framing is admitted up front; a native-object rejection is
        // deferred until that body is requested and never cached as a Method.
        let carrier = send(h.id, bundle, b"bytes", &[]).as_bytes().unwrap();
        let mut corrupt = birnam_abi::borrowed_bytes(carrier.pointer, carrier.length).unwrap().to_vec();
        let first_len = u64::from_le_bytes(corrupt[44..52].try_into().unwrap()) as usize;
        corrupt[52 + first_len + 8] = 0; // second ELF header, synchronous body
        let corrupt = owned(&corrupt);
        let mark = h.tokens.borrow().len();
        collect_units(h);
        let before = units(h);
        assert_eq!(h.call(codegen, b"beginPortableImageMapped:programs:bindings:selection:", &[corrupt, inputs[0], inputs[1], inputs[2]]), h.named(b"true"));
        let index = h.integer(0);
        let method = h.call(codegen, b"portableImageBlockMethod:synchronous:", &[index, h.named(b"false")]);
        assert_ne!(method, h.named(b"nil"));
        for _ in 0..2 {
            let rejected = h.call(codegen, b"portableImageBlockMethod:synchronous:", &[index, h.named(b"true")]);
            assert_eq!(h.call(rejected, b"isError", &[]), h.named(b"true"));
        }
        h.call(codegen, b"cancelPortableImageUnit", &[]);
        h.clear(mark);
        collect_units(h);
        assert_eq!(units(h), before);
        let fake_behavior = h.call(h.id.receiver, b"behaviorOf:", &[fake_unit]);
        send(h.id, fake_behavior, b"removeMethod", &[Operand::object(h.id.intern_name(b"initialize").unwrap())]);
    }
}
