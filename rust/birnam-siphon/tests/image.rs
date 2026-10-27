//! Image save/load, live owner callbacks and independent code-lifetime controls.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use birnam_host_support::Context;
use std::{cell::RefCell, os::unix::ffi::OsStrExt, path::Path};
struct H {
    host: Context,
    gc: Oop,
    fixture: Oop,
    tokens: RefCell<Vec<Oop>>,
    floor_tokens: RefCell<Vec<Oop>>,
    lang_bindings: Vec<(Oop, Oop, Oop)>,
}
impl H {
    fn named(&self, n: &[u8]) -> Oop {
        self.host.named(n).unwrap()
    }
    fn keep(&self, o: Oop) -> Oop {
        self.tokens
            .borrow_mut()
            .push(self.host.send(self.host.roots, b"retain:", &[o]).unwrap());
        o
    }
    fn floor_keep(&self, o: Oop) -> Oop {
        self.floor_tokens
            .borrow_mut()
            .push(self.host.send(self.host.roots, b"retain:", &[o]).unwrap());
        o
    }
    fn key(&self, n: &[u8]) -> Oop {
        self.keep(self.host.intern(n).unwrap())
    }
    fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        self.keep(self.host.send(r, n, a).unwrap())
    }
    fn native(&self, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.host.id.send(r, self.key(n), a) }
    }
    fn string(&self, b: &[u8]) -> Oop {
        let q = self.host.scope();
        self.keep(q.string(b).unwrap())
    }
    fn raw(&self, b: &[u8]) -> Oop {
        let q = self.host.scope();
        self.keep(q.byte_values(b).unwrap())
    }
    fn word(&self, n: usize) -> Oop {
        let q = self.host.scope();
        self.keep(q.word(n).unwrap())
    }
    fn word_bits(&self, o: Oop) -> usize {
        self.host.scope().word_bits(o).unwrap()
    }
    fn bytes(&self, o: Oop) -> Vec<u8> {
        self.host
            .scope()
            .read_bytes(self.call(o, b"bytes", &[]))
            .unwrap()
    }
    fn int(&self, n: i64) -> Oop {
        self.call(
            self.named(b"Integer"),
            b"fromWord:",
            &[self.word(n as usize)],
        )
    }
    fn number(&self, o: Oop) -> usize {
        self.word_bits(self.call(self.named(b"Integer"), b"wordOf:", &[o]))
    }
    fn eval(&self, s: &[u8]) -> Oop {
        let v = self.call(self.named(b"Codegen"), b"eval:", &[self.string(s)]);
        self.host.scope().check_result(v).unwrap();
        v
    }
    fn encoded(&self, b: &[u8]) -> Oop {
        self.call(self.named(b"Bytes"), b"fromBytes:", &[self.raw(b)])
    }
    fn vector(&self, a: &[Oop]) -> Oop {
        let mut v = self.call(self.named(b"Vector"), b"new", &[]);
        for o in a {
            v = self.call(v, b"add:", &[*o]);
        }
        v
    }
    fn materialize(&self, code: Oop, captures: &[Oop], resumable: bool) -> Oop {
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
    fn units(&self) -> usize {
        as_size(self.call(self.named(b"TCC"), b"liveNativeUnits", &[])).unwrap()
    }
    fn collect(&self) {
        self.call(self.gc, b"collectPrecisely", &[]);
        self.call(self.named(b"TCC"), b"checkpoint", &[]);
    }
    fn clear_from(&self, i: usize) {
        let tokens = self.tokens.borrow_mut().split_off(i);
        for t in tokens.into_iter().rev() {
            self.host.send(self.host.roots, b"release:", &[t]).unwrap();
        }
    }
    fn restore_lang(&self) {
        for &(vt, n, m) in &self.lang_bindings {
            self.call(vt, b"addMethod", &[n, m]);
        }
    }
    fn context(&self, alias: Oop, failure: Oop) -> Oop {
        self.call(
            self.host.id.root,
            b"sequence",
            &[
                alias,
                self.gc,
                size(0).unwrap(),
                self.named(b"Codegen"),
                failure,
                self.host.roots,
            ],
        )
    }
    fn calls(&self, c: Oop) -> usize {
        as_size(self.call(c, b"at:", &[size(2).unwrap()])).unwrap()
    }
    fn method(&self, n: &[u8], c: Oop) -> Oop {
        self.call(
            self.call(self.fixture, b"methodAt:", &[self.key(n)]),
            b"withContext:",
            &[c],
        )
    }
    fn fault(&self, status: usize, detail: usize) -> Oop {
        self.keep(
            self.native(
                self.named(b"Execution"),
                b"failNative:detail:",
                &[size(status).unwrap(), size(detail).unwrap()],
            )
            .unwrap_err(),
        )
    }
    fn load(&self, name: &str) {
        let path = self
            .host
            .resources
            .lib_dir
            .join(format!("libbirnam_{name}.dylib"));
        let v = self.call(
            self.named(b"Codegen"),
            b"load:",
            &[self.string(path.as_os_str().as_encoded_bytes())],
        );
        self.host.scope().check_result(v).unwrap();
        assert_ne!(v, self.named(b"nil"), "load {name} returned nil");
    }
    fn clear_source_bindings(&self) {
        self.load("globalsOwner");
        let globals = self.named(b"Globals");
        let names = self.call(globals, b"bindingNames", &[]);
        let count = self.number(self.call(names, b"size", &[]));
        for i in 0..count {
            let name = self.call(names, b"at:", &[self.int(i as i64)]);
            assert_eq!(self.call(globals, b"remove:", &[name]), globals);
        }
    }
}
impl Drop for H {
    fn drop(&mut self) {
        self.clear_from(0);
        for t in self.floor_tokens.take().into_iter().rev() {
            self.host.send(self.host.roots, b"release:", &[t]).unwrap();
        }
    }
}
fn binding(h: &H, vt: Oop, n: &[u8]) -> Oop {
    {
        let method = h.call(
            h.named(b"Execution"),
            b"directMethodFor:in:",
            &[h.key(n), vt],
        );
        assert!(
            method != h.named(b"nil"),
            "missing binding {}",
            String::from_utf8_lossy(n)
        );
        method
    }
}
fn set(h: &H, vt: Oop, n: &[u8], method: Oop) {
    h.call(vt, b"addMethod", &[h.key(n), method]);
}
fn behavior_of(h: &H, o: Oop) -> Oop {
    h.call(h.host.id.root, b"behaviorOf:", &[o])
}
fn global(h: &H, n: &[u8]) -> Oop {
    h.call(h.named(b"Globals"), b"at:", &[h.key(n)])
}
fn harness() -> H {
    let host = oop::bootstrap();
    let gc = host.named(b"GC").unwrap();
    let mut h = H {
        host,
        gc,
        fixture: std::ptr::null_mut(),
        tokens: RefCell::new(vec![]),
        floor_tokens: RefCell::new(vec![]),
        lang_bindings: vec![],
    };
    h.call(gc, b"precisePayloads:", &[size(1).unwrap()]);
    let lang = h.named(b"Lang");
    let vt = h.floor_keep(behavior_of(&h, lang));
    for name in [
        b"buildGrammar".as_slice(),
        b"initializeGrammar",
        b"grammar",
        b"parse:",
        b"furthestFail",
    ] {
        let selector = h.floor_keep(h.key(name));
        let method = h.floor_keep(h.call(h.host.id.root, b"bind:to:", &[selector, lang]));
        h.lang_bindings.push((vt, selector, method));
    }
    // Only Lang methods survive explicit world cleanup through fixture roots.
    // Keep the real bootstrap Module registry, including optional providers.
    let module = global(&h, b"Module");
    h.call(
        module,
        b"libDir:",
        &[h.string(h.host.resources.lib_dir.as_os_str().as_encoded_bytes())],
    );
    h.load("store");
    h.call(module, b"markLoaded:", &[h.key(b"store")]);
    assert_eq!(
        h.call(
            module,
            b"field:of:",
            &[h.key(b"imageProvider"), h.key(b"store")]
        ),
        h.named(b"true")
    );

    let path = std::env::temp_dir().join(format!(
        "birnam-image-roundtrip-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/image-roundtrip.c")
            )
            .arg("-o")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    h.fixture = h.floor_keep(h.host.load_native(&h.host.scope(), &path, &[]).unwrap());
    std::fs::remove_file(path).unwrap();
    h.clear_from(0);
    h.collect();
    h
}
const IMAGE_WORD_BITS: [usize; 4] = [0, i64::MAX as usize, 1usize << 63, usize::MAX];
const IMAGE_FLOAT_BITS: [usize; 4] = [
    0x8000000000000000,
    0x7ff8000000000042,
    0x7ff0000000000000,
    0x7fefffffffffffff,
];
fn numeric_image_values(h: &H, saving: bool) {
    {
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
                    values.push(h.keep(h.call(h.named(family), b"fromBits:", &[h.word(value)])));
                }
                values.push(values[3]);
                h.call(
                    h.named(b"Globals"),
                    b"at:put:",
                    &[h.key(name), h.vector(&values)],
                );
            }
            let values = global(h, name);
            for (index, value) in bits.into_iter().enumerate() {
                let restored = h.call(values, b"at:", &[h.int(index as i64)]);
                assert_eq!(h.word_bits(h.call(restored, b"bits", &[])), value);
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
        assert_eq!(h.call(error, b"kind", &[]), h.key(b"overflow"));
        assert_eq!(
            h.bytes(h.call(error, b"message", &[])),
            b"word out of Integer range"
        );
        if saving {
            h.call(
                h.named(b"Globals"),
                b"at:put:",
                &[h.key(b"ImageNumericError"), error],
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
    if let Ok(value) = std::env::var("BIRNAM_IMAGE_RESIDENT_EXPECTED") {
        return value.parse().unwrap();
    }
    let path = std::env::temp_dir().join(format!(
        "birnam-resident-control-{}.txt",
        std::process::id()
    ));
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--ignored", "--nocapture"])
        .env_remove("BIRNAM_IMAGE_TEST_MODE")
        .env_remove("BIRNAM_IMAGE_CHAIN_EPOCH")
        .env("BIRNAM_IMAGE_RESIDENT_CONTROL", &path)
        .env("BIRNAM_IMAGE_RESIDENT_MODE", mode)
        .status()
        .unwrap();
    assert!(status.success(), "resident cleanup control failed");
    let count = std::fs::read_to_string(&path)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    count
}
fn resident_control() -> bool {
    let Some(path) = std::env::var_os("BIRNAM_IMAGE_RESIDENT_CONTROL") else {
        return false;
    };
    {
        let h = harness();
        let mode = std::env::var("BIRNAM_IMAGE_RESIDENT_MODE").unwrap();
        if mode != "source" {
            let image = std::path::PathBuf::from(&path).with_extension("image");
            let filename = h.string(image.as_os_str().as_bytes());
            assert_eq!(
                h.call(h.named(b"System"), b"saveImage:", &[filename]),
                h.named(b"true")
            );
            h.call(
                h.named(b"System"),
                b"loadImage:rebuild:",
                &[
                    filename,
                    h.named(if mode == "rebuild" { b"true" } else { b"false" }),
                ],
            );
            std::fs::remove_file(image).unwrap();
        }
        h.clear_source_bindings();
        h.call(h.named(b"Store"), b"imageLoadBegin", &[]);
        h.call(h.named(b"Codegen"), b"evictMaterializations", &[]);
        h.restore_lang();
        h.clear_from(0);
        h.collect();
        std::fs::write(path, format!("{}\n", h.units())).unwrap();
    }
    true
}
#[test]
#[ignore = "requires production native modules"]
fn image_owner_roundtrip() {
    if resident_control() {
        return;
    }
    {
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
            let slot_alias = h.key(b"imageTestRestoreSlot:in:put:");
            set(&h, model_vt, b"imageTestRestoreSlot:in:put:", slot_entry);
            let collecting = h.context(slot_alias, std::ptr::null_mut());
            set(
                &h,
                model_vt,
                b"restoreImageSlotAt:in:put:",
                h.method(b"restore", collecting),
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
                    h.method(b"deny", h.fault(982, 17)),
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
                        h.method(b"deny", h.fault(982, 17)),
                    );
                }
                let vt = behavior_of(&h, h.named(b"TCC"));
                for selector in [b"compile:".as_slice(), b"compileObject:"] {
                    set(&h, vt, selector, h.method(b"deny", h.fault(982, 17)));
                }
            }
            let result = h.call(
                system,
                b"loadImage:rebuild:",
                &[filename, h.named(if mapped { b"false" } else { b"true" })],
            );
            assert_eq!(result, h.named(if mapped { b"true" } else { b"false" }));
            assert_eq!(
                h.call(
                    h.named(b"Execution"),
                    b"directMethodFor:in:",
                    &[
                        h.key(b"imageTransient"),
                        behavior_of(&h, h.named(b"Integer"))
                    ]
                ),
                h.named(b"nil")
            );
            assert_eq!(
                h.call(
                    h.call(h.named(b"Vector"), b"new", &[]),
                    b"imageInherited",
                    &[]
                ),
                h.key(b"inherited")
            );
            assert_eq!(
                h.call(
                    h.named(b"Execution"),
                    b"directMethodFor:in:",
                    &[h.key(b"imageTransientCollection"), h.named(b"Collection")]
                ),
                h.named(b"nil")
            );
            assert!(h.calls(collecting) > 0);
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
            let fixture = h.call(codegen, b"moduleSourceFor:", &[h.key(b"fixture")]);
            assert_eq!(h.bytes(fixture), b"[Module named: #fixture requires: #[]]");
            assert_eq!(
                h.call(codegen, b"moduleSourceFor:", &[h.key(b"later")]),
                nil
            );
            numeric_image_values(&h, false);
            let wide = global(&h, b"ImageWideVector");
            assert_eq!(h.number(h.call(wide, b"size", &[])), 130);
            for index in 0..130 {
                assert_eq!(
                    h.number(h.call(wide, b"at:", &[h.int(index)])),
                    index as usize
                );
            }

            let range = global(&h, b"ImageRange");
            assert_eq!(range, global(&h, b"ImageRangeAlias"));
            assert_eq!(h.number(h.call(range, b"size", &[])), 5);
            assert_eq!(h.number(h.call(range, b"at:", &[h.int(2)])), 0);
            let reversed = h.call(range, b"reversed", &[]);
            for index in 0..5 {
                assert_eq!(
                    h.number(h.call(reversed, b"at:", &[h.int(index)])) as i64,
                    2 - index
                );
            }
            let typed = global(&h, b"ImageTypedMap");
            assert_eq!(typed, global(&h, b"ImageTypedMapAlias"));
            assert_eq!(h.number(h.call(typed, b"size", &[])), 5);
            let negative_zero =
                h.keep(h.call(h.named(b"Float"), b"fromBits:", &[h.word(1usize << 63)]));
            for (key, expected) in [
                (h.int(0), b"integer".as_slice()),
                (negative_zero, b"float"),
                (h.key(b"alpha"), b"symbol"),
                (h.string(b"alpha"), b"string"),
                (h.string("café".as_bytes()), b"utf8"),
            ] {
                assert_eq!(h.bytes(h.call(typed, b"at:", &[key])), expected);
            }
            let updated = h.call(
                typed,
                b"at:put:",
                &[h.string(b"alpha"), h.string(b"changed")],
            );
            assert_eq!(
                h.bytes(h.call(updated, b"at:", &[h.string(b"alpha")])),
                b"changed"
            );
            assert_eq!(
                h.bytes(h.call(typed, b"at:", &[h.string(b"alpha")])),
                b"string"
            );

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
                h.call(h.host.id.root, b"parentOf:", &[behavior_of(&h, child)]),
                behavior_of(&h, target)
            );
            assert_eq!(h.number(h.call(grand, b"answer:", &[h.int(40)])), 42);
            assert_eq!(h.number(h.call(grand, b"extra", &[])), 9);
            let cyclic = global(&h, b"ImageCycle");
            assert_eq!(
                h.call(h.named(b"ObjectModel"), b"slotAt:in:", &[h.word(0), cyclic]),
                cyclic
            );
            assert_eq!(
                h.call(h.named(b"ObjectModel"), b"slotAt:in:", &[h.word(1), cyclic]),
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
                    assert_eq!(
                        h.call(map, b"at:", &[key]),
                        closure,
                        "private/shared Map identity key {index} must be rehashed"
                    );
                }
            }
            assert_eq!(
                h.number(h.call(global(&h, b"ImageMapChild"), b"imageMapChildMarker", &[])),
                91
            );
            let cell = global(&h, b"ImageCell");
            assert_eq!(h.number(h.call(cell, b"value", &[])), 2);
            assert_eq!(h.number(h.call(cell, b"imageCellMarker", &[])), 73);
            assert_eq!(
                h.call(
                    h.named(b"Execution"),
                    b"directMethodFor:in:",
                    &[h.key(b"imageCellMarker"), h.named(b"StoreVt"),]
                ),
                h.named(b"nil"),
                "Cell methods must not be restored on their native Store owner"
            );
            let cell_block = global(&h, b"ImageCellBlock");
            assert_eq!(h.call(cell_block, b"captureAt:", &[h.int(0)]), cell);
            assert_eq!(h.number(h.call(cell_block, b"value", &[])), 2);
            let store = h.named(b"Store");
            let entity = h.key(b"imageHistory");
            let attr = h.key(b"tracked");
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
                &[h.key(b"alternateAnswer:")],
            );
            h.call(
                h.named(b"Store"),
                b"addMethod:on:with:",
                &[h.key(b"answer:"), behavior_of(&h, target), replacement],
            );
            h.collect();
            assert_eq!(h.number(h.call(grand, b"answer:", &[h.int(40)])), 43);
            assert_eq!(h.number(h.call(sibling, b"answer:", &[h.int(40)])), 43);
            h.call(
                behavior_of(&h, h.named(b"Integer")),
                b"removeMethod:",
                &[h.key(b"imageFixture")],
            );
            h.call(
                h.named(b"Collection"),
                b"removeMethod:",
                &[h.key(b"imageInherited")],
            );
            h.call(
                h.named(b"CellVt"),
                b"removeMethod:",
                &[h.key(b"imageCellMarker")],
            );
            h.clear_source_bindings();
            h.call(h.named(b"Store"), b"imageLoadBegin", &[]);
            h.restore_lang();
            h.clear_from(0);
            h.collect();
            assert_eq!(
                h.units(),
                resident_units(
                    "image_owner_roundtrip",
                    if mode == "rebuild" || mode == "stripped" {
                        "rebuild"
                    } else {
                        "mapped"
                    }
                ),
                "restored fixture mappings reclaim against equivalent resident cleanup"
            );
            return;
        }
        numeric_image_values(&h, true);
        let mut wide = h.call(h.named(b"Vector"), b"new", &[]);
        for index in 0..130 {
            wide = h.call(wide, b"add:", &[h.int(index)]);
        }
        h.call(
            h.named(b"Globals"),
            b"at:put:",
            &[h.key(b"ImageWideVector"), wide],
        );

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
            h.call(h.named(b"Globals"), b"at:put:", &[h.key(name), value]);
        }
        let vt = h.call(h.named(b"ObjectVt"), b"delegated", &[]);
        h.keep(vt);
        h.call(
            h.named(b"ObjectModel"),
            b"setLayoutOf:to:rest:",
            &[vt, h.raw(&[1, 1]), h.word(0)],
        );
        let cycle = h.keep(h.call(
            h.named(b"ObjectModel"),
            b"allocate:bytes:",
            &[vt, h.word(2 * size_of::<usize>())],
        ));
        h.call(h.host.id.root, b"ownerOf:put:", &[vt, cycle]);
        for (i, v) in [(0, cycle), (1, closure)] {
            h.call(
                h.named(b"ObjectModel"),
                b"slotAt:in:put:",
                &[h.word(i), cycle, v],
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
        for (name, value) in [
            (b"ImageCycle".as_slice(), cycle),
            (b"ImageMap", map),
            (b"ImageMapKeys", keys),
        ] {
            h.call(h.named(b"Globals"), b"at:put:", &[h.key(name), value]);
        }
        h.eval(b"[Reflect defineMethod: #imageMapMarker on: ImageMap as: [:self | 73]]");
        let map_behavior = behavior_of(&h, map);
        for name in [b"ImageMapAlias".as_slice(), b"ImageMapChild"] {
            let copy = h.call(map, b"at:put:", &[h.key(name), closure]);
            h.call(
                h.host.id.root,
                b"reassociateBehaviorOf:with:",
                &[copy, map_behavior],
            );
            h.call(h.named(b"Globals"), b"at:put:", &[h.key(name), copy]);
            assert_eq!(h.number(h.call(copy, b"imageMapMarker", &[])), 73);
        }
        h.eval(b"[Reflect defineMethod: #imageMapChildMarker on: ImageMapChild as: [:self | 91]]");
        assert_eq!(
            h.number(h.call(global(&h, b"ImageMapChild"), b"imageMapChildMarker", &[])),
            91
        );

        h.eval(b"[db assert: #tracked attr: #history value: true]; [db assert: #imageHistory attr: #tracked value: 1]; [db assert: #imageHistory attr: #tracked value: 2]; [Reflect defineMethod: #imageFixture on: Integer as: [:self | #saved]]");
        h.eval(b"let ImageCell := [db cellFor: #imageHistory attr: #tracked]; let ImageCellBlock := [[:cell | [| [cell value]]] value: ImageCell]");
        h.eval(b"let ImageSystem := [System clone]; let ImageSystemAlias := ImageSystem; [Reflect defineMethod: #imageSystemMarker on: ImageSystem as: [:self | 83]]");
        let codegen = h.named(b"Codegen");
        let source = h.string(b"[Module named: #fixture requires: #[]]");
        assert_eq!(
            h.call(
                codegen,
                b"recordModuleSource:named:",
                &[source, h.key(b"fixture")]
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
        let failed_map = h.context(h.key(b"imageTestLoadObject:"), h.fault(887, 42));
        set(&h, tcc_vt, b"loadObject:", h.method(b"map", failed_map));
        let error = h.native(system, b"loadImage:", &[filename]).unwrap_err();
        assert_eq!(error, h.call(failed_map, b"at:", &[size(4).unwrap()]));
        assert_eq!(h.calls(failed_map), 2);
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
        // A failure after a real dictionary write and precise collection leaves the
        // applied image state in place. No automatic inverse is attempted.
        let later = h.string(b"[Module named: #later requires: #[]]");
        assert_eq!(
            h.call(
                codegen,
                b"recordModuleSource:named:",
                &[later, h.key(b"later")]
            ),
            later
        );
        h.eval(b"[Reflect defineMethod: #imageFixture on: Integer as: [:self | #before]]; [Reflect defineMethod: #imageTransient on: Integer as: [:self | #transient]]; [Reflect defineMethod: #imageInherited on: Collection as: [:self | #beforeInherited]]");
        let codegen = h.named(b"Codegen");
        let codegen_vt = behavior_of(&h, codegen);
        let selector = b"installPortableImageBinding:on:as:source:";
        let original = binding(&h, codegen_vt, selector);
        let alias = h.key(b"imageTestOriginalInstall:on:as:source:");
        set(
            &h,
            codegen_vt,
            b"imageTestOriginalInstall:on:as:source:",
            original,
        );
        let failure = h.context(alias, h.fault(881, 73));
        set(&h, codegen_vt, selector, h.method(b"publish", failure));
        let result = h.native(system, b"loadImage:", &[filename]).unwrap_err();
        assert_eq!(result, h.call(failure, b"at:", &[size(4).unwrap()]));
        assert_eq!(h.calls(failure), 1);
        set(&h, codegen_vt, selector, original);
        let report = h.call(system, b"imageSaveReport", &[]);
        assert_eq!(h.call(report, b"status", &[]), h.key(b"partialLoad"));
        assert_eq!(h.call(system, b"imageCompositionIdentity", &[]), nil);
        // The image removes this later binding before installing its methods.
        // An inverse would wrongly restore it under the selected contract.
        assert_eq!(
            h.call(
                h.named(b"Execution"),
                b"directMethodFor:in:",
                &[
                    h.key(b"imageTransient"),
                    behavior_of(&h, h.named(b"Integer")),
                ]
            ),
            h.named(b"nil")
        );
        // Source ownership follows the Globals state actually reached before
        // the failure. There is no earlier native registry exchange or inverse.
        let owner = global(&h, b"SourceRegistry");
        let record = h.call(owner, b"recordFor:", &[h.key(b"later")]);
        let visible = if record == nil {
            nil
        } else {
            h.call(codegen, b"sourceTextOf:", &[record])
        };
        assert_eq!(
            h.call(codegen, b"moduleSourceFor:", &[h.key(b"later")]),
            visible,
            "compiler lookup must use the visible ordinary owner after partial application"
        );
        // Continue independent save-admission checks from an explicitly restored
        // snapshot, rather than assuming a failed load restored the session.
        assert_eq!(h.call(system, b"loadImage:", &[filename]), h.named(b"true"));
        let original_bytes = std::fs::read(&path).unwrap();
        // An ordinary source capability must not silently lose its transient
        // state. Admission follows its current owner method, not Store metadata.
        let ephemeral_roots = h.tokens.borrow().len();
        h.eval(b"[[| let ephemeral := [Object clone]; [Reflect defineMethod: #imagePayload on: ephemeral as: [:self | false]]; [Globals at: #ImageEphemeral put: #[ephemeral]]] value]");
        assert_eq!(h.call(system, b"saveImage:", &[filename]), nil);
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        h.eval(b"[Globals at: #ImageEphemeral put: nil]");
        // The fixture's eval result also retains the vector/receiver. Its live
        // authored method is a real image root until those test roots are gone.
        h.clear_from(ephemeral_roots);
        h.collect();
        assert_eq!(h.call(system, b"saveImage:", &[filename]), h.named(b"true"));
        let original_bytes = std::fs::read(&path).unwrap();
        let machine = h.call(h.named(b"Machine"), b"new", &[]);
        let native_method = h.call(h.named(b"Integer"), b"methodFor:", &[h.key(b"+")]);
        let block_method = h.call(closure, b"methodFor:", &[h.key(b"value")]);
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
                    &[h.key(b"ImageUnsupported"), capability],
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
            h.call(codegen, b"moduleSourceFor:", &[h.key(b"later")]),
            nil
        );
        // Source registration remains available after explicit image reload.
        assert_eq!(
            h.call(
                codegen,
                b"recordModuleSource:named:",
                &[later, h.key(b"later")]
            ),
            later
        );
        assert_eq!(h.call(h.int(1), b"imageFixture", &[]), h.key(b"saved"));
        // Structural corruption must be rejected before touching current Globals.
        let current_shared = global(&h, b"ImageShared");
        let mut damaged = std::fs::read(&path).unwrap();
        damaged[28] ^= 1;
        let encoded = h.encoded(&damaged);
        assert_eq!(
            h.call(system, b"loadImageBytes:rebuild:", &[encoded, nil]),
            nil
        );
        assert_eq!(global(&h, b"ImageShared"), current_shared);
        std::fs::remove_file(path).unwrap();
    }
}
// A collecting Vector constructor must not invalidate the enumerated vtables.
fn image_bindings_root_behavior_snapshot_before_allocation(h: &H) {
    h.collect();
    let vt = behavior_of(h, h.named(b"Vector"));
    let original = binding(h, vt, b"new");
    let alias = h.key(b"imageTestVectorNew");
    set(h, vt, b"imageTestVectorNew", original);
    let hook = h.context(alias, std::ptr::null_mut());
    for _ in 0..8 {
        let o = h.host.send(h.named(b"Object"), b"clone", &[]).unwrap();
        h.host
            .send(h.host.id.root, b"ensurePrivateVtableFor:", &[o])
            .unwrap();
    }
    set(h, vt, b"new", h.method(b"before", hook));
    let result = h.call(h.named(b"Codegen"), b"portableImageBindings", &[]);
    set(h, vt, b"new", original);
    h.call(vt, b"removeMethod:", &[alias]);
    assert!(h.calls(hook) > 0);
    assert!(h.number(h.call(result, b"size", &[])) > 0);
}
#[test]
#[ignore = "requires production native modules"]
fn repeated_image_resaves_keep_a_fixed_live_set() {
    if resident_control() {
        return;
    }
    const EPOCHS: usize = 20;
    if let Ok(epoch) = std::env::var("BIRNAM_IMAGE_CHAIN_EPOCH") {
        let epoch: usize = epoch.parse().unwrap();
        let path = std::path::PathBuf::from(std::env::var_os("BIRNAM_IMAGE_CHAIN_PATH").unwrap());
        {
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
                h.call(globals, b"at:put:", &[h.key(b"ChainValue"), next]);
            }
            // Every epoch changes the same historical fact, then drops old
            // history. Both the restored closure and latest value stay live.
            let entity = h.keep(h.key(b"chainEntity"));
            let attribute = h.keep(h.key(b"chainValue"));
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
            h.clear_source_bindings();
            h.call(store, b"imageLoadBegin", &[]);
            h.call(codegen, b"evictMaterializations", &[]);
            h.restore_lang();
            h.clear_from(0);
            h.collect();
            assert_eq!(
                h.units(),
                resident_units(
                    "repeated_image_resaves_keep_a_fixed_live_set",
                    if epoch == 0 {
                        "source"
                    } else if epoch % 2 == 0 {
                        "rebuild"
                    } else {
                        "mapped"
                    }
                ),
                "epoch {epoch} retained fixture mappings"
            );
        }
        return;
    }
    let directory = std::env::temp_dir().join(format!("birnam-image-chain-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("current.image");
    let mut baselines: [Option<Vec<usize>>; 2] = [None, None];
    let resident = [
        resident_units("repeated_image_resaves_keep_a_fixed_live_set", "source"),
        resident_units("repeated_image_resaves_keep_a_fixed_live_set", "mapped"),
        resident_units("repeated_image_resaves_keep_a_fixed_live_set", "rebuild"),
    ];
    for epoch in 0..=EPOCHS {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "repeated_image_resaves_keep_a_fixed_live_set",
                "--ignored",
                "--nocapture",
            ])
            .env(
                "BIRNAM_IMAGE_RESIDENT_EXPECTED",
                resident[if epoch == 0 {
                    0
                } else if epoch % 2 == 1 {
                    1
                } else {
                    2
                }]
                .to_string(),
            )
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
