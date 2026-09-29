#![allow(dead_code)]
//! Image-v8 save/load through production owner messages.
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
                .unwrap()
        }
    }
    unsafe fn collect(&self) {
        unsafe {
            collect(self.id, self.gc);
            send(self.id, self.named(b"TCC"), b"checkpoint", &[]);
        }
    }
}
unsafe extern "C" fn deny_compile(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
    Outcome::failed(982, 17)
}
unsafe fn harness() -> H {
    unsafe {
        let id = id();
        let ns = load(
            id,
            "bootstrap",
            &[Operand::bytes(directory().as_os_str().as_bytes())],
        );
        let named = |n: &[u8]| {
            object(send(
                id,
                ns,
                b"at:",
                &[Operand::object(id.intern_name(n).unwrap())],
            ))
        };
        H {
            id,
            ns,
            roots: named(b"Roots"),
            gc: named(b"GC"),
            tokens: RefCell::new(Vec::new()),
        }
    }
}
unsafe fn global(h: &H, n: &[u8]) -> Oop {
    unsafe { h.call(h.named(b"Globals"), b"at:", &[h.id.intern_name(n).unwrap()]) }
}
#[test]
#[ignore = "requires production native modules"]
fn image_owner_roundtrip() {
    unsafe {
        let h = harness();
        let system = h.named(b"System");
        let nil = h.named(b"nil");
        let path = std::env::var_os("BIRNAM_IMAGE_TEST_PATH")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::env::temp_dir()
                    .join(format!("birnam-message-image-{}.image", std::process::id()))
            });
        let filename = h.string(path.as_os_str().as_bytes());
        if let Ok(mode) = std::env::var("BIRNAM_IMAGE_TEST_MODE") {
            h.eval(b"[Reflect defineMethod: #imageTransient on: Integer as: [:self | #transient]]");
            let model_vt = behavior_of(&h, h.named(b"ObjectModel"));
            let slot_entry = binding(&h, model_vt, b"restoreImageSlotAt:in:put:");
            let slot_alias = h.id.intern_name(b"imageTestRestoreSlot:in:put:").unwrap();
            set(&h, model_vt, b"imageTestRestoreSlot:in:put:", slot_entry);
            let mut collecting = CollectingRestore {
                id: h.id,
                gc: h.gc,
                alias: slot_alias,
                calls: Cell::new(0),
            };
            set(
                &h,
                model_vt,
                b"restoreImageSlotAt:in:put:",
                NativeEntry::new(
                    collect_restore,
                    (&mut collecting as *mut CollectingRestore).cast(),
                ),
            );
            let mapped = mode == "mapped";
            if mapped {
                let vt = behavior_of(&h, h.named(b"TCC"));
                for selector in [b"compile:".as_slice(), b"compileObject:"] {
                    set(
                        &h,
                        vt,
                        selector,
                        NativeEntry::new(deny_compile, core::ptr::null_mut()),
                    );
                }
            }
            let result = h.call(
                system,
                b"loadImage:rebuild:",
                &[filename, h.named(if mapped { b"false" } else { b"true" })],
            );
            assert_eq!(result, h.named(if mapped { b"true" } else { b"false" }));
            assert_eq!(
                send(
                    h.id,
                    h.id.receiver,
                    b"bindingOf:in:",
                    &[
                        Operand::object(h.id.intern_name(b"imageTransient").unwrap()),
                        Operand::object(behavior_of(&h, h.named(b"Integer")))
                    ]
                )
                .kind(),
                birnam_abi::id_protocol::UNIT
            );
            assert!(collecting.calls.get() > 0);
            set(&h, model_vt, b"restoreImageSlotAt:in:put:", slot_entry);
            let identity = h.bytes(h.call(system, b"imageCompositionIdentity", &[]));
            assert_eq!(identity.len(), 64);
            assert!(identity.iter().all(|b| b.is_ascii_hexdigit()));
            let codegen = h.named(b"Codegen");
            let fixture = h.call(
                codegen,
                b"moduleSourceFor:",
                &[h.id.intern_name(b"fixture").unwrap()],
            );
            assert_eq!(h.bytes(fixture), b"[Module named: #fixture requires: #[]]");
            assert_eq!(
                h.call(
                    codegen,
                    b"moduleSourceFor:",
                    &[h.id.intern_name(b"later").unwrap()]
                ),
                nil
            );
            let shared = global(&h, b"ImageShared");
            assert_eq!(shared, global(&h, b"ImageAlias"));
            assert_eq!(h.bytes(global(&h, b"ImageBytes")), b"asset\0\xffbytes");
            let closure = global(&h, b"ImageClosure");
            assert_eq!(
                h.call(global(&h, b"ImageAnnotated"), b"sourceProvenance", &[]),
                closure
            );
            assert_eq!(h.call(closure, b"value", &[]), shared);
            let sync = global(&h, b"ImageSync");
            assert_eq!(h.call(sync, b"value", &[]), shared);
            assert_eq!(h.call(sync, b"isSynchronous", &[]), h.named(b"true"));
            assert_eq!(
                h.call(closure, b"codeObject", &[]),
                h.call(sync, b"codeObject", &[])
            );
            assert_eq!(
                h.number(h.call(global(&h, b"ImageTarget"), b"answer:", &[h.int(40)])),
                42
            );
            let cyclic = global(&h, b"ImageCycle");
            assert_eq!(
                object(send(
                    h.id,
                    h.named(b"ObjectModel"),
                    b"slotAt:in:",
                    &[Operand::word(0), Operand::object(cyclic)]
                )),
                cyclic
            );
            assert_eq!(
                object(send(
                    h.id,
                    h.named(b"ObjectModel"),
                    b"slotAt:in:",
                    &[Operand::word(1), Operand::object(cyclic)]
                )),
                closure
            );
            assert_eq!(
                h.bytes(h.call(global(&h, b"ImageEmbedded"), b"value", &[])),
                b"compiled asset\0\xff"
            );
            let map = global(&h, b"ImageMap");
            assert_eq!(h.call(map, b"at:", &[shared]), closure);
            let cell = global(&h, b"ImageCell");
            assert_eq!(h.number(h.call(cell, b"value", &[])), 2);
            let cell_block = global(&h, b"ImageCellBlock");
            assert_eq!(h.call(cell_block, b"captureAt:", &[h.int(0)]), cell);
            assert_eq!(h.number(h.call(cell_block, b"value", &[])), 2);
            let store = h.named(b"Store");
            let entity = h.id.intern_name(b"imageHistory").unwrap();
            let attr = h.id.intern_name(b"tracked").unwrap();
            assert_eq!(
                h.number(h.call(store, b"history:attr:", &[entity, attr])),
                2
            );
            assert_eq!(
                h.number(h.call(store, b"currentValue:attr:", &[entity, attr])),
                2
            );
            h.collect();
            assert_eq!(h.call(closure, b"value", &[]), shared);
            assert!(h.units() > 0);
            if !mapped {
                let resaved = path.with_extension("resaved");
                assert_eq!(
                    h.call(
                        system,
                        b"saveImage:",
                        &[h.string(resaved.as_os_str().as_bytes())]
                    ),
                    h.named(b"true")
                );
                let status = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--ignored",
                        "--exact",
                        "image_owner_roundtrip",
                        "--nocapture",
                    ])
                    .env("BIRNAM_IMAGE_TEST_MODE", "mapped")
                    .env("BIRNAM_IMAGE_TEST_PATH", &resaved)
                    .status()
                    .unwrap();
                assert!(status.success(), "resaved image child failed");
                std::fs::remove_file(resaved).unwrap();
            }
            send(
                h.id,
                behavior_of(&h, h.named(b"Integer")),
                b"removeMethod:",
                &[Operand::object(h.id.intern_name(b"imageFixture").unwrap())],
            );
            h.call(h.named(b"Store"), b"imageLoadBegin", &[]);
            h.clear_from(0);
            h.collect();
            assert_eq!(
                h.units(),
                0,
                "all restored mappings reclaim after final references"
            );
            return;
        }
        h.eval(b"let ImageShared := [Object clone]; let ImageAlias := ImageShared; let ImageTarget := [Object clone]; [Reflect defineMethod: #answer: on: ImageTarget as: [:self :value | [value + 2]]]; let ImageClosure := [| ImageShared]");
        // A real capture, not a global lookup: both dispositions share the CodeObject.
        let maker = h.eval(b"[:held | [| held]]");
        let shared = global(&h, b"ImageShared");
        let closure = h.call(maker, b"value:", &[shared]);
        let code = h.call(closure, b"codeObject", &[]);
        let annotated = h.call(
            h.named(b"CodeObject"),
            b"fromProgram:provenance:",
            &[h.call(code, b"program", &[]), closure],
        );
        let sync = h.materialize(code, &[shared], false);
        for (name, value) in [
            (b"ImageClosure".as_slice(), closure),
            (b"ImageSync", sync),
            (b"ImageBytes", h.encoded(b"asset\0\xffbytes")),
            (b"ImageAnnotated", annotated),
        ] {
            h.call(
                h.named(b"Globals"),
                b"at:put:",
                &[h.id.intern_name(name).unwrap(), value],
            );
        }
        let vt = object(send(h.id, h.named(b"ObjectVt"), b"delegated", &[]));
        h.keep(vt);
        send(
            h.id,
            h.named(b"ObjectModel"),
            b"setLayoutOf:to:rest:",
            &[
                Operand::object(vt),
                Operand::bytes(&[1, 1]),
                Operand::word(0),
            ],
        );
        let cycle = h.keep(object(send(
            h.id,
            h.named(b"ObjectModel"),
            b"allocate:bytes:",
            &[Operand::object(vt), Operand::word(2 * size_of::<usize>())],
        )));
        send(
            h.id,
            h.id.receiver,
            b"ownerOf:put:",
            &[Operand::object(vt), Operand::object(cycle)],
        );
        for (i, v) in [(0, cycle), (1, closure)] {
            send(
                h.id,
                h.named(b"ObjectModel"),
                b"slotAt:in:put:",
                &[Operand::word(i), Operand::object(cycle), Operand::object(v)],
            );
        }
        let map = h.call(h.named(b"Map"), b"new", &[]);
        let map = h.call(map, b"at:put:", &[shared, closure]);
        for (name, value) in [(b"ImageCycle".as_slice(), cycle), (b"ImageMap", map)] {
            h.call(
                h.named(b"Globals"),
                b"at:put:",
                &[h.id.intern_name(name).unwrap(), value],
            );
        }
        h.eval(b"[db assert: #tracked attr: #history value: true]; [db assert: #imageHistory attr: #tracked value: 1]; [db assert: #imageHistory attr: #tracked value: 2]; [Reflect defineMethod: #imageFixture on: Integer as: [:self | #saved]]");
        h.eval(b"let ImageCell := [db cellFor: #imageHistory attr: #tracked]; let ImageCellBlock := [[:cell | [| [cell value]]] value: ImageCell]");
        let codegen = h.named(b"Codegen");
        let source = h.string(b"[Module named: #fixture requires: #[]]");
        assert_eq!(
            h.call(
                codegen,
                b"recordModuleSource:named:",
                &[source, h.id.intern_name(b"fixture").unwrap()]
            ),
            source
        );
        let asset_dir = path.with_extension("assets");
        std::fs::create_dir_all(&asset_dir).unwrap();
        std::fs::write(asset_dir.join("asset.bin"), b"compiled asset\0\xff").unwrap();
        let source = h.string(b"let ImageEmbedded := [| [System embedFile: \"asset.bin\"]]");
        h.call(
            h.named(b"Codegen"),
            b"evalPersisting:from:",
            &[
                source,
                h.string(asset_dir.join("source.birnam").as_os_str().as_bytes()),
            ],
        );
        std::fs::remove_dir_all(asset_dir).unwrap();
        assert_eq!(h.call(system, b"saveImage:", &[filename]), h.named(b"true"));
        h.collect();
        let before = h.units();
        let tcc_vt = behavior_of(&h, h.named(b"TCC"));
        let load_entry = binding(&h, tcc_vt, b"loadObject:");
        set(&h, tcc_vt, b"imageTestLoadObject:", load_entry);
        let mut failed_map = FailingMap {
            id: h.id,
            gc: h.gc,
            codegen,
            alias: h.id.intern_name(b"imageTestLoadObject:").unwrap(),
            calls: Cell::new(0),
        };
        set(
            &h,
            tcc_vt,
            b"loadObject:",
            NativeEntry::new(fail_mapping, (&mut failed_map as *mut FailingMap).cast()),
        );
        let error =
            h.id.send_named(system, b"loadImage:", &[Operand::object(filename)])
                .unwrap_err();
        assert_eq!((error.status, error.detail), (887, 42));
        assert_eq!(failed_map.calls.get(), 2);
        set(&h, tcc_vt, b"loadObject:", load_entry);
        h.collect();
        assert_eq!(
            h.units(),
            before,
            "partial mapping failure releases its units"
        );
        assert_eq!(global(&h, b"ImageShared"), shared);
        for mode in ["mapped", "rebuild"] {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "image_owner_roundtrip",
                    "--nocapture",
                ])
                .env("BIRNAM_IMAGE_TEST_MODE", mode)
                .env("BIRNAM_IMAGE_TEST_PATH", &path)
                .status()
                .unwrap();
            assert!(status.success(), "{mode} image child failed");
        }
        // A failure after a real dictionary write and precise collection restores
        // both the old prototype binding and the original Store state.
        let later = h.string(b"[Module named: #later requires: #[]]");
        assert_eq!(
            h.call(
                codegen,
                b"recordModuleSource:named:",
                &[later, h.id.intern_name(b"later").unwrap()]
            ),
            later
        );
        h.call(codegen, b"sealInitialGeneration", &[]);
        h.eval(b"[Reflect defineMethod: #imageFixture on: Integer as: [:self | #before]]; [Reflect defineMethod: #imageTransient on: Integer as: [:self | #transient]]");
        let codegen = h.named(b"Codegen");
        let codegen_vt = behavior_of(&h, codegen);
        let selector = b"installPortableImageBinding:on:as:source:";
        let original = binding(&h, codegen_vt, selector);
        let alias =
            h.id.intern_name(b"imageTestOriginalInstall:on:as:source:")
                .unwrap();
        set(
            &h,
            codegen_vt,
            b"imageTestOriginalInstall:on:as:source:",
            original,
        );
        let mut failure = FailingPublication {
            id: h.id,
            alias,
            calls: Cell::new(0),
            gc: h.gc,
        };
        set(
            &h,
            codegen_vt,
            selector,
            NativeEntry::new(
                fail_publication,
                (&mut failure as *mut FailingPublication).cast(),
            ),
        );
        let result =
            h.id.send_named(system, b"loadImage:", &[Operand::object(filename)])
                .unwrap_err();
        assert_eq!((result.status, result.detail), (881, 73));
        assert_eq!(failure.calls.get(), 1);
        set(&h, codegen_vt, selector, original);
        assert_eq!(global(&h, b"ImageShared"), shared);
        assert_eq!(
            h.call(h.int(1), b"imageTransient", &[]),
            h.id.intern_name(b"transient").unwrap()
        );
        let original_bytes = std::fs::read(&path).unwrap();
        let machine = h.call(h.named(b"Machine"), b"new", &[]);
        h.call(
            h.named(b"Globals"),
            b"at:put:",
            &[h.id.intern_name(b"ImageUnsupported").unwrap(), machine],
        );
        assert_eq!(h.call(system, b"saveImage:", &[filename]), nil);
        assert_eq!(
            std::fs::read(&path).unwrap(),
            original_bytes,
            "failed save preserves destination"
        );
        assert_eq!(
            h.call(
                codegen,
                b"moduleSourceFor:",
                &[h.id.intern_name(b"later").unwrap()]
            ),
            later
        );
        assert_eq!(
            h.call(
                codegen,
                b"recordModuleSource:named:",
                &[later, h.id.intern_name(b"later").unwrap()]
            ),
            nil
        );
        assert_eq!(
            h.call(h.int(1), b"imageFixture", &[]),
            h.id.intern_name(b"before").unwrap()
        );
        // Structural corruption must be rejected before touching current Globals.
        let mut damaged = std::fs::read(&path).unwrap();
        damaged[28] ^= 1;
        let encoded = h.encoded(&damaged);
        assert_eq!(
            h.call(system, b"loadImageBytes:rebuild:", &[encoded, nil]),
            nil
        );
        assert_eq!(global(&h, b"ImageShared"), shared);
        std::fs::remove_file(path).unwrap();
    }
}
struct FailingPublication {
    id: Protocol,
    alias: Oop,
    calls: Cell<usize>,
    gc: Oop,
}
struct CollectingRestore {
    id: Protocol,
    gc: Oop,
    alias: Oop,
    calls: Cell<usize>,
}
unsafe extern "C" fn collect_restore(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    (|| unsafe {
        let s = &*c.cast::<CollectingRestore>();
        let args = borrowed_slice(a, n).unwrap();
        let result = s.id.send(r, s.alias, args)?;
        s.calls.set(s.calls.get() + 1);
        s.id.send_named(s.gc, b"collectPrecisely", &[])?;
        Ok(result)
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
struct FailingMap {
    id: Protocol,
    gc: Oop,
    codegen: Oop,
    alias: Oop,
    calls: Cell<usize>,
}
unsafe extern "C" fn fail_mapping(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let s = &*c.cast::<FailingMap>();
        let args = borrowed_slice(a, n).unwrap();
        s.calls.set(s.calls.get() + 1);
        s.id.send_named(s.gc, b"collectPrecisely", &[])?;
        assert!(
            s.id.send_named(s.codegen, b"cancelPortableImageUnit", &[])
                .is_err(),
            "image stage rejects reentrant cancellation"
        );
        if s.calls.get() == 2 {
            return Err(Outcome::failed(887, 42));
        }
        s.id.send(r, s.alias, args)
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
unsafe extern "C" fn fail_publication(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    (|| unsafe {
        let f = &*c.cast::<FailingPublication>();
        let args = borrowed_slice(a, n).unwrap();
        f.id.send(r, f.alias, args)?;
        f.calls.set(f.calls.get() + 1);
        f.id.send_named(f.gc, b"collectPrecisely", &[])?;
        Err(Outcome::failed(881, 73))
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
