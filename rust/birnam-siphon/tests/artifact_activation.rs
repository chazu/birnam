//! Durable artifacts through production owners, without source recompilation.
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
    unsafe fn bytes(&self, o: Oop) -> Vec<u8> {
        unsafe {
            let b = send(self.id, o, b"bytes", &[]).as_bytes().unwrap();
            borrowed_bytes(b.pointer, b.length).unwrap().to_vec()
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

struct Forward {
    id: Protocol,
    gc: Oop,
    alias: Oop,
    calls: Cell<usize>,
}
unsafe extern "C" fn forward(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let f = &*c.cast::<Forward>();
        f.calls.set(f.calls.get() + 1);
        f.id.send_named(f.gc, b"collectPrecisely", &[])?;
        f.id.send(r, f.alias, borrowed_slice(a, n).unwrap())
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
unsafe extern "C" fn reject(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(909, 73)
}
#[test]
#[ignore = "requires production native modules"]
fn durable_artifacts_use_owned_code_and_current_messages() {
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
        let reflect = h.named(b"Reflect");
        assert_eq!(
            h.bytes(h.call(
                reflect,
                b"nameOf:",
                &[id.intern_name(b"artifactName").unwrap()]
            )),
            b"artifactName"
        );
        let anonymous = h.call(h.named(b"Object"), b"clone", &[]);
        for value in [anonymous, h.named(b"Object"), h.string(b"not a Symbol")] {
            let error = h.call(reflect, b"nameOf:", &[value]);
            assert_eq!(h.call(error, b"kind", &[]), id.intern_name(b"notASymbol").unwrap());
            assert_eq!(h.bytes(h.call(error, b"message", &[])), b"nameOf: expects a symbol");
        }
        let cg = h.named(b"Codegen");
        let tcc = h.named(b"TCC");
        let temp = std::env::temp_dir().join(format!("birnam-artifact-{}", std::process::id()));
        std::fs::create_dir_all(&temp).unwrap();
        let file = temp.join("module.birnam");
        let artifact = temp.join("libbirnam_durable.dylib");
        let payload = b"owned\0\xff";
        std::fs::write(temp.join("asset"), payload).unwrap();
        let source = b"[| [System embedFile: \"asset\"]]";
        std::fs::write(&file, source).unwrap();
        let path = h.string(artifact.as_os_str().as_bytes());
        let compiled = h.call(
            cg,
            b"compile:output:from:",
            &[
                h.string(source),
                path,
                h.string(file.as_os_str().as_bytes()),
            ],
        );
        assert_eq!(
            compiled,
            path,
            "{}",
            String::from_utf8_lossy(&h.bytes(h.call(cg, b"lastError", &[])))
        );
        assert!(std::fs::read(&artifact).unwrap().starts_with(b"\x7fELF"));
        std::fs::remove_file(&file).unwrap();
        std::fs::remove_file(temp.join("asset")).unwrap();
        let descriptor = h.call(h.named(b"Artifact"), b"inspect:", &[path]);
        assert_ne!(descriptor, h.named(b"nil"));
        let tcc_vt = h.named(b"TCCVt");
        let compile = binding(id, tcc_vt, b"compile:");
        let compile_object = binding(id, tcc_vt, b"compileObject:");
        let lang_vt = h.named(b"LangVt");
        let parse = binding(id, lang_vt, b"parse:");
        // A displaced compiled entry is borrowed; pin its Method until restored.
        h.keep(object(send(id, h.named(b"Execution"), b"methodOfEntry:", &[Operand::entry(parse)])));
        let failure = NativeEntry {
            method: Some(reject),
            context: std::ptr::null_mut(),
        };
        set(id, tcc_vt, b"compile:", failure);
        set(id, tcc_vt, b"compileObject:", failure);
        set(id, lang_vt, b"parse:", failure);
        // Force collection immediately before snapshot inspection and relocation.
        let mut hooks = Vec::new();
        for (vt, selector, alias) in [
            (
                h.named(b"ArtifactVt"),
                b"inspectBytes:from:".as_slice(),
                b"originalInspect:from:".as_slice(),
            ),
            (tcc_vt, b"loadObject:", b"originalLoad:"),
        ] {
            let original = binding(id, vt, selector);
            set(id, vt, alias, original);
            let f = Box::new(Forward {
                id,
                gc: h.gc,
                alias: id.intern_name(alias).unwrap(),
                calls: Cell::new(0),
            });
            install(
                id,
                vt,
                selector,
                forward,
                (&*f as *const Forward).cast_mut().cast(),
            );
            hooks.push((vt, selector, original, f));
        }
        collect(id, h.gc);
        send(id, tcc, b"checkpoint", &[]);
        let before = send(id, tcc, b"liveNativeUnits", &[]).as_word().unwrap();
        let mark = h.tokens.borrow().len();
        let block = h.call(cg, b"load:", &[path]);
        assert_ne!(block, h.named(b"nil"));
        collect(id, h.gc);
        let bytes = h.call(block, b"value", &[]);
        assert_eq!(h.bytes(bytes), payload);
        assert_eq!(
            send(id, tcc, b"liveNativeUnits", &[]).as_word(),
            Some(before + 1)
        );
        for (vt, selector, original, f) in hooks {
            assert!(f.calls.get() > 0);
            set(id, vt, selector, original);
        }
        set(id, tcc_vt, b"compile:", compile);
        set(id, tcc_vt, b"compileObject:", compile_object);
        set(id, lang_vt, b"parse:", parse);
        let bytes_root = retain(id, h.roots, bytes);
        let block_root = retain(id, h.roots, block);
        for token in h.tokens.borrow_mut().drain(mark..) {
            release(id, h.roots, token);
        }
        collect(id, h.gc);
        send(id, tcc, b"checkpoint", &[]);
        assert_eq!(h.bytes(h.call(block, b"value", &[])), payload);
        release(id, h.roots, block_root);
        collect(id, h.gc);
        send(id, tcc, b"checkpoint", &[]);
        assert_eq!(
            send(id, tcc, b"liveNativeUnits", &[]).as_word(),
            Some(before)
        );
        assert_eq!(h.bytes(bytes), payload);
        release(id, h.roots, bytes_root);
        // A failed current owner send retains its typed failure unchanged.
        let original = binding(id, tcc_vt, b"loadObject:");
        set(id, tcc_vt, b"loadObject:", failure);
        let err = id
            .send_named(cg, b"load:", &[Operand::object(path)])
            .unwrap_err();
        assert_eq!((err.status, err.detail), (909, 73));
        set(id, tcc_vt, b"loadObject:", original);
        // Compile failures do not overwrite an existing artifact.
        let old = std::fs::read(&artifact).unwrap();
        assert_eq!(
            h.call(cg, b"compile:output:", &[h.string(b"[bad"), path]),
            h.named(b"nil")
        );
        assert_eq!(std::fs::read(&artifact).unwrap(), old);
        // A modified source digest is rejected before the compiled body executes.
        let mut corrupt = old.clone();
        let magic = b"BIRNAM-ARTIFACT-DESCRIPTOR-V1";
        let start = corrupt
            .windows(magic.len())
            .position(|v| v == magic)
            .unwrap()
            + magic.len();
        // Payload length is followed by four fields before the source digest.
        let mut offset = start + 4;
        for _ in 0..4 {
            let n = u32::from_le_bytes(corrupt[offset..offset + 4].try_into().unwrap()) as usize;
            offset += 4 + n;
        }
        offset += 4;
        corrupt[offset] = if corrupt[offset] == b'0' { b'1' } else { b'0' };
        std::fs::write(&artifact, corrupt).unwrap();
        assert_eq!(h.call(cg, b"load:", &[path]), h.named(b"nil"));
        assert!(
            String::from_utf8_lossy(&h.bytes(h.call(cg, b"lastError", &[]))).contains("digest")
        );
        std::fs::write(&artifact, old).unwrap();
        h.call(cg, b"sealInitialGeneration", &[]);
        let rejected = h.call(cg, b"load:", &[path]);
        assert_eq!(h.call(rejected, b"isError", &[]), h.named(b"true"));
        assert_eq!(
            h.call(rejected, b"kind", &[]),
            id.intern_name(b"moduleIntroductionNotAvailable").unwrap()
        );
        std::fs::remove_dir_all(temp).unwrap();
    }
}
