//! Production Artifact inspection without legacy tables or activation.
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
    unsafe fn eval(&self, b: &[u8]) -> Oop {
        unsafe { self.call(self.named(b"Codegen"), b"eval:", &[self.string(b)]) }
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

fn fixture(abi: &[u8], target: &[u8], native: &[&[u8]]) -> Vec<u8> {
    fn field(out: &mut Vec<u8>, b: &[u8]) {
        out.extend_from_slice(&(b.len() as u32).to_le_bytes());
        out.extend_from_slice(b);
    }
    let mut payload = Vec::new();
    for value in [
        b"module".as_slice(),
        b"candidate",
        abi,
        target,
        b"source-digest",
        b"revision-digest",
    ] {
        field(&mut payload, value);
    }
    for list in [
        &[b"depA".as_slice(), b"depB".as_slice()][..],
        native,
        &[b"activate".as_slice()][..],
    ] {
        payload.extend_from_slice(&(list.len() as u32).to_le_bytes());
        for value in list {
            field(&mut payload, value);
        }
    }
    let mut bytes = b"BIRNAM-ARTIFACT-DESCRIPTOR-V1".to_vec();
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend(payload);
    bytes
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
        let args = borrowed_slice(a, n).unwrap();
        let copied = args
            .first()
            .and_then(|a| a.as_bytes())
            .map(|b| borrowed_bytes(b.pointer, b.length).unwrap().to_vec());
        f.calls.set(f.calls.get() + 1);
        f.id.send_named(f.gc, b"collectPrecisely", &[])?;
        if f.fail.get() {
            return Err(Outcome::failed(881, 23));
        }
        if let Some(bytes) = copied {
            f.id.send(r, f.alias, &[Operand::bytes(&bytes)])
        } else {
            f.id.send(r, f.alias, args)
        }
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
#[test]
#[ignore = "requires production native modules"]
fn artifact_inspection_uses_messages_and_traced_descriptors() {
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
        let artifact = h.named(b"Artifact");
        let nil = h.named(b"nil");
        assert_eq!(h.call(artifact, b"lastFailure", &[]), nil);
        let dir = std::env::temp_dir().join(format!("birnam-artifact-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("candidate.dylib");
        let path_obj = h.string(path.as_os_str().as_bytes());
        let abi = b"birnam-artifact-generation-v1";
        let target = b"aarch64-macos";
        let good = fixture(abi, target, &[b"libdep.dylib"]);
        let failure = |want: &[u8]| {
            assert_eq!(
                h.call(artifact, b"lastFailure", &[]),
                id.intern_name(want).unwrap()
            )
        };
        assert_eq!(h.call(artifact, b"inspect:", &[]), nil);
        failure(b"artifactPathInvalid");
        assert_eq!(h.call(artifact, b"inspect:", &[nil]), nil);
        failure(b"artifactPathInvalid");
        assert_eq!(h.call(artifact, b"inspect:", &[path_obj, path_obj]), nil);
        failure(b"artifactPathInvalid");
        assert_eq!(h.call(artifact, b"inspect:", &[path_obj]), nil);
        failure(b"artifactUnavailable");
        for (bytes, reason) in [
            (b"junk".to_vec(), b"artifactDescriptionMissing".as_slice()),
            (
                good[..good.len() - 2].to_vec(),
                b"artifactDescriptionMissing",
            ),
            (
                [good.clone(), good.clone()].concat(),
                b"artifactDescriptionMissing",
            ),
            (fixture(b"wrong", target, &[]), b"artifactAbiMismatch"),
            (fixture(abi, b"wrong", &[]), b"artifactTargetMismatch"),
            (good.clone(), b"nativeRequirementUnavailable"),
            (
                fixture(abi, target, &[b"../escape"]),
                b"nativeRequirementUnavailable",
            ),
        ] {
            std::fs::write(&path, bytes).unwrap();
            assert_eq!(h.call(artifact, b"inspect:", &[path_obj]), nil);
            failure(reason);
        }
        // Size limit is checked before reading a sparse file's contents.
        let file = std::fs::File::create(&path).unwrap();
        file.set_len((1u64 << 30) + 1).unwrap();
        drop(file);
        assert_eq!(h.call(artifact, b"inspect:", &[path_obj]), nil);
        failure(b"artifactUnavailable");
        std::fs::write(dir.join("libdep.dylib"), b"present").unwrap();
        // A real dylib with a constructor sentinel must remain inert during inspection.
        let marker = dir.join("activated");
        let c = dir.join("candidate.c");
        let bytes = good
            .iter()
            .map(|b| b.to_string())
            .collect::<Vec<_>>()
            .join(",");
        std::fs::write(&c, format!("#include <stdio.h>\n__attribute__((used)) const unsigned char descriptor[] = {{{bytes}}};\n__attribute__((constructor)) static void activate(void) {{ FILE *f = fopen(\"{}\", \"w\"); if (f) fclose(f); }}\n", marker.display())).unwrap();
        assert!(
            std::process::Command::new("cc")
                .args(["-dynamiclib", "-o"])
                .arg(&path)
                .arg(&c)
                .status()
                .unwrap()
                .success()
        );

        // Replace dependency methods with collecting forwards. Every forward dispatches by send.
        let mut forwards = Vec::new();
        let mut originals = Vec::new();
        for (i, (family, name)) in [
            (b"String".as_slice(), b"fromBytes:".as_slice()),
            (b"String", b"bytes"),
            (b"Integer", b"fromWord:"),
            (b"Integer", b"wordOf:"),
            (b"Vector", b"new"),
            (b"Vector", b"add:"),
            (b"ObjectModel", b"allocate:bytes:"),
            (b"ObjectModel", b"slotAt:in:put:"),
            (b"ObjectModel", b"slotAt:in:"),
        ]
        .into_iter()
        .enumerate()
        {
            let vt = behavior_of(&h, h.named(family));
            let original = binding(&h, vt, name);
            let alias = h.keep(
                id.intern_name(format!("artifactOriginal{i}").as_bytes())
                    .unwrap(),
            );
            send(
                id,
                vt,
                b"addMethod",
                &[Operand::object(alias), Operand::entry(original)],
            );
            let f = Box::new(Forward {
                id,
                gc: h.gc,
                alias,
                calls: Cell::new(0),
                fail: Cell::new(false),
            });
            set(
                &h,
                vt,
                name,
                NativeEntry::new(forward, (&*f as *const Forward).cast_mut().cast()),
            );
            forwards.push(f);
            originals.push((vt, name, original));
        }
        let desc = h.call(artifact, b"inspect:", &[path_obj]);
        assert_ne!(desc, nil);
        assert!(!marker.exists());
        assert_eq!(h.call(artifact, b"lastFailure", &[]), nil);
        assert_eq!(behavior_of(&h, desc), h.named(b"ArtifactDescriptorVt"));
        for (name, bytes) in [
            (b"kind".as_slice(), b"module".as_slice()),
            (b"identity", b"candidate"),
        ] {
            assert_eq!(h.call(desc, name, &[]), id.intern_name(bytes).unwrap());
        }
        for (name, bytes) in [
            (b"artifactAbi".as_slice(), abi.as_slice()),
            (b"target", target.as_slice()),
            (b"sourceDigest", b"source-digest"),
            (b"revisionDigest", b"revision-digest"),
            (b"path", path.as_os_str().as_bytes()),
        ] {
            assert_eq!(h.bytes(h.call(desc, name, &[])), bytes);
        }
        let integer = |n| {
            h.keep(object(send(
                id,
                h.named(b"Integer"),
                b"fromWord:",
                &[Operand::word(n)],
            )))
        };
        let zero = integer(0);
        let one = integer(1);
        let negative = integer(usize::MAX);
        for (list, count, at, expected, strings) in [
            (
                b"roots".as_slice(),
                b"rootsCount".as_slice(),
                b"rootAt:".as_slice(),
                vec![b"candidate".as_slice()],
                false,
            ),
            (
                b"requires",
                b"requiresCount",
                b"requireAt:",
                vec![b"depA".as_slice(), b"depB".as_slice()],
                false,
            ),
            (
                b"nativeRequires",
                b"nativeRequiresCount",
                b"nativeRequireAt:",
                vec![b"libdep.dylib".as_slice()],
                true,
            ),
            (
                b"capabilities",
                b"capabilitiesCount",
                b"capabilityAt:",
                vec![b"activate".as_slice()],
                false,
            ),
        ] {
            assert_eq!(h.number(h.call(desc, count, &[])), expected.len());
            let vector = h.call(desc, list, &[]);
            assert_eq!(h.number(h.call(vector, b"size", &[])), expected.len());
            for (i, want) in expected.iter().enumerate() {
                let index = integer(i);
                for value in [h.call(desc, at, &[index]), h.call(vector, b"at:", &[index])] {
                    if strings {
                        assert_eq!(h.bytes(value), *want);
                    } else {
                        assert_eq!(value, id.intern_name(want).unwrap());
                    }
                }
            }
            for args in [
                vec![],
                vec![negative],
                vec![integer(expected.len())],
                vec![nil],
                vec![zero, one],
            ] {
                assert_eq!(h.call(desc, at, &args), nil);
            }
        }
        assert!(forwards.iter().all(|f| f.calls.get() > 0));
        // Native failure travels unchanged and doesn't overwrite language diagnostics.
        forwards[0].fail.set(true);
        let e = id
            .send_named(artifact, b"inspect:", &[Operand::object(path_obj)])
            .unwrap_err();
        assert_eq!((e.status, e.detail), (881, 23));
        forwards[0].fail.set(false);
        // Drop all caller roots for new input/result objects; only adapter scopes protect construction.
        let transient_path = object(send(
            id,
            h.named(b"String"),
            b"fromBytes:",
            &[Operand::bytes(path.as_os_str().as_bytes())],
        ));
        let transient = object(send(
            id,
            artifact,
            b"inspect:",
            &[Operand::object(transient_path)],
        ));
        let token = retain(id, h.roots, transient);
        collect(id, h.gc);
        assert!(present(id, transient));
        assert!(!present(id, transient_path));
        assert_eq!(
            h.bytes(h.call(transient, b"path", &[])),
            path.as_os_str().as_bytes()
        );
        release(id, h.roots, token);
        collect(id, h.gc);
        assert!(!present(id, transient));
        for (vt, name, original) in originals {
            set(&h, vt, name, original);
        }

        // Old C-string symbol projection remains; String projections retain every byte.
        let mut unusual = good.clone();
        let start = unusual
            .windows(b"candidate".len())
            .position(|w| w == b"candidate")
            .unwrap();
        unusual[start + 4] = 0;
        let digest = unusual
            .windows(b"source-digest".len())
            .position(|w| w == b"source-digest")
            .unwrap();
        unusual[digest + 3] = 0;
        let odd_path = dir.join("unusual.bin");
        std::fs::write(&odd_path, &unusual).unwrap();
        let odd = h.call(
            artifact,
            b"inspect:",
            &[h.string(odd_path.as_os_str().as_bytes())],
        );
        assert_eq!(
            h.call(odd, b"identity", &[]),
            id.intern_name(b"cand").unwrap()
        );
        assert_eq!(
            h.bytes(h.call(odd, b"sourceDigest", &[])),
            b"sou\0ce-digest"
        );
        std::fs::write(&odd_path, fixture(abi, target, &[b"bad\0name"])).unwrap();
        assert_eq!(
            h.call(
                artifact,
                b"inspect:",
                &[h.string(odd_path.as_os_str().as_bytes())]
            ),
            nil
        );
        failure(b"nativeRequirementUnavailable");
        // Descriptor clone uses ObjectModel's existing layout/tracing, with no custom hook.
        let cloned = h.call(desc, b"clone", &[]);
        collect(id, h.gc);
        assert_eq!(
            h.call(cloned, b"identity", &[]),
            id.intern_name(b"candidate").unwrap()
        );
        // Artifact's ordinary prototype owner supports authored MOP publication.
        h.eval(b"[Reflect defineMethod: #artifactMigrationProbe on: Artifact as: [:self | 17]]");
        assert_eq!(h.number(h.eval(b"[Artifact artifactMigrationProbe]")), 17);

        // Ordinary reflection replacement affects existing descriptors; reload preserves it.
        let vt = h.named(b"ArtifactDescriptorVt");
        let identity = binding(&h, vt, b"identity");
        let kind = binding(&h, vt, b"kind");
        set(&h, vt, b"identity", kind);
        assert_eq!(
            h.call(desc, b"identity", &[]),
            id.intern_name(b"module").unwrap()
        );
        assert_eq!(load(id, "artifact", &[Operand::object(ns)]), artifact);
        assert_eq!(h.number(h.eval(b"[Artifact artifactMigrationProbe]")), 17);
        assert_eq!(
            h.call(desc, b"identity", &[]),
            id.intern_name(b"module").unwrap()
        );
        set(&h, vt, b"identity", identity);
        // Compiled source observes the public adapter without activation.
        h.bind(b"ArtifactTestPath", path_obj);
        let answer = h.eval(b"[[Artifact inspect: ArtifactTestPath] identity]");
        assert_eq!(answer, id.intern_name(b"candidate").unwrap());
        assert!(!marker.exists());
        std::fs::remove_dir_all(&dir).unwrap();
        for token in h.tokens.borrow_mut().drain(..) {
            release(id, h.roots, token);
        }
    }
}
