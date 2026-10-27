//! Binary values and embedded files through current Oop message owners.
#[path = "support/oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use birnam_host_support::{Context, Scope};
use std::{os::unix::ffi::OsStrExt, path::Path, ptr};
struct H<'a> {
    host: &'a Context,
    q: Scope<'a>,
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
    fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        self.q.call(r, n, a).unwrap()
    }
    fn string(&self, b: &[u8]) -> Oop {
        self.q.string(b).unwrap()
    }
    fn binary(&self, b: &[u8]) -> Oop {
        self.call(
            self.named(b"Bytes"),
            b"fromBytes:",
            &[self.q.byte_values(b).unwrap()],
        )
    }
    fn bytes(&self, v: Oop) -> Vec<u8> {
        let q = self.host.scope();
        q.read_bytes(q.call(v, b"bytes", &[]).unwrap()).unwrap()
    }
    fn eval(&self, s: &[u8]) -> Oop {
        self.call(self.named(b"Codegen"), b"eval:", &[self.string(s)])
    }
    fn number(&self, v: Oop) -> usize {
        as_size(self.call(self.named(b"Integer"), b"wordOf:", &[v])).unwrap()
    }
    fn at(&self, v: Oop, i: usize) -> Oop {
        self.call(
            v,
            b"at:",
            &[self.call(
                self.named(b"Integer"),
                b"fromWord:",
                &[self.q.word(i).unwrap()],
            )],
        )
    }
    fn vt(&self, v: Oop) -> Oop {
        self.call(self.host.id.root, b"behaviorOf:", &[v])
    }
    fn binding(&self, vt: Oop, n: &[u8]) -> Oop {
        self.call(vt, b"lookup", &[self.key(n)])
    }
    fn set(&self, vt: Oop, n: &[u8], m: Oop) {
        self.call(vt, b"addMethod", &[self.key(n), m]);
    }
    fn method(&self, n: &[u8], c: Oop) -> Oop {
        self.call(
            self.call(self.fixture, b"methodAt:", &[self.key(n)]),
            b"withContext:",
            &[c],
        )
    }
    fn collect(&self) {
        self.call(self.named(b"GC"), b"collectPrecisely", &[]);
        self.call(self.named(b"TCC"), b"checkpoint", &[]);
    }
    fn units(&self) -> usize {
        as_size(self.call(self.named(b"TCC"), b"liveNativeUnits", &[])).unwrap()
    }
    fn identity(&self, v: Oop) -> Oop {
        self.call(self.host.id.root, b"identityOf:", &[v])
    }
    fn present(&self, identity: Oop) -> bool {
        let state = self.hooks.context(&[identity, size(0).unwrap()]);
        let probe = self.call(self.named(b"Object"), b"clone", &[]);
        let vt = self.call(self.host.id.root, b"ensurePrivateVtableFor:", &[probe]);
        self.set(
            vt,
            b"allocation:bytes:identity:",
            self.method(b"allocation:bytes:identity:", state),
        );
        self.call(self.host.id.root, b"allocationsDo:", &[probe]);
        self.hooks.count(state, 1) == 1
    }
}
#[test]
#[ignore = "requires production Oop modules"]
fn bytes_and_embedding_use_live_messages_and_owned_payloads() {
    let host = oop::bootstrap();
    let q = host.scope();
    let library = std::env::temp_dir().join(format!(
        "birnam-embedding-hooks-{}.dylib",
        std::process::id()
    ));
    assert!(
        std::process::Command::new("clang")
            .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/oop-host/embedding.c")
            )
            .arg("-o")
            .arg(&library)
            .status()
            .unwrap()
            .success()
    );
    let fixture = host.load_native(&q, &library, &[]).unwrap();
    std::fs::remove_file(library).unwrap();
    let h = H {
        host: &host,
        q,
        hooks: oop::hooks::Hooks::new(&host),
        fixture,
    };
    h.call(h.named(b"GC"), b"precisePayloads:", &[size(1).unwrap()]);
    let proto = h.named(b"Bytes");
    let vt = h.named(b"BytesVt");
    let nil = h.named(b"nil");
    assert_eq!(h.number(h.call(proto, b"size", &[])), 0);
    let bytes = h.eval(b"[\"a\0\xff\" asBytes]");
    let payload: Vec<u8> = (0..=255).collect();
    let value = h.binary(&payload);
    assert_eq!(h.bytes(value), payload);
    assert_eq!(h.vt(value), vt);
    assert_eq!(h.number(h.call(value, b"size", &[])), 256);
    assert_eq!(h.number(h.at(value, 255)), 255);
    assert_eq!(h.at(value, 256), nil);
    assert_eq!(h.at(value, usize::MAX), nil);
    assert_eq!(h.call(value, b"at:", &[nil]), nil);
    let copy = h.call(value, b"clone", &[]);
    assert_ne!(copy, value);
    assert_eq!(h.bytes(copy), payload);
    let text = h.call(value, b"asString", &[]);
    assert_eq!(h.bytes(text), payload);
    assert_eq!(h.bytes(h.call(text, b"asBytes", &[])), payload);
    assert_ne!(h.call(bytes, b"clone", &[]), bytes);
    assert_eq!(h.number(h.eval(b"let B := [Bytes clone]; [Reflect defineMethod: #answer on: B as: [:self | 37]]; [B answer]")),37);
    h.collect();
    assert_eq!(h.bytes(copy), payload);
    let (identity, pointer_bytes, opaque) = {
        let q = host.scope();
        let dead = q
            .call(proto, b"fromBytes:", &[q.byte_values(b"dead").unwrap()])
            .unwrap();
        let pointer_bytes = dead.addr().to_ne_bytes();
        (h.identity(dead), pointer_bytes, h.binary(&pointer_bytes))
    };
    h.collect();
    assert!(
        !h.present(identity),
        "binary bytes must not retain a coincidental object address"
    );
    assert_eq!(h.bytes(opaque), pointer_bytes);
    let temp_name = format!(".embedding-test-{}", std::process::id());
    let temp = std::env::current_dir().unwrap().join(&temp_name);
    std::fs::create_dir(&temp).unwrap();
    let asset = temp.join("asset.bin");
    let source_file = temp.join("module.birnam");
    std::fs::write(&asset, &payload).unwrap();
    std::fs::write(&source_file, b"[| [System embedFile: \"asset.bin\"]]").unwrap();
    let block = h.call(
        h.named(b"System"),
        b"load:",
        &[h.string(source_file.as_os_str().as_bytes())],
    );
    h.q.check_result(block).unwrap();
    std::fs::remove_file(&asset).unwrap();
    std::fs::remove_file(&source_file).unwrap();
    h.collect();
    let embedded = h.call(block, b"value", &[]);
    assert_eq!(h.bytes(embedded), payload);
    assert_eq!(h.vt(embedded), vt);
    assert_eq!(h.bytes(h.call(embedded, b"clone", &[])), payload);
    exercise_replacement(&h, block, &temp);
    // Compile with the system compiler, then load after removing both input files.
    for mode in ["resumable", "synchronous"] {
        for nested in [false, true] {
            std::fs::write(&asset, &payload).unwrap();
            let source = format!("[System embedFile: \"{temp_name}/asset.bin\"]");
            let source = if nested {
                format!("[| {source}]")
            } else {
                source
            };
            let source = h.string(source.as_bytes());
            let emitted = if mode == "resumable" {
                h.call(h.named(b"Codegen"), b"emit:", &[source])
            } else {
                let method = h.call(h.named(b"Codegen"), b"compileSynchronous:", &[source]);
                let code = h.call(method, b"codeObject", &[]);
                let canonical = h.call(h.named(b"CodeObject"), b"canonicalBytesOf:", &[code]);
                h.call(h.named(b"NativeBackend"), b"emitSynchronous:", &[canonical])
            };
            h.q.check_result(emitted).unwrap();
            let c = h.bytes(emitted);
            let c_path = temp.join(format!("{mode}-{nested}.c"));
            let library = temp.join(format!("{mode}-{nested}.dylib"));
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
            let owner = h.call(h.named(b"Object"), b"clone", &[]);
            let owner_vt = h.call(host.id.root, b"ensurePrivateVtableFor:", &[owner]);
            h.set(
                owner_vt,
                b"retain:",
                h.method(b"retain:", h.named(b"Roots")),
            );
            let method = host
                .load_native(&h.q, &library, &[host.namespace, owner])
                .unwrap();
            h.q.check_result(method).unwrap();
            let captures = h.call(h.named(b"Vector"), b"new", &[]);
            let block = h.call(
                h.named(b"Block"),
                b"fromMethod:captures:",
                &[method, captures],
            );
            let machine = h.call(h.named(b"Machine"), b"new", &[]);
            h.collect();
            let mut result = h.call(machine, b"runBlock:", &[block]);
            if nested {
                h.collect();
                result = h.call(result, b"value", &[]);
            }
            assert_eq!(h.bytes(result), payload);
        }
    }
    exercise_errors(&h, &temp);
    payload_outlives_code(&h, &temp);
    std::fs::remove_dir_all(temp).unwrap();
}
fn exercise_replacement(h: &H<'_>, block: Oop, temp: &Path) {
    let gc = h.named(b"GC");
    let string_vt = h.named(b"StringVt");
    let bytes_vt = h.named(b"BytesVt");
    for (vt, name) in [
        (string_vt, b"fromBytes:".as_slice()),
        (string_vt, b"asBytes".as_slice()),
        (bytes_vt, b"fromBytes:"),
    ] {
        let old = h.binding(vt, name);
        let alias = b"embeddingOriginal:";
        h.set(vt, alias, old);
        let state = h
            .hooks
            .context(&[gc, h.key(alias), size(0).unwrap(), ptr::null_mut()]);
        h.set(vt, name, h.method(b"forward", state));
        assert_eq!(
            h.bytes(h.call(block, b"value", &[])),
            (0..=255).collect::<Vec<_>>()
        );
        assert_eq!(
            h.hooks.count(state, 2),
            1,
            "runtime constructors must use fresh sends"
        );
        h.set(vt, name, old);
        h.call(vt, b"removeMethod:", &[h.key(alias)]);
    }
    let value = h.binary(b"replacement\0\xff");
    let state = h.hooks.context(&[
        gc,
        value,
        size(0).unwrap(),
        ptr::null_mut(),
        ptr::null_mut(),
    ]);
    let old = h.binding(bytes_vt, b"bytes");
    let alias = b"embeddingBytes";
    h.set(bytes_vt, alias, old);
    let forward = h
        .hooks
        .context(&[gc, h.key(alias), size(0).unwrap(), value]);
    h.set(bytes_vt, b"bytes", h.method(b"forward", forward));
    let receiver = h.call(h.named(b"Object"), b"clone", &[]);
    let receiver_vt = h.call(h.host.id.root, b"ensurePrivateVtableFor:", &[receiver]);
    h.set(receiver_vt, b"embedFile:", h.method(b"embedded", state));
    let lexical = h.eval(b"[:System | [System embedFile: \"lexical.bin\"]]");
    assert_eq!(h.hooks.count(state, 2), 0);
    assert_eq!(h.call(lexical, b"value:", &[receiver]), value);
    let lexical = h.eval(b"[:x | let System := x; [System embedFile: \"local.bin\"]]");
    assert_eq!(h.hooks.count(state, 2), 1);
    assert_eq!(h.call(lexical, b"value:", &[receiver]), value);
    h.hooks.put(state, 2, size(0).unwrap());
    h.hooks.put(forward, 2, size(0).unwrap());
    let globals = h.named(b"Globals");
    let key = h.key(b"System");
    let previous = h.call(globals, b"at:", &[key]);
    h.call(globals, b"at:put:", &[key, receiver]);
    let lexical = h.eval(b"let System := System; [| [System embedFile: \"top-level.bin\"]]");
    assert_eq!(
        h.hooks.count(state, 2),
        0,
        "top-level lexical System must not embed"
    );
    assert_eq!(h.call(lexical, b"value", &[]), value);
    assert_eq!(h.hooks.count(state, 2), 1);
    h.hooks.put(state, 2, size(0).unwrap());
    let path = h.string(temp.join("module.birnam").as_os_str().as_bytes());
    let b = h.call(
        h.named(b"Codegen"),
        b"evalPersisting:from:",
        &[
            h.string(b"[| [System embedFile: \"never-created.bin\"]]"),
            path,
        ],
    );
    assert_eq!(h.hooks.count(state, 2), 1);
    assert_eq!(
        h.hooks.count(forward, 2),
        1,
        "compiler reads the embedded Bytes once through a send"
    );
    assert_eq!(
        h.bytes(h.hooks.field(state, 3)),
        temp.join("never-created.bin").as_os_str().as_bytes()
    );
    assert_eq!(h.bytes(h.call(b, b"value", &[])), b"replacement\0\xff");
    assert_eq!(
        h.hooks.count(state, 2),
        1,
        "embedding runs during compilation only"
    );
    h.set(bytes_vt, b"bytes", old);
    h.call(bytes_vt, b"removeMethod:", &[h.key(alias)]);
    let fault = h.hooks.fault(73);
    h.hooks.put(state, 4, fault);
    let source = h.string(b"[System embedFile: \"unused\"]");
    assert_eq!(
        unsafe {
            h.host
                .id
                .send(h.named(b"Codegen"), h.key(b"eval:"), &[source])
        }
        .unwrap_err(),
        fault
    );
    h.call(globals, b"at:put:", &[key, previous]);
}
fn exercise_errors(h: &H<'_>, temp: &Path) {
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
        String::from_utf8_lossy(&h.bytes(h.call(codegen, b"lastError", &[]))).contains("16 MiB")
    );
}
fn payload_outlives_code(h: &H<'_>, temp: &Path) {
    h.collect();
    let before = h.units();
    let anchor = h.host.scope();
    let value = {
        let q = h.host.scope();
        std::fs::write(temp.join("owned.bin"), b"owned\0\xff").unwrap();
        anchor
            .keep(
                q.call(
                    h.named(b"Codegen"),
                    b"evalPersisting:from:",
                    &[
                        q.string(b"[System embedFile: \"owned.bin\"]").unwrap(),
                        q.string(temp.join("module.birnam").as_os_str().as_bytes())
                            .unwrap(),
                    ],
                )
                .unwrap(),
            )
            .unwrap()
    };
    let identity = h.identity(value);
    h.collect();
    assert_eq!(h.units(), before);
    assert_eq!(
        h.bytes(value),
        b"owned\0\xff",
        "Bytes must own its payload after code retirement"
    );
    drop(anchor);
    h.collect();
    assert!(!h.present(identity));
}
