//! Artifact bodies, independent bindings, image staging and live owner policy through Oops.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use birnam_host_support::Context;
use std::{cell::RefCell, os::unix::ffi::OsStrExt, path::Path, ptr};
struct H<'a> {
    host: &'a Context,
    roots: Oop,
    retain: Oop,
    release: Oop,
    tokens: RefCell<Vec<Oop>>,
    hooks: oop::hooks::Hooks<'a>,
    fixture: Oop,
}
impl H<'_> {
    fn named(&self, n: &[u8]) -> Oop {
        self.host.named(n).unwrap()
    }
    fn key(&self, n: &[u8]) -> Oop {
        self.hooks.symbol(n)
    }
    fn keep(&self, v: Oop) -> Oop {
        let token = unsafe { self.host.id.send(self.roots, self.retain, &[v]) }.unwrap();
        self.tokens.borrow_mut().push(token);
        v
    }
    fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        self.keep(self.host.send(r, n, a).unwrap())
    }
    fn integer(&self, n: usize) -> Oop {
        self.call(self.named(b"Integer"), b"fromWord:", &[size(n).unwrap()])
    }
    fn number(&self, v: Oop) -> usize {
        as_size(self.call(v, b"word", &[])).unwrap()
    }
    fn bytes(&self, b: &[u8]) -> Oop {
        let values = b
            .iter()
            .map(|&x| size(x as usize).unwrap())
            .collect::<Vec<_>>();
        let provider = self.call(self.host.id.root, b"sequence", &values);
        self.call(self.named(b"Bytes"), b"fromBytes:", &[provider])
    }
    fn text(&self, b: &[u8]) -> Oop {
        self.call(self.named(b"String"), b"fromBytes:", &[self.bytes(b)])
    }
    fn read(&self, v: Oop) -> Vec<u8> {
        self.host.scope().read_bytes(v).unwrap()
    }
    fn eval(&self, b: &[u8]) -> Oop {
        let v = self.call(self.named(b"Codegen"), b"eval:", &[self.text(b)]);
        self.host.scope().check_result(v).unwrap();
        v
    }
    fn mark(&self) -> usize {
        self.tokens.borrow().len()
    }
    fn clear(&self, mark: usize) {
        let tokens = self.tokens.borrow_mut().split_off(mark);
        for token in tokens.into_iter().rev() {
            unsafe { self.host.id.send(self.roots, self.release, &[token]) }.unwrap();
        }
    }
    fn units(&self) -> usize {
        as_size(self.call(self.named(b"TCC"), b"liveNativeUnits", &[])).unwrap()
    }
    fn native_bytes(&self) -> Option<usize> {
        let tcc = self.named(b"TCC");
        if self.call(tcc, b"methodFor:", &[self.key(b"liveNativeBytes")]) == self.named(b"nil") {
            None
        } else {
            Some(as_size(self.call(tcc, b"liveNativeBytes", &[])).unwrap())
        }
    }
    fn collect(&self) {
        self.call(self.named(b"GC"), b"collectPrecisely", &[]);
        self.call(self.named(b"TCC"), b"checkpoint", &[]);
    }
    fn lookup(&self, r: Oop, n: &[u8]) -> Oop {
        self.call(self.host.id.root, b"bind:to:", &[self.key(n), r])
    }
    fn replace(&self, r: Oop, n: &[u8], m: Oop) {
        let vt = self.call(self.host.id.root, b"ensurePrivateVtableFor:", &[r]);
        self.call(vt, b"addMethod", &[self.key(n), m]);
    }
    fn alias(&self, r: Oop, name: &[u8], alias: &[u8]) -> Oop {
        let m = self.lookup(r, name);
        self.replace(r, alias, m);
        m
    }
    fn restore(&self, r: Oop, name: &[u8], alias: &[u8], m: Oop) {
        self.replace(r, name, m);
        let vt = self.call(self.host.id.root, b"behaviorOf:", &[r]);
        self.call(vt, b"removeMethod:", &[self.key(alias)]);
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
        self.clear(0);
    }
}
#[test]
#[ignore = "requires production Oop modules"]
fn supplied_artifact_templates_share_code_without_sharing_binding_state() {
    let host = oop::bootstrap();
    let roots = host.named(b"Roots").unwrap();
    let scope = host.scope();
    let path = std::env::temp_dir().join(format!(
        "birnam-artifact-templates-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/artifact-templates.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let fixture = host.load_native(&scope, &path, &[]).unwrap();
    std::fs::remove_file(path).unwrap();
    let h = H {
        host: &host,
        roots,
        retain: host.intern(b"retain:").unwrap(),
        release: host.intern(b"release:").unwrap(),
        tokens: RefCell::new(vec![]),
        hooks: oop::hooks::Hooks::new(&host),
        fixture,
    };
    h.call(h.named(b"GC"), b"precisePayloads:", &[size(1).unwrap()]);
    h.call(h.named(b"MethodInvocation"), b"install", &[]);
    let mut source = "[[| let target := [Object clone];\n".to_string();
    for i in 0..16 {
        source.push_str(&format!("[Reflect defineMethod: #answer{i}: on: target as: [:self :argument | #[self argument {i}]]];\n"));
    }
    source.truncate(source.len() - 2);
    source.push_str("\n] value]");
    let dir =
        std::env::temp_dir().join(format!("birnam-artifact-templates-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let codegen = h.named(b"Codegen");
    let tcc = h.named(b"TCC");
    let reflect = h.named(b"Reflect");
    let backend = h.named(b"NativeBackend");
    let gc = h.named(b"GC");
    let emit = b"emit:source:filename:persistent:synchronous:";
    let original = h.lookup(backend, emit);
    let error = h.call(
        h.named(b"Error"),
        b"kind:",
        &[h.key(b"suppliedTemplateRejected")],
    );
    let rejected = h
        .hooks
        .context(&[gc, error, size(0).unwrap(), ptr::null_mut()]);
    let reject = h.method(b"probe", rejected);
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
        // Saved body code loads with source emission disabled. Binding code is declared, not recompiled.
        h.hooks.put(rejected, 2, size(0).unwrap());
        h.replace(backend, emit, reject);
        h.collect();
        let before = h.units();
        let bytes_before = h.native_bytes();
        let started = std::time::Instant::now();
        let target = h.call(codegen, b"load:", &[path]);
        let elapsed = started.elapsed();
        let bytes_after = h.native_bytes();
        host.scope().check_result(target).unwrap();
        assert_eq!(h.units() - before, 1);
        assert_eq!(h.hooks.count(rejected, 2), 0);
        let argument = h.call(h.named(b"Object"), b"clone", &[]);
        let mut methods = vec![];
        for i in 0..16 {
            let selector = format!("answer{i}:");
            let m = h.call(target, b"methodFor:", &[h.key(selector.as_bytes())]);
            assert!(!methods.contains(&m));
            methods.push(m);
            let value = h.call(target, selector.as_bytes(), &[argument]);
            assert_eq!(h.call(value, b"at:", &[h.integer(0)]), target);
            assert_eq!(h.call(value, b"at:", &[h.integer(1)]), argument);
            assert_eq!(h.number(h.call(value, b"at:", &[h.integer(2)])), i);
        }
        h.collect();
        assert_eq!(
            h.number(h.call(
                h.call(target, b"answer15:", &[argument]),
                b"at:",
                &[h.integer(2)]
            )),
            15
        );
        h.replace(backend, emit, original);
        // Artifact execution still observes the current public construction policy.
        let hook = h.lookup(reflect, b"methodForArity:implementation:");
        h.replace(reflect, b"methodForArity:implementation:", reject);
        assert_eq!(h.call(codegen, b"load:", &[path]), error);
        assert_eq!(h.hooks.count(rejected, 2), 16);
        h.replace(reflect, b"methodForArity:implementation:", hook);
        let nested_target = h.call(h.named(b"Object"), b"clone", &[]);
        let body = h.eval(b"[:self :argument | argument]");
        let loader = h.alias(tcc, b"loadObject:", b"fixtureLoadObject:");
        let nested = h.hooks.context(&[
            h.key(b"fixtureLoadObject:"),
            reflect,
            nested_target,
            h.key(b"nested:"),
            body,
            gc,
            size(0).unwrap(),
        ]);
        h.replace(tcc, b"loadObject:", h.method(b"reenter", nested));
        let loaded = h.call(codegen, b"load:", &[path]);
        h.restore(tcc, b"loadObject:", b"fixtureLoadObject:", loader);
        assert_eq!(
            h.hooks.count(nested, 6),
            1,
            "one saved body unit; constructor reentry must not load an adapter or replay"
        );
        assert_eq!(h.call(nested_target, b"nested:", &[argument]), argument);
        assert_eq!(
            h.number(h.call(
                h.call(loaded, b"answer7:", &[argument]),
                b"at:",
                &[h.integer(2)]
            )),
            7
        );
        // Keep the historical measurement keys consumed by supplied-template-bench.py.
        eprintln!(
            "artifact-template synchronous={synchronous} units=1 native_bytes={:?} elapsed_ns={}",
            bytes_before.zip(bytes_after).map(|(a, b)| b - a),
            elapsed.as_nanos()
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
    image_selection(&h);
    image_body_demand(&h);
}
fn image_selection(h: &H<'_>) {
    use birnam_artifact_codec::{
        code_binding_inventory as b, materialization_selection as m, portable_inventory as p,
    };
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
    let selection = m::encode(&programs, &bindings, &[m::Disposition::Resumable; 2], &[]).unwrap();
    let inputs = [h.bytes(&programs), h.bytes(&bindings), h.bytes(&selection)];
    let cg = h.named(b"Codegen");
    let yes = h.named(b"true");
    let selector = b"methodTemplateForArity:synchronous:";
    let original = h.lookup(cg, selector);
    let bodies = [
        h.eval(b"[:self :argument | #[self argument #first]]"),
        h.eval(b"[:self :argument | #[self argument #second]]"),
    ];
    let target = h.call(h.named(b"Object"), b"clone", &[]);
    let argument = h.named(b"true");
    let args = h.call(h.named(b"Vector"), b"new", &[]);
    let args = h.call(args, b"add:", &[argument]);
    let monitor = h.call(h.named(b"Object"), b"clone", &[]);
    for name in [
        b"watch".as_slice(),
        b"remaining",
        b"allocation:bytes:identity:",
    ] {
        h.replace(monitor, name, h.method(name, monitor));
    }
    for distinct in [false, true] {
        for mapped in [false, true] {
            let mark = h.mark();
            let first = h.call(cg, selector, &[h.integer(1), h.named(b"false")]);
            let second = h.call(cg, selector, &[h.integer(1), h.named(b"false")]);
            assert_ne!(first, second);
            let mut counters = vec![];
            for template in [first, second] {
                h.alias(template, b"withImplementation:", b"fixtureImplementation:");
                let state = h.hooks.context(&[
                    h.key(b"fixtureImplementation:"),
                    h.named(b"GC"),
                    size(0).unwrap(),
                ]);
                h.replace(
                    template,
                    b"withImplementation:",
                    h.method(b"constructor", state),
                );
                counters.push(state);
            }
            h.host.send(monitor, b"watch", &[first, second]).unwrap();
            let state = h.hooks.context(&[
                h.named(b"GC"),
                first,
                second,
                size(distinct as usize).unwrap(),
                size(0).unwrap(),
            ]);
            h.replace(cg, selector, h.method(b"selectTemplate", state));
            let bundle = if mapped {
                Some(h.call(cg, b"compilePortableImageCode:bindings:selection:", &inputs))
            } else {
                None
            };
            assert_eq!(
                h.hooks.count(state, 4),
                0,
                "saving body code does not construct named bindings"
            );
            h.collect();
            let before = h.units();
            assert_eq!(
                if let Some(bundle) = bundle {
                    h.call(
                        cg,
                        b"beginPortableImageMapped:programs:bindings:selection:",
                        &[bundle, inputs[0], inputs[1], inputs[2]],
                    )
                } else {
                    h.call(
                        cg,
                        b"beginPortableImageRebuild:bindings:selection:",
                        &inputs,
                    )
                },
                yes
            );
            assert_eq!(h.hooks.count(state, 4), 2);
            h.replace(cg, selector, original);
            h.hooks.put(state, 1, ptr::null_mut());
            h.hooks.put(state, 2, ptr::null_mut());
            h.clear(mark);
            h.collect();
            assert_eq!(
                as_size(h.call(monitor, b"remaining", &[])),
                Some(if distinct { 2 } else { 1 }),
                "only selected templates remain rooted by the stage"
            );
            for &counter in &counters {
                h.hooks.put(counter, 2, size(0).unwrap());
            }
            for (i, body) in bodies.into_iter().enumerate() {
                assert_eq!(
                    h.call(
                        cg,
                        b"preparePortableImageBinding:implementation:",
                        &[h.integer(i), body]
                    ),
                    yes
                );
                let method = h.call(cg, b"preparedPortableImageBinding:", &[h.integer(i)]);
                let value = h.call(method, b"invokeOn:arguments:", &[target, args]);
                assert_eq!(h.call(value, b"at:", &[h.integer(0)]), target);
                assert_eq!(h.call(value, b"at:", &[h.integer(1)]), argument);
                assert_eq!(
                    h.call(value, b"at:", &[h.integer(2)]),
                    h.key(if i == 0 { b"first" } else { b"second" })
                );
            }
            assert_eq!(h.hooks.count(counters[0], 2), if distinct { 1 } else { 2 });
            assert_eq!(h.hooks.count(counters[1], 2), usize::from(distinct));
            h.collect();
            assert_eq!(h.units(), before);
            assert_eq!(h.call(cg, b"cancelPortableImageUnit", &[]), yes);
            h.clear(mark);
            h.collect();
            assert_eq!(as_size(h.call(monitor, b"remaining", &[])), Some(0));
            assert_eq!(h.units(), before);
            eprintln!(
                "image templates mapped={mapped} distinct={distinct}: current constructors preserved with no native adapter unit"
            );
        }
    }
}
fn image_body_demand(h: &H<'_>) {
    use birnam_artifact_codec::{
        code_binding_inventory as b, materialization_selection as m, portable_inventory as p,
    };
    let programs = p::encode_closure(&[include_bytes!(
        "../../../tests/fixtures/portable-code-v1/program-empty.bin"
    )])
    .unwrap();
    let bindings = b::encode(&[]).unwrap();
    let selection = m::encode(&programs, &bindings, &[], &[m::Disposition::Resumable]).unwrap();
    let inputs = [h.bytes(&programs), h.bytes(&bindings), h.bytes(&selection)];
    let cg = h.named(b"Codegen");
    let tcc = h.named(b"TCC");
    let backend = h.named(b"NativeBackend");
    let gc = h.named(b"GC");
    let nil = h.named(b"nil");
    let yes = h.named(b"true");
    let emit = b"emit:source:filename:persistent:synchronous:";
    let emit_alias = b"fixtureEmit:source:filename:persistent:synchronous:";
    let emitter = h.alias(backend, emit, emit_alias);
    let error = h.call(
        h.named(b"Error"),
        b"kind:message:",
        &[h.key(b"demandRejected"), h.text(b"demand probe rejected")],
    );
    let fake = h.call(h.named(b"Object"), b"clone", &[]);
    let init = h
        .hooks
        .context(&[gc, nil, size(0).unwrap(), ptr::null_mut()]);
    h.replace(fake, b"initialize", h.method(b"probe", init));
    let bundle = h.call(cg, b"compilePortableImageCode:bindings:selection:", &inputs);
    for mapped in [true, false] {
        let loader_selector = if mapped {
            b"loadObject:".as_slice()
        } else {
            b"compile:"
        };
        let alias = b"fixtureBodyLoad:";
        let loader = h.alias(tcc, loader_selector, alias);
        let emitted = h.hooks.context(&[
            h.key(emit_alias),
            gc,
            tcc,
            cg,
            size(0).unwrap(),
            ptr::null_mut(),
        ]);
        let loaded =
            h.hooks
                .context(&[h.key(alias), gc, tcc, cg, size(0).unwrap(), ptr::null_mut()]);
        h.replace(backend, emit, h.method(b"demand", emitted));
        h.collect();
        let before = h.units();
        let mark = h.mark();
        let begin = || {
            if mapped {
                h.call(
                    cg,
                    b"beginPortableImageMapped:programs:bindings:selection:",
                    &[bundle, inputs[0], inputs[1], inputs[2]],
                )
            } else {
                h.call(
                    cg,
                    b"beginPortableImageRebuild:bindings:selection:",
                    &inputs,
                )
            }
        };
        assert_eq!(begin(), yes);
        h.replace(tcc, loader_selector, h.method(b"demand", loaded));
        h.collect();
        assert_eq!(h.units(), before);
        assert_eq!(h.hooks.count(emitted, 4), 0);
        assert_eq!(h.hooks.count(loaded, 4), 0);
        let index = h.integer(0);
        let resumable = [index, h.named(b"false")];
        if !mapped {
            h.hooks.put(emitted, 5, error);
            assert_eq!(
                h.call(cg, b"portableImageBlockMethod:synchronous:", &resumable),
                nil
            );
            assert_eq!(h.hooks.count(loaded, 4), 0);
            h.hooks.put(emitted, 5, ptr::null_mut());
        }
        for answer in [error, fake] {
            h.hooks.put(loaded, 5, answer);
            assert_eq!(
                h.call(cg, b"portableImageBlockMethod:synchronous:", &resumable),
                if answer == error { error } else { nil }
            );
            h.collect();
            assert_eq!(h.units(), before);
        }
        h.hooks.put(loaded, 5, fake);
        let fault = h.hooks.fault(17);
        h.hooks.put(init, 3, fault);
        assert_eq!(
            unsafe {
                h.host.id.send(
                    cg,
                    h.key(b"portableImageBlockMethod:synchronous:"),
                    &resumable,
                )
            }
            .unwrap_err(),
            fault
        );
        h.hooks.put(init, 3, ptr::null_mut());
        h.hooks.put(loaded, 5, ptr::null_mut());
        let method = h
            .host
            .send(cg, b"portableImageBlockMethod:synchronous:", &resumable)
            .unwrap();
        let calls = h.hooks.count(loaded, 4);
        let emitted_calls = h.hooks.count(emitted, 4);
        h.collect();
        assert_eq!(h.units(), before + 1);
        assert_eq!(
            h.call(cg, b"portableImageBlockMethod:synchronous:", &resumable),
            method
        );
        assert_eq!(h.hooks.count(loaded, 4), calls);
        assert_eq!(h.hooks.count(emitted, 4), emitted_calls);
        h.hooks.put(loaded, 5, error);
        let synchronous = [index, yes];
        assert_eq!(
            h.call(cg, b"portableImageBlockMethod:synchronous:", &synchronous),
            error
        );
        h.hooks.put(loaded, 5, ptr::null_mut());
        let sync = h.call(cg, b"portableImageBlockMethod:synchronous:", &synchronous);
        assert_ne!(sync, method);
        assert_eq!(
            h.hooks.count(emitted, 4),
            if mapped {
                0
            } else {
                h.hooks.count(loaded, 4) + 1
            }
        );
        assert!(
            h.host
                .send(
                    cg,
                    b"portableImageBlockMethod:synchronous:",
                    &[h.integer(1), h.named(b"false")]
                )
                .is_err()
        );
        assert_eq!(h.call(cg, b"cancelPortableImageUnit", &[]), yes);
        h.collect();
        assert_eq!(h.units(), before + 2);
        for method in [method, sync] {
            let captures = h.call(h.named(b"Vector"), b"new", &[]);
            let block = h.call(
                h.named(b"Block"),
                b"fromMethod:captures:",
                &[method, captures],
            );
            let machine = h.call(h.named(b"Machine"), b"new", &[]);
            assert_eq!(h.call(machine, b"runBlock:", &[block]), nil);
        }
        h.clear(mark);
        h.collect();
        assert_eq!(h.units(), before);
        assert_eq!(begin(), yes);
        assert_eq!(h.call(cg, b"cancelPortableImageUnit", &[]), yes);
        h.collect();
        assert_eq!(h.units(), before);
        h.replace(backend, emit, emitter);
        h.restore(tcc, loader_selector, alias, loader);
        eprintln!(
            "image body demand mapped={mapped}: reentry refusal, retry, caching and code retirement passed"
        );
    }
    let mut corrupt = h.read(bundle);
    assert_eq!(&corrupt[..8], b"BIRNAMO2");
    let first_len = u64::from_le_bytes(corrupt[44..52].try_into().unwrap()) as usize;
    corrupt[52 + first_len + 8] = 0;
    let corrupt = h.bytes(&corrupt);
    let mark = h.mark();
    h.collect();
    let before = h.units();
    assert_eq!(
        h.call(
            cg,
            b"beginPortableImageMapped:programs:bindings:selection:",
            &[corrupt, inputs[0], inputs[1], inputs[2]]
        ),
        yes
    );
    let index = h.integer(0);
    let method = h.call(
        cg,
        b"portableImageBlockMethod:synchronous:",
        &[index, h.named(b"false")],
    );
    assert_ne!(method, nil);
    for _ in 0..2 {
        let rejected = h.call(cg, b"portableImageBlockMethod:synchronous:", &[index, yes]);
        assert_eq!(h.call(rejected, b"isError", &[]), yes);
    }
    h.call(cg, b"cancelPortableImageUnit", &[]);
    h.clear(mark);
    h.collect();
    assert_eq!(h.units(), before);
    h.restore(backend, emit, emit_alias, emitter);
    let behavior = h.call(h.host.id.root, b"behaviorOf:", &[fake]);
    h.call(behavior, b"removeMethod:", &[h.key(b"initialize")]);
}
