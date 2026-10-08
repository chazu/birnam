//! Binary prototype and embedded payloads through production message owners.
mod support;
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{NativeEntry, Operand, Outcome, Protocol},
};
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
    unsafe fn call(&self, o: Oop, n: &[u8], a: &[Oop]) -> Oop {
        unsafe {
            self.keep(object(send(
                self.id,
                o,
                n,
                &a.iter().copied().map(Operand::object).collect::<Vec<_>>(),
            )))
        }
    }
}

impl H {
    unsafe fn eval(&self, source: &[u8]) -> Oop {
        unsafe { self.call(self.named(b"Codegen"), b"eval:", &[self.string(source)]) }
    }
}
impl H {
    unsafe fn bytes(&self, o: Oop) -> Vec<u8> {
        unsafe {
            let b = send(self.id, o, b"bytes", &[]).as_bytes().unwrap();
            borrowed_bytes(b.pointer, b.length).unwrap().to_vec()
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
    unsafe fn at(&self, o: Oop, i: usize) -> Oop {
        unsafe {
            let i = self.keep(object(send(
                self.id,
                self.named(b"Integer"),
                b"fromWord:",
                &[Operand::word(i)],
            )));
            self.call(o, b"at:", &[i])
        }
    }
}

unsafe fn binding(id: Protocol, vt: Oop, selector: &[u8]) -> NativeEntry {
    unsafe {
        send(
            id,
            id.receiver,
            b"bindingOf:in:",
            &[
                Operand::object(id.intern_name(selector).unwrap()),
                Operand::object(vt),
            ],
        )
        .as_entry()
        .unwrap()
    }
}
unsafe fn set(id: Protocol, vt: Oop, selector: &[u8], entry: NativeEntry) {
    unsafe {
        send(
            id,
            vt,
            b"addMethod",
            &[
                Operand::object(id.intern_name(selector).unwrap()),
                Operand::entry(entry),
            ],
        );
    }
}
#[test]
#[ignore = "requires production native modules"]
fn bytes_and_embedding_use_live_messages_and_owned_payloads() {
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
        };
        let proto = h.named(b"Bytes");
        let vt = h.named(b"BytesVt");
        assert_eq!(h.number(h.call(proto, b"size", &[])), 0);
        let bytes = h.eval(b"[\"a\0\xff\" asBytes]");
        // Construct arbitrary binary independently of source escape spelling.
        let payload: Vec<u8> = (0..=255).collect();
        let value = h.keep(object(send(
            id,
            proto,
            b"fromBytes:",
            &[Operand::bytes(&payload)],
        )));
        assert_eq!(h.bytes(value), payload);
        assert_eq!(
            object(send(
                id,
                id.receiver,
                b"behaviorOf:",
                &[Operand::object(value)]
            )),
            vt
        );
        assert_eq!(h.number(h.call(value, b"size", &[])), 256);
        assert_eq!(h.number(h.at(value, 255)), 255);
        assert_eq!(h.at(value, 256), h.named(b"nil"));
        assert_eq!(h.at(value, usize::MAX), h.named(b"nil"));
        assert_eq!(h.call(value, b"at:", &[h.named(b"nil")]), h.named(b"nil"));
        let copy = h.call(value, b"clone", &[]);
        assert_ne!(copy, value);
        assert_eq!(h.bytes(copy), payload);
        let text = h.call(value, b"asString", &[]);
        assert_eq!(h.bytes(text), payload);
        assert_eq!(h.bytes(h.call(text, b"asBytes", &[])), payload);
        assert_ne!(h.call(bytes, b"clone", &[]), bytes);
        // Prototype methods use the usual MOP, including a private behavior.
        assert_eq!(
            h.number(h.eval(b"let B := [Bytes clone]; [Reflect defineMethod: #answer on: B as: [:self | 37]]; [B answer]")),
            37
        );
        collect(id, h.gc);
        assert_eq!(h.bytes(copy), payload);
        let dead = object(send(id, proto, b"fromBytes:", &[Operand::bytes(b"dead")]));
        let dead_token = retain(id, h.roots, dead);
        let pointer_bytes = dead.addr().to_ne_bytes();
        let opaque = h.keep(object(send(
            id,
            proto,
            b"fromBytes:",
            &[Operand::bytes(&pointer_bytes)],
        )));
        release(id, h.roots, dead_token);
        collect(id, h.gc);
        assert!(
            !present(id, dead),
            "binary bytes must not retain a coincidental object address"
        );
        assert_eq!(h.bytes(opaque), pointer_bytes);

        let temp_name = format!(".embedding-test-{}", std::process::id());
        let temp = std::env::current_dir().unwrap().join(&temp_name);
        std::fs::create_dir_all(&temp).unwrap();
        let asset = temp.join("asset.bin");
        let source_file = temp.join("module.birnam");
        let source = b"[| [System embedFile: \"asset.bin\"]]";
        std::fs::write(&asset, &payload).unwrap();
        std::fs::write(&source_file, source).unwrap();
        let block = h.call(
            h.named(b"System"),
            b"load:",
            &[h.string(source_file.as_os_str().as_bytes())],
        );
        assert_ne!(h.call(block, b"isError", &[]), h.named(b"true"));
        std::fs::remove_file(&asset).unwrap();
        std::fs::remove_file(&source_file).unwrap();
        collect(id, h.gc);
        let embedded = h.call(block, b"value", &[]);
        assert_eq!(h.bytes(embedded), payload);
        assert_eq!(
            object(send(
                id,
                id.receiver,
                b"behaviorOf:",
                &[Operand::object(embedded)]
            )),
            vt
        );
        assert_eq!(h.bytes(h.call(embedded, b"clone", &[])), payload);
        exercise_replacement(&h, block, &temp);

        // emit: bytes -> a real system-compiler dylib -> Id load -> Machine.
        // Neither the asset nor generated C exists when the library is loaded.
        std::fs::write(&asset, &payload).unwrap();
        let source = format!("[System embedFile: \"{temp_name}/asset.bin\"]");
        let c = h.bytes(h.call(
            h.named(b"Codegen"),
            b"emit:",
            &[h.string(source.as_bytes())],
        ));
        let c_path = temp.join("embedded.c");
        let library = temp.join("embedded.dylib");
        std::fs::write(&c_path, c).unwrap();
        let output = std::process::Command::new("cc")
            .args(["-std=c11", "-dynamiclib"])
            .arg(&c_path)
            .arg("-o")
            .arg(&library)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::remove_file(&asset).unwrap();
        std::fs::remove_file(&c_path).unwrap();
        let owner_vt = h.keep(behavior(id));
        install(
            id,
            owner_vt,
            b"retain:",
            library_retain,
            (&h as *const H).cast_mut().cast(),
        );
        let owner = h.keep(allocate(id, owner_vt, 0));
        let method = h.keep(object(
            id.send(
                id.receiver,
                id.load,
                &[
                    Operand::bytes(library.as_os_str().as_bytes()),
                    Operand::object(ns),
                    Operand::object(owner),
                ],
            )
            .unwrap(),
        ));
        let captures = h.call(h.named(b"Vector"), b"new", &[]);
        let block = h.call(
            h.named(b"Block"),
            b"fromMethod:captures:",
            &[method, captures],
        );
        let machine = h.call(h.named(b"Machine"), b"new", &[]);
        collect(id, h.gc);
        assert_eq!(h.bytes(h.call(machine, b"runBlock:", &[block])), payload);
        exercise_errors(&h, &temp);
        payload_outlives_code(&h, &temp);
        std::fs::remove_dir_all(&temp).unwrap();
    }
}
// Id's library loader keeps this fixture dylib mapped for the process lifetime.
// Its test owner retains Method references through messages for the same lifetime.
unsafe extern "C" fn library_retain(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    let h = unsafe { &*c.cast::<H>() };
    let Some([value]) = (unsafe { borrowed_slice(a, n) }) else {
        return Outcome::failed(1, 0);
    };
    match unsafe { h.id.send_named(h.roots, b"retain:", &[*value]) } {
        Ok(_) => Outcome::loaded(Operand::object(r)),
        Err(e) => e,
    }
}
struct Forward {
    observed_receiver: Option<Oop>,
    id: Protocol,
    gc: Oop,
    alias: Oop,
    calls: Cell<usize>,
}
unsafe extern "C" fn forward(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let f = &*c.cast::<Forward>();
        let args = borrowed_slice(a, n).unwrap();
        // Construction borrows cannot span the collecting send below.
        let copied = args
            .first()
            .and_then(|a| a.as_bytes())
            .map(|b| borrowed_bytes(b.pointer, b.length).unwrap().to_vec());
        if f.observed_receiver.is_none_or(|target| target == r) {
            f.calls.set(f.calls.get() + 1);
        }
        f.id.send_named(f.gc, b"collectPrecisely", &[])?;
        // Receiver is retained by the compiler/Machine. No send follows a
        // borrowed result from the alias, so the original borrow rule holds.
        if let Some(bytes) = copied {
            f.id.send(r, f.alias, &[Operand::bytes(&bytes)])
        } else {
            f.id.send(r, f.alias, args)
        }
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
struct Embedded {
    id: Protocol,
    gc: Oop,
    value: Oop,
    path: RefCell<Vec<u8>>,
    calls: Cell<usize>,
    fail: bool,
}
unsafe extern "C" fn embedded(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let e = &*c.cast::<Embedded>();
        let [path] = borrowed_slice(a, n).unwrap() else {
            return Err(Outcome::failed(1, 0));
        };
        let raw =
            e.id.send_named(object(*path), b"bytes", &[])?
                .as_bytes()
                .unwrap();
        *e.path.borrow_mut() = borrowed_bytes(raw.pointer, raw.length).unwrap().to_vec();
        e.calls.set(e.calls.get() + 1);
        e.id.send_named(e.gc, b"collectPrecisely", &[])?;
        if e.fail {
            Err(Outcome::failed(881, 73))
        } else {
            Ok(Operand::object(e.value))
        }
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
unsafe fn exercise_replacement(h: &H, block: Oop, temp: &std::path::Path) {
    unsafe {
        let id = h.id;
        let string_vt = h.named(b"StringVt");
        let bytes_vt = h.named(b"BytesVt");
        for (vt, name) in [
            (string_vt, b"asBytes".as_slice()),
            (bytes_vt, b"fromBytes:"),
        ] {
            let old = binding(id, vt, name);
            let alias = h.keep(id.intern_name(b"embeddingOriginal:").unwrap());
            set(id, vt, b"embeddingOriginal:", old);
            let f = Forward {
                observed_receiver: None,
                id,
                gc: h.gc,
                alias,
                calls: Cell::new(0),
            };
            install(
                id,
                vt,
                name,
                forward,
                (&f as *const Forward).cast_mut().cast(),
            );
            assert_eq!(
                h.bytes(h.call(block, b"value", &[])),
                (0..=255).collect::<Vec<_>>()
            );
            assert_eq!(
                f.calls.get(),
                // String construction allocates Bytes-owned storage, then
                // asBytes constructs the returned binary value separately.
                if name == b"fromBytes:" { 2 } else { 1 },
                "runtime constructors must use fresh sends"
            );
            set(id, vt, name, old);
        }
        let value = h.keep(object(send(
            id,
            h.named(b"Bytes"),
            b"fromBytes:",
            &[Operand::bytes(b"replacement\0\xff")],
        )));
        let e = Embedded {
            id,
            gc: h.gc,
            value,
            path: RefCell::new(Vec::new()),
            calls: Cell::new(0),
            fail: false,
        };
        let old = binding(id, bytes_vt, b"bytes");
        let alias = h.keep(id.intern_name(b"embeddingBytes").unwrap());
        set(id, bytes_vt, b"embeddingBytes", old);
        let f = Forward {
            // Canonical CodeObject admission also reads Bytes during compilation.
            observed_receiver: Some(value),
            id,
            gc: h.gc,
            alias,
            calls: Cell::new(0),
        };
        install(
            id,
            bytes_vt,
            b"bytes",
            forward,
            (&f as *const Forward).cast_mut().cast(),
        );
        // Source global lookup is live, rather than hardwired to Namespace System.
        let receiver_vt = h.keep(behavior(id));
        install(
            id,
            receiver_vt,
            b"embedFile:",
            embedded,
            (&e as *const Embedded).cast_mut().cast(),
        );
        let receiver = h.keep(allocate(id, receiver_vt, 0));
        // A lexical receiver named System is still that receiver, not the
        // compile-time form. Compiling these Blocks must perform no file I/O.
        let lexical = h.eval(b"[:System | [System embedFile: \"lexical.bin\"]]");
        assert_eq!(e.calls.get(), 0);
        assert_eq!(h.call(lexical, b"value:", &[receiver]), value);
        let lexical = h.eval(b"[:x | let System := x; [System embedFile: \"local.bin\"]]");
        assert_eq!(e.calls.get(), 1);
        assert_eq!(h.call(lexical, b"value:", &[receiver]), value);
        e.calls.set(0);
        f.calls.set(0);
        let globals = h.named(b"Globals");
        let key = h.keep(id.intern_name(b"System").unwrap());
        let previous = h.call(globals, b"at:", &[key]);
        h.call(globals, b"at:put:", &[key, receiver]);
        // A top-level let is also a lexical binding. It must be captured by
        // the nested Block, never replaced with a compile-time global read.
        let lexical = h.eval(b"let System := System; [| [System embedFile: \"top-level.bin\"]]");
        assert_eq!(e.calls.get(), 0, "top-level lexical System must not embed");
        assert_eq!(h.call(lexical, b"value", &[]), value);
        assert_eq!(e.calls.get(), 1);
        e.calls.set(0);
        let path = h.string(temp.join("module.birnam").as_os_str().as_bytes());
        let b = h.call(
            h.named(b"Codegen"),
            b"evalPersisting:from:",
            &[
                h.string(b"[| [System embedFile: \"never-created.bin\"]]"),
                path,
            ],
        );
        assert_eq!(e.calls.get(), 1);
        assert_eq!(
            f.calls.get(),
            1,
            "compiler reads the embedded Bytes once through a send"
        );
        assert_eq!(
            *e.path.borrow(),
            temp.join("never-created.bin").as_os_str().as_bytes()
        );
        assert_eq!(h.bytes(h.call(b, b"value", &[])), b"replacement\0\xff");
        assert_eq!(e.calls.get(), 1, "embedding runs during compilation only");
        set(id, bytes_vt, b"bytes", old);
        // Dependency native failure propagates without conversion or execution.
        let e = Embedded { fail: true, ..e };
        install(
            id,
            receiver_vt,
            b"embedFile:",
            embedded,
            (&e as *const Embedded).cast_mut().cast(),
        );
        let source = h.string(b"[System embedFile: \"unused\"]");
        let failure = id
            .send_named(h.named(b"Codegen"), b"eval:", &[Operand::object(source)])
            .unwrap_err();
        assert_eq!((failure.status, failure.detail), (881, 73));
        h.call(globals, b"at:put:", &[key, previous]);
    }
}
unsafe fn exercise_errors(h: &H, temp: &std::path::Path) {
    unsafe {
        let codegen = h.named(b"Codegen");
        for source in [
            b"[System embedFile: \"\"]".as_slice(),
            b"[System embedFile: \"/absolute\"]",
            b"[System embedFile: \"../parent\"]",
            b"[System embedFile: \"a/./b\"]",
            b"[System embedFile: nil]",
            b"[System embedFile: \"missing-file\"]",
            b"[System => embedFile: \"unused\"]",
        ] {
            let result = h.call(codegen, b"eval:", &[h.string(source)]);
            assert_eq!(
                h.call(result, b"isError", &[]),
                h.named(b"true"),
                "{source:?}"
            );
        }
        let source = h.string(b"[System embedFile: \"missing-file\"]");
        assert_eq!(
            h.call(
                codegen,
                b"evalPersisting:from:",
                &[
                    source,
                    h.string(temp.join("module.birnam").as_os_str().as_bytes())
                ]
            ),
            h.named(b"nil")
        );
        assert!(
            String::from_utf8_lossy(&h.bytes(h.call(codegen, b"lastError", &[])))
                .contains("could not read")
        );
        // Empty files are valid; over-limit reads fail before emitting an artifact.
        let asset = temp.join("limit.bin");
        let path = h.string(temp.join("module.birnam").as_os_str().as_bytes());
        let source = h.string(b"[System embedFile: \"limit.bin\"]");
        std::fs::write(&asset, b"").unwrap();
        assert_eq!(
            h.bytes(h.call(codegen, b"evalPersisting:from:", &[source, path])),
            b""
        );
        std::fs::OpenOptions::new()
            .write(true)
            .open(&asset)
            .unwrap()
            .set_len(16 * 1024 * 1024 + 1)
            .unwrap();
        assert_eq!(
            h.call(codegen, b"evalPersisting:from:", &[source, path]),
            h.named(b"nil")
        );
        assert!(
            String::from_utf8_lossy(&h.bytes(h.call(codegen, b"lastError", &[])))
                .contains("16 MiB")
        );
    }
}

unsafe fn payload_outlives_code(h: &H, temp: &std::path::Path) {
    unsafe {
        let tcc = h.named(b"TCC");
        collect(h.id, h.gc);
        send(h.id, tcc, b"checkpoint", &[]);
        let before = send(h.id, tcc, b"liveNativeUnits", &[]).as_word().unwrap();
        let mark = h.tokens.borrow().len();
        std::fs::write(temp.join("owned.bin"), b"owned\0\xff").unwrap();
        let value = h.call(
            h.named(b"Codegen"),
            b"evalPersisting:from:",
            &[
                h.string(b"[System embedFile: \"owned.bin\"]"),
                h.string(temp.join("module.birnam").as_os_str().as_bytes()),
            ],
        );
        let token = retain(h.id, h.roots, value);
        let tokens: Vec<_> = h.tokens.borrow_mut().drain(mark..).collect();
        for token in tokens {
            release(h.id, h.roots, token);
        }
        collect(h.id, h.gc);
        send(h.id, tcc, b"checkpoint", &[]);
        assert_eq!(
            send(h.id, tcc, b"liveNativeUnits", &[]).as_word(),
            Some(before)
        );
        assert_eq!(
            h.bytes(value),
            b"owned\0\xff",
            "Bytes must own its payload after the code mapping is freed"
        );
        release(h.id, h.roots, token);
        collect(h.id, h.gc);
        assert!(!present(h.id, value));
    }
}
