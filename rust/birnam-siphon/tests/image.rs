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
            floor: CompiledFloor::pin_lang_only(id, ns, named(b"Roots")),
        }
    }
}
unsafe fn global(h: &H, n: &[u8]) -> Oop {
    unsafe { h.call(h.named(b"Globals"), b"at:", &[h.id.intern_name(n).unwrap()]) }
}
const IMAGE_WORD_BITS: [usize; 4] = [0, i64::MAX as usize, 1usize << 63, usize::MAX];
const IMAGE_FLOAT_BITS: [usize; 4] = [
    0x8000000000000000,
    0x7ff8000000000042,
    0x7ff0000000000000,
    0x7fefffffffffffff,
];
unsafe fn numeric_image_values(h: &H, saving: bool) {
    unsafe {
        for (name, family, bits) in [
            (
                b"ImageWords".as_slice(),
                b"Word".as_slice(),
                IMAGE_WORD_BITS,
            ),
            (
                b"ImageFloats".as_slice(),
                b"Float".as_slice(),
                IMAGE_FLOAT_BITS,
            ),
        ] {
            if saving {
                let mut values = Vec::new();
                for value in bits {
                    values.push(h.keep(object(send(
                        h.id,
                        h.named(family),
                        b"fromBits:",
                        &[Operand::word(value)],
                    ))));
                }
                values.push(values[3]);
                h.call(
                    h.named(b"Globals"),
                    b"at:put:",
                    &[h.id.intern_name(name).unwrap(), h.vector(&values)],
                );
            }
            let values = global(h, name);
            for (index, value) in bits.into_iter().enumerate() {
                let restored = h.call(values, b"at:", &[h.int(index as i64)]);
                assert_eq!(send(h.id, restored, b"bits", &[]).as_word(), Some(value));
            }
            assert_eq!(
                h.call(values, b"at:", &[h.int(3)]),
                h.call(values, b"at:", &[h.int(4)])
            );
        }
        let words = global(h, b"ImageWords");
        let boundary = h.call(words, b"at:", &[h.int(1)]);
        assert_eq!(
            h.number(h.call(boundary, b"asInteger", &[])),
            i64::MAX as usize
        );
        let overflow = h.call(words, b"at:", &[h.int(3)]);
        let error = h.call(overflow, b"asInteger", &[]);
        assert_eq!(
            h.call(error, b"kind", &[]),
            h.id.intern_name(b"overflow").unwrap()
        );
        assert_eq!(
            h.bytes(h.call(error, b"message", &[])),
            b"word out of Integer range"
        );
        if saving {
            h.call(
                h.named(b"Globals"),
                b"at:put:",
                &[h.id.intern_name(b"ImageNumericError").unwrap(), error],
            );
        }
        let saved_error = global(h, b"ImageNumericError");
        assert_eq!(
            h.call(saved_error, b"kind", &[]),
            h.call(error, b"kind", &[])
        );
        assert_eq!(
            h.bytes(h.call(saved_error, b"message", &[])),
            h.bytes(h.call(error, b"message", &[]))
        );
    }
}
// Compare the same post-Store-clear resident state in an independent process.
// No fixture code or snapshot is created in the control; it cannot retain a
// fixture mapping merely to make a physical count match.
fn resident_units(test: &str, mode: &str) -> usize {
    if let Ok(value)=std::env::var("BIRNAM_IMAGE_RESIDENT_EXPECTED") { return value.parse().unwrap(); }
    let path = std::env::temp_dir().join(format!("birnam-resident-control-{}.txt",std::process::id()));
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact",test,"--ignored","--nocapture"])
        .env_remove("BIRNAM_IMAGE_TEST_MODE")
        .env_remove("BIRNAM_IMAGE_CHAIN_EPOCH")
        .env("BIRNAM_IMAGE_RESIDENT_CONTROL",&path)
        .env("BIRNAM_IMAGE_RESIDENT_MODE",mode).status().unwrap();
    assert!(status.success(), "resident cleanup control failed");
    let count = std::fs::read_to_string(&path).unwrap().trim().parse().unwrap();
    std::fs::remove_file(path).unwrap(); count
}
unsafe fn resident_control() -> bool {
    let Some(path)=std::env::var_os("BIRNAM_IMAGE_RESIDENT_CONTROL") else { return false; };
    unsafe {
        let h=harness();
        let mode=std::env::var("BIRNAM_IMAGE_RESIDENT_MODE").unwrap();
        if mode != "source" {
            let image=std::path::PathBuf::from(&path).with_extension("image");
            let filename=h.string(image.as_os_str().as_bytes());
            assert_eq!(h.call(h.named(b"System"),b"saveImage:",&[filename]),h.named(b"true"));
            h.call(h.named(b"System"),b"loadImage:rebuild:",&[filename,h.named(if mode=="rebuild" {b"true"} else {b"false"})]);
            std::fs::remove_file(image).unwrap();
        }
        clear_source_bindings(h.id, h.ns);
        h.call(h.named(b"Store"),b"imageLoadBegin",&[]);
        h.call(h.named(b"Codegen"),b"evictMaterializations",&[]);
        h.floor.restore_lang();h.clear_from(0);h.collect();
        std::fs::write(path,format!("{}\n",h.units())).unwrap();
    }
    true
}
#[test]
#[ignore = "requires production native modules"]
fn image_owner_roundtrip() {
    if unsafe { resident_control() } { return; }
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
            h.eval(b"[Reflect defineMethod: #imageTransient on: Integer as: [:self | #transient]]; [Reflect defineMethod: #imageTransientCollection on: Collection as: [:self | #transient]]");
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
            // Neither recovery mode evaluates source or reruns initializers.
            // Rebuilding compiles canonical portable programs through its owner.
            let codegen_vt = behavior_of(&h, h.named(b"Codegen"));
            for selector in [
                b"eval:".as_slice(),
                b"evalPersisting:",
                b"evalPersisting:from:",
                b"compile:",
                b"compile:output:",
                b"compile:output:from:",
                b"compile:output:root:",
            ] {
                binding(&h, codegen_vt, selector);
                set(
                    &h,
                    codegen_vt,
                    selector,
                    NativeEntry::new(deny_compile, core::ptr::null_mut()),
                );
            }
            if mapped {
                for selector in [
                    b"compilePortableImageCode:bindings:selection:".as_slice(),
                    b"beginPortableImageRebuild:bindings:selection:",
                ] {
                    binding(&h, codegen_vt, selector);
                    set(
                        &h,
                        codegen_vt,
                        selector,
                        NativeEntry::new(deny_compile, core::ptr::null_mut()),
                    );
                }
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
            assert_eq!(
                h.call(
                    h.call(h.named(b"Vector"), b"new", &[]),
                    b"imageInherited",
                    &[]
                ),
                h.id.intern_name(b"inherited").unwrap()
            );
            assert_eq!(
                send(
                    h.id,
                    h.id.receiver,
                    b"bindingOf:in:",
                    &[
                        Operand::object(h.id.intern_name(b"imageTransientCollection").unwrap()),
                        Operand::object(h.named(b"Collection"))
                    ]
                )
                .kind(),
                birnam_abi::id_protocol::UNIT
            );
            assert!(collecting.calls.get() > 0);
            set(&h, model_vt, b"restoreImageSlotAt:in:put:", slot_entry);
            let receiver = global(&h, b"ImageSystem");
            assert_ne!(receiver, system);
            assert_eq!(receiver, global(&h, b"ImageSystemAlias"));
            assert_eq!(h.number(h.call(receiver, b"imageSystemMarker", &[])), 83);
            h.call(receiver, b"gc", &[]);
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
            numeric_image_values(&h, false);
            let wide = global(&h, b"ImageWideVector");
            assert_eq!(h.number(h.call(wide, b"size", &[])), 130);
            for index in 0..130 {
                assert_eq!(h.number(h.call(wide, b"at:", &[h.int(index)])), index as usize);
            }

            let range = global(&h, b"ImageRange");
            assert_eq!(range, global(&h, b"ImageRangeAlias"));
            assert_eq!(h.number(h.call(range, b"size", &[])), 5);
            assert_eq!(h.number(h.call(range, b"at:", &[h.int(2)])), 0);
            let reversed = h.call(range, b"reversed", &[]);
            for index in 0..5 {
                assert_eq!(h.number(h.call(reversed, b"at:", &[h.int(index)])) as i64, 2 - index);
            }
            let typed = global(&h, b"ImageTypedMap");
            assert_eq!(typed, global(&h, b"ImageTypedMapAlias"));
            assert_eq!(h.number(h.call(typed, b"size", &[])), 5);
            let negative_zero = h.keep(object(send(h.id, h.named(b"Float"), b"fromBits:", &[Operand::word(1usize << 63)])));
            for (key, expected) in [
                (h.int(0), b"integer".as_slice()),
                (negative_zero, b"float"),
                (h.id.intern_name(b"alpha").unwrap(), b"symbol"),
                (h.string(b"alpha"), b"string"),
                (h.string("café".as_bytes()), b"utf8"),
            ] {
                assert_eq!(h.bytes(h.call(typed, b"at:", &[key])), expected);
            }
            let updated = h.call(typed, b"at:put:", &[h.string(b"alpha"), h.string(b"changed")]);
            assert_eq!(h.bytes(h.call(updated, b"at:", &[h.string(b"alpha")])), b"changed");
            assert_eq!(h.bytes(h.call(typed, b"at:", &[h.string(b"alpha")])), b"string");

            let outer = global(&h, b"ImageOuter");
            let inner = h.call(outer, b"value:", &[h.int(52)]);
            assert_eq!(h.number(h.call(inner, b"value", &[])), 52);
            let boxed = global(&h, b"ImageBox");
            assert_eq!(boxed, global(&h, b"ImageBoxAlias"));
            assert_eq!(h.number(h.call(boxed, b"imageBoxMarker", &[])), 61);
            let equal_a = global(&h, b"ImageEqualA");
            let equal_b = global(&h, b"ImageEqualB");
            assert_ne!(equal_a, equal_b);
            assert_eq!(h.bytes(equal_a), h.bytes(equal_b));

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
            let target = global(&h, b"ImageTarget");
            let child = global(&h, b"ImageChild");
            let grand = global(&h, b"ImageGrand");
            let sibling = global(&h, b"ImageSibling");
            assert_eq!(behavior_of(&h, grand), behavior_of(&h, child));
            assert_eq!(behavior_of(&h, sibling), behavior_of(&h, target));
            assert_ne!(behavior_of(&h, child), behavior_of(&h, target));
            assert_eq!(
                object(send(
                    h.id,
                    h.id.receiver,
                    b"parentOf:",
                    &[Operand::object(behavior_of(&h, child))]
                )),
                behavior_of(&h, target)
            );
            assert_eq!(h.number(h.call(grand, b"answer:", &[h.int(40)])), 42);
            assert_eq!(h.number(h.call(grand, b"extra", &[])), 9);
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
            let keys = global(&h, b"ImageMapKeys");
            for name in [b"ImageMap".as_slice(), b"ImageMapAlias", b"ImageMapChild"] {
                let map = global(&h, name);
                assert_eq!(h.number(h.call(map, b"imageMapMarker", &[])), 73);
                for index in 0..24 {
                    let key = h.call(keys, b"at:", &[h.int(index)]);
                    assert_eq!(h.call(map, b"at:", &[key]), closure,
                        "private/shared Map identity key {index} must be rehashed");
                }
            }
            assert_eq!(h.number(h.call(global(&h, b"ImageMapChild"), b"imageMapChildMarker", &[])), 91);
            let cell = global(&h, b"ImageCell");
            assert_eq!(h.number(h.call(cell, b"value", &[])), 2);
            assert_eq!(h.number(h.call(cell, b"imageCellMarker", &[])), 73);
            assert_eq!(send(h.id, h.id.receiver, b"bindingOf:in:", &[
                Operand::object(h.id.intern_name(b"imageCellMarker").unwrap()),
                Operand::object(h.named(b"StoreVt")),
            ]).kind(), birnam_abi::id_protocol::UNIT,
                "Cell methods must not be restored on their native Store owner");
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
            let replacement = h.call(
                behavior_of(&h, target),
                b"methodFor:",
                &[h.id.intern_name(b"alternateAnswer:").unwrap()],
            );
            h.call(
                h.named(b"Store"),
                b"addMethod:on:with:",
                &[
                    h.id.intern_name(b"answer:").unwrap(),
                    behavior_of(&h, target),
                    replacement,
                ],
            );
            h.collect();
            assert_eq!(h.number(h.call(grand, b"answer:", &[h.int(40)])), 43);
            assert_eq!(h.number(h.call(sibling, b"answer:", &[h.int(40)])), 43);
            send(
                h.id,
                behavior_of(&h, h.named(b"Integer")),
                b"removeMethod:",
                &[Operand::object(h.id.intern_name(b"imageFixture").unwrap())],
            );
            send(
                h.id,
                h.named(b"Collection"),
                b"removeMethod:",
                &[Operand::object(
                    h.id.intern_name(b"imageInherited").unwrap(),
                )],
            );
            send(
                h.id,
                h.named(b"CellVt"),
                b"removeMethod:",
                &[Operand::object(h.id.intern_name(b"imageCellMarker").unwrap())],
            );
            clear_source_bindings(h.id, h.ns);
            h.call(h.named(b"Store"), b"imageLoadBegin", &[]);
            h.floor.restore_lang();
            h.clear_from(0);
            h.collect();
            assert_eq!(
                h.units(),
                resident_units("image_owner_roundtrip", if mode == "rebuild" || mode == "stripped" { "rebuild" } else { "mapped" }),
                "restored fixture mappings reclaim against equivalent resident cleanup"
            );
            return;
        }
        numeric_image_values(&h, true);
        let mut wide = h.call(h.named(b"Vector"), b"new", &[]);
        for index in 0..130 {
            wide = h.call(wide, b"add:", &[h.int(index)]);
        }
        h.call(h.named(b"Globals"), b"at:put:", &[h.id.intern_name(b"ImageWideVector").unwrap(), wide]);

        h.eval(r#"let ImageRange := [-2 to: 2]; let ImageRangeAlias := ImageRange; let ImageTypedMap := {0 -> "integer", 0.0 -> "float", #alpha -> "symbol", "alpha" -> "string", "café" -> "utf8"}; let ImageTypedMapAlias := ImageTypedMap"#.as_bytes());

        h.eval(b"let ImageBox := [\"special\" , \"\"]; let ImageBoxAlias := ImageBox; [Reflect defineMethod: #imageBoxMarker on: ImageBox as: [:self | 61]]; let ImageEqualA := [\"equal\" , \"\"]; let ImageEqualB := [\"equal\" , \"\"]");
        // CellVt has Store as its native owner, but hosts instance methods
        // directly. Image binding targets must preserve that distinction.
        h.eval(b"[Reflect defineMethod: #imageCellMarker on: CellVt as: [:self | 73]]");
        h.eval(b"let ImageShared := [Object clone]; let ImageAlias := ImageShared; let ImageTarget := [Object clone]; [Reflect defineMethod: #answer: on: ImageTarget as: [:self :value | [value + 2]]]; let ImageClosure := [| ImageShared]");
        h.eval(b"[Reflect defineMethod: #alternateAnswer: on: ImageTarget as: [:self :value | [value + 3]]]; let ImageChild := [ImageTarget clone]; [Reflect defineMethod: #extra on: ImageChild as: [:self | 9]]; let ImageGrand := [ImageChild clone]; let ImageSibling := [ImageTarget clone]");
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
            (b"ImageOuter", maker),
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
        let mut map = h.call(map, b"at:put:", &[shared, closure]);
        let mut keys = h.call(h.named(b"Vector"), b"new", &[]);
        for _ in 0..24 {
            let key = h.call(h.named(b"Object"), b"clone", &[]);
            keys = h.call(keys, b"add:", &[key]);
            map = h.call(map, b"at:put:", &[key, closure]);
        }
        for (name, value) in [(b"ImageCycle".as_slice(), cycle), (b"ImageMap", map), (b"ImageMapKeys", keys)] {
            h.call(
                h.named(b"Globals"),
                b"at:put:",
                &[h.id.intern_name(name).unwrap(), value],
            );
        }
        h.eval(b"[Reflect defineMethod: #imageMapMarker on: ImageMap as: [:self | 73]]");
        let map_behavior = behavior_of(&h, map);
        for name in [b"ImageMapAlias".as_slice(), b"ImageMapChild"] {
            let copy = h.call(map, b"at:put:", &[h.id.intern_name(name).unwrap(), closure]);
            send(h.id, h.id.receiver, b"reassociateBehaviorOf:with:", &[
                Operand::object(copy), Operand::object(map_behavior),
            ]);
            h.call(h.named(b"Globals"), b"at:put:", &[h.id.intern_name(name).unwrap(), copy]);
            assert_eq!(h.number(h.call(copy, b"imageMapMarker", &[])), 73);
        }
        h.eval(b"[Reflect defineMethod: #imageMapChildMarker on: ImageMapChild as: [:self | 91]]");
        assert_eq!(h.number(h.call(global(&h, b"ImageMapChild"), b"imageMapChildMarker", &[])), 91);

        h.eval(b"[db assert: #tracked attr: #history value: true]; [db assert: #imageHistory attr: #tracked value: 1]; [db assert: #imageHistory attr: #tracked value: 2]; [Reflect defineMethod: #imageFixture on: Integer as: [:self | #saved]]");
        h.eval(b"let ImageCell := [db cellFor: #imageHistory attr: #tracked]; let ImageCellBlock := [[:cell | [| [cell value]]] value: ImageCell]");
        h.eval(b"let ImageSystem := [System clone]; let ImageSystemAlias := ImageSystem; [Reflect defineMethod: #imageSystemMarker on: ImageSystem as: [:self | 83]]");
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
        h.eval(b"[Reflect defineMethod: #imageInherited on: Collection as: [:self | #inherited]]");
        image_bindings_root_behavior_snapshot_before_allocation(&h);
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
        // Native-free images rebuild from the same validated inventories and
        // still expose a stable composition identity for generation recovery.
        let bytes = std::fs::read(&path).unwrap();
        let mut offset = 28;
        while u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) != 4 {
            let size =
                u64::from_le_bytes(bytes[offset + 4..offset + 12].try_into().unwrap()) as usize;
            offset += 12 + size;
        }
        let stripped = path.with_extension("stripped.image");
        std::fs::write(&stripped, &bytes[..offset]).unwrap();
        for mode in ["mapped", "rebuild", "stripped"] {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "image_owner_roundtrip",
                    "--nocapture",
                ])
                .env("BIRNAM_IMAGE_TEST_MODE", mode)
                .env(
                    "BIRNAM_IMAGE_TEST_PATH",
                    if mode == "stripped" { &stripped } else { &path },
                )
                .status()
                .unwrap();
            assert!(status.success(), "{mode} image child failed");
        }
        std::fs::remove_file(stripped).unwrap();
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
        h.eval(b"[Reflect defineMethod: #imageFixture on: Integer as: [:self | #before]]; [Reflect defineMethod: #imageTransient on: Integer as: [:self | #transient]]; [Reflect defineMethod: #imageInherited on: Collection as: [:self | #beforeInherited]]");
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
        assert_eq!(
            h.call(
                h.call(h.named(b"Vector"), b"new", &[]),
                b"imageInherited",
                &[]
            ),
            h.id.intern_name(b"beforeInherited").unwrap()
        );
        let original_bytes = std::fs::read(&path).unwrap();
        // An ordinary source capability must not silently lose its transient
        // state. Admission follows the Store declaration, not a prototype name.
        h.eval(b"[db assert: #imageTransientObject attr: #history value: false]; [db assert: #imageTransientObject attr: #imageTransient value: true]; [[| let ephemeral := [Object clone]; [db assert: ephemeral attr: #imageTransientObject value: true]; [Globals at: #ImageEphemeral put: #[ephemeral]]] value]");
        assert_eq!(h.call(system, b"saveImage:", &[filename]), nil);
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        h.eval(b"[Globals at: #ImageEphemeral put: nil]");
        assert_eq!(h.call(system, b"saveImage:", &[filename]), h.named(b"true"));
        let original_bytes = std::fs::read(&path).unwrap();
        let machine = h.call(h.named(b"Machine"), b"new", &[]);
        let native_method = h.call(
            h.named(b"Integer"),
            b"methodFor:",
            &[h.id.intern_name(b"+").unwrap()],
        );
        let block_method = h.call(
            closure,
            b"methodFor:",
            &[h.id.intern_name(b"value").unwrap()],
        );
        for capability in [machine, native_method, block_method] {
            for private in [false, true] {
                if private {
                    h.call(
                        h.named(b"ObjectModel"),
                        b"ensurePrivateVtableFor:",
                        &[capability],
                    );
                }
                h.call(
                    h.named(b"Globals"),
                    b"at:put:",
                    &[h.id.intern_name(b"ImageUnsupported").unwrap(), capability],
                );
                assert_eq!(h.call(system, b"saveImage:", &[filename]), nil);
                assert_eq!(
                    std::fs::read(&path).unwrap(),
                    original_bytes,
                    "failed capability save preserves destination"
                );
            }
        }
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
// Collection during Vector construction used to compact Id's behavior registry
// underneath Codegen's numeric-index enumeration.
unsafe fn image_bindings_root_behavior_snapshot_before_allocation(h: &H) {
    unsafe {
        h.collect();
        let vector = h.named(b"Vector");
        let vt = behavior_of(&h, vector);
        let original = binding(&h, vt, b"new");
        let alias = h.id.intern_name(b"imageTestVectorNew").unwrap();
        set(&h, vt, b"imageTestVectorNew", original);
        let mut hook = CollectingRestore {
            id: h.id,
            gc: h.gc,
            alias,
            calls: Cell::new(0),
        };
        // These behaviors have no persistent owner; collection can retire them.
        for _ in 0..8 {
            let o = object(send(h.id, h.named(b"Object"), b"clone", &[]));
            send(
                h.id,
                h.id.receiver,
                b"ensurePrivateVtableFor:",
                &[Operand::object(o)],
            );
        }
        set(
            &h,
            vt,
            b"new",
            NativeEntry::new(
                collect_before_vector,
                (&mut hook as *mut CollectingRestore).cast(),
            ),
        );
        let result = h.call(h.named(b"Codegen"), b"portableImageBindings", &[]);
        set(&h, vt, b"new", original);
        assert!(hook.calls.get() > 0);
        assert!(h.number(h.call(result, b"size", &[])) > 0);
    }
}
unsafe extern "C" fn collect_before_vector(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    (|| unsafe {
        let s = &*c.cast::<CollectingRestore>();
        if s.calls.replace(s.calls.get() + 1) == 0 {
            s.id.send_named(s.gc, b"collectPrecisely", &[])?;
        }
        s.id.send(r, s.alias, borrowed_slice(a, n).unwrap())
    })()
    .map_or_else(|e| e, Outcome::loaded)
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
        let call = s.calls.get();
        if call <= 8 || call.is_power_of_two() {
            s.id.send_named(s.gc, b"collectPrecisely", &[])?;
        }
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
#[test]
#[ignore = "requires production native modules"]
fn repeated_image_resaves_keep_a_fixed_live_set() {
    if unsafe { resident_control() } { return; }
    const EPOCHS: usize = 20;
    if let Ok(epoch) = std::env::var("BIRNAM_IMAGE_CHAIN_EPOCH") {
        let epoch: usize = epoch.parse().unwrap();
        let path = std::path::PathBuf::from(std::env::var_os("BIRNAM_IMAGE_CHAIN_PATH").unwrap());
        unsafe {
            let h = harness();
            let system = h.named(b"System");
            let store = h.named(b"Store");
            let globals = h.named(b"Globals");
            let codegen = h.named(b"Codegen");
            let filename = h.string(path.as_os_str().as_bytes());
            let saved_roots = h.tokens.borrow().len();
            if epoch == 0 {
                h.eval(b"let ChainValue := 0; let ChainClosure := [:value | [value + 1]]; [db assert: #chainValue attr: #history value: true]");
            } else {
                let rebuild = epoch % 2 == 0;
                assert_eq!(
                    h.call(
                        system,
                        b"loadImage:rebuild:",
                        &[filename, h.named(if rebuild { b"true" } else { b"false" })]
                    ),
                    h.named(if rebuild { b"false" } else { b"true" })
                );
                let previous = global(&h, b"ChainValue");
                assert_eq!(h.number(previous), epoch - 1);
                let next = h.call(global(&h, b"ChainClosure"), b"value:", &[previous]);
                assert_eq!(h.number(next), epoch);
                h.call(
                    globals,
                    b"at:put:",
                    &[h.id.intern_name(b"ChainValue").unwrap(), next],
                );
            }
            // Every epoch changes the same historical fact, then drops old
            // history. Both the restored closure and latest value stay live.
            let entity = h.keep(h.id.intern_name(b"chainEntity").unwrap());
            let attribute = h.keep(h.id.intern_name(b"chainValue").unwrap());
            h.call(store, b"retainCurrentOnly", &[]);
            for value in 0..1000 {
                let mark = h.tokens.borrow().len();
                h.call(
                    store,
                    b"assert:attr:value:",
                    &[entity, attribute, h.int(value)],
                );
                h.clear_from(mark);
            }
            h.call(store, b"compact", &[]);
            h.call(codegen, b"evictMaterializations", &[]);
            h.clear_from(saved_roots);
            h.collect();
            assert_eq!(h.call(system, b"saveImage:", &[filename]), h.named(b"true"));
            h.clear_from(saved_roots);
            h.collect();
            let facts = h.number(h.call(store, b"retainedFactCount", &[]));
            let fact_bytes = h.number(h.call(store, b"retainedFactBytes", &[]));
            let units = h.units();
            assert!(units > 0, "the saved closure must retain executable code");
            std::fs::write(
                path.with_extension(format!("epoch-{epoch}")),
                format!(
                    "{facts} {fact_bytes} {units} {}\n",
                    std::fs::metadata(&path).unwrap().len()
                ),
            )
            .unwrap();
            // Clearing Store/Globals and the last external root must release
            // fixture mappings after restoring the resident compiled core.
            clear_source_bindings(h.id, h.ns);
            h.call(store, b"imageLoadBegin", &[]);
            h.call(codegen, b"evictMaterializations", &[]);
            h.floor.restore_lang();
            h.clear_from(0);
            h.collect();
            assert_eq!(h.units(), resident_units("repeated_image_resaves_keep_a_fixed_live_set", if epoch==0 {"source"} else if epoch%2==0 {"rebuild"} else {"mapped"}), "epoch {epoch} retained fixture mappings");
        }
        return;
    }
    let directory = std::env::temp_dir().join(format!("birnam-image-chain-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("current.image");
    let mut baselines: [Option<Vec<usize>>; 2] = [None, None];
    let resident = [resident_units("repeated_image_resaves_keep_a_fixed_live_set", "source"),
        resident_units("repeated_image_resaves_keep_a_fixed_live_set", "mapped"),
        resident_units("repeated_image_resaves_keep_a_fixed_live_set", "rebuild")];
    for epoch in 0..=EPOCHS {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "repeated_image_resaves_keep_a_fixed_live_set",
                "--ignored",
                "--nocapture",
            ])
            .env("BIRNAM_IMAGE_RESIDENT_EXPECTED", resident[if epoch==0 {0} else if epoch%2==1 {1} else {2}].to_string())
            .env("BIRNAM_IMAGE_CHAIN_EPOCH", epoch.to_string())
            .env("BIRNAM_IMAGE_CHAIN_PATH", &path)
            .status()
            .unwrap();
        assert!(
            status.success(),
            "Image chain epoch {epoch} failed; evidence in {}",
            directory.display()
        );
        let row: Vec<usize> =
            std::fs::read_to_string(path.with_extension(format!("epoch-{epoch}")))
                .unwrap()
                .split_whitespace()
                .map(|v| v.parse().unwrap())
                .collect();
        assert_eq!(row.len(), 4);
        if epoch >= 3 {
            let baseline = &mut baselines[epoch % 2];
            if let Some(expected) = baseline {
                assert_eq!(&row, expected, "Image chain grew in epoch {epoch}");
            } else {
                *baseline = Some(row);
            }
        }
    }
    std::fs::remove_dir_all(directory).unwrap();
}
