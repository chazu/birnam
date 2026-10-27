//! Generation acceptance through public Oop owners and declared callbacks.
#[path = "oop.rs"]
mod oop;
use birnam_abi::{Oop, as_size, size};
use birnam_host_support::{Context, Scope};
use std::{cell::RefCell, path::Path};
pub fn bootstrap() -> Context {
    oop::bootstrap()
}
pub struct H<'a> {
    pub host: &'a Context,
    pub q: Scope<'a>,
    pub fixture: Oop,
    pub gc: Oop,
    retain: Oop,
    release: Oop,
    tokens: RefCell<Vec<Oop>>,
}
impl<'a> H<'a> {
    pub fn new(host: &'a Context) -> Self {
        Self::with_fixture(host, "generation")
    }
    pub fn with_fixture(host: &'a Context, fixture_name: &str) -> Self {
        let q = host.scope();
        let gc = host.named(b"GC").unwrap();
        q.call(gc, b"precisePayloads:", &[size(1).unwrap()])
            .unwrap();
        let path = std::env::temp_dir().join(format!(
            "birnam-{fixture_name}-{}.dylib",
            std::process::id()
        ));
        assert!(
            std::process::Command::new("clang")
                .args(["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror"])
                .arg(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join(format!("../../tests/fixtures/oop-host/{fixture_name}.c"))
                )
                .arg("-o")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let fixture = host.load_native(&q, &path, &[]).unwrap();
        std::fs::remove_file(path).unwrap();
        let retain = q.keep(host.intern(b"retain:").unwrap()).unwrap();
        let release = q.keep(host.intern(b"release:").unwrap()).unwrap();
        Self {
            host,
            q,
            fixture,
            gc,
            retain,
            release,
            tokens: RefCell::new(vec![]),
        }
    }
    pub fn named(&self, n: &[u8]) -> Oop {
        self.host.named(n).unwrap()
    }
    pub fn key(&self, n: &[u8]) -> Oop {
        self.keep(self.host.intern(n).unwrap())
    }
    pub fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        self.keep(self.host.send(r, n, a).unwrap())
    }
    pub fn text(&self, b: &[u8]) -> Oop {
        let q = self.host.scope();
        self.keep(q.string(b).unwrap())
    }
    pub fn bytes(&self, v: Oop) -> Vec<u8> {
        self.q.read_bytes(self.call(v, b"bytes", &[])).unwrap()
    }
    pub fn integer(&self, n: usize) -> Oop {
        self.call(self.named(b"Integer"), b"fromWord:", &[size(n).unwrap()])
    }
    pub fn number(&self, o: Oop) -> usize {
        self.q
            .word_bits(self.call(self.named(b"Integer"), b"wordOf:", &[o]))
            .unwrap()
    }
    pub fn at(&self, v: Oop, i: usize) -> Oop {
        self.call(v, b"at:", &[self.integer(i)])
    }
    pub fn vector(&self, a: &[Oop]) -> Oop {
        let mut v = self.call(self.named(b"Vector"), b"new", &[]);
        for x in a {
            v = self.call(v, b"add:", &[*x]);
        }
        v
    }
    pub fn context(&self, a: &[Oop]) -> Oop {
        self.call(self.host.id.root, b"sequence", a)
    }
    pub fn put(&self, c: Oop, i: usize, v: Oop) {
        self.call(c, b"at:put:", &[size(i).unwrap(), v]);
    }
    pub fn count(&self, c: Oop, i: usize) -> usize {
        as_size(self.call(c, b"at:", &[size(i).unwrap()])).unwrap()
    }
    pub fn vt(&self, v: Oop) -> Oop {
        self.call(self.host.id.root, b"behaviorOf:", &[v])
    }
    pub fn binding(&self, vt: Oop, n: &[u8]) -> Oop {
        self.call(vt, b"lookup", &[self.key(n)])
    }
    pub fn bind(&self, vt: Oop, n: &[u8], m: Oop) {
        self.call(vt, b"addMethod", &[self.key(n), m]);
    }
    pub fn method(&self, n: &[u8], c: Oop) -> Oop {
        self.call(
            self.call(self.fixture, b"methodAt:", &[self.key(n)]),
            b"withContext:",
            &[c],
        )
    }
    pub fn eval(&self, s: &[u8]) -> Oop {
        let v = self.call(self.named(b"Codegen"), b"evalPersisting:", &[self.text(s)]);
        self.q.check_result(v).unwrap();
        v
    }
    pub fn prototype(&self, n: &[u8]) -> Oop {
        self.call(self.named(b"Globals"), b"at:", &[self.key(n)])
    }
    pub fn record(&self, n: &[u8]) -> Oop {
        self.call(self.prototype(n), b"clone", &[])
    }
    pub fn lifecycle(&self) {
        self.modules(&["store", "stdlib", "candidatePolicy", "compiledUnit"]);
    }
    pub fn modules(&self, modules: &[&str]) {
        let module = self.host.global(&self.q, b"Module").unwrap();
        self.call(
            module,
            b"libDir:",
            &[self.text(self.host.resources.lib_dir.as_os_str().as_encoded_bytes())],
        );
        for name in modules {
            let path = self
                .host
                .resources
                .lib_dir
                .join(format!("libbirnam_{name}.dylib"));
            let value = self.call(
                self.named(b"Codegen"),
                b"load:",
                &[self.text(path.as_os_str().as_encoded_bytes())],
            );
            self.q.check_result(value).unwrap();
            assert_ne!(value, self.named(b"nil"));
            self.call(module, b"markLoaded:", &[self.key(name.as_bytes())]);
        }
    }
    pub fn keep(&self, value: Oop) -> Oop {
        let token = unsafe { self.host.id.send(self.host.roots, self.retain, &[value]) }.unwrap();
        self.tokens.borrow_mut().push(token);
        value
    }
    pub fn clear(&self) {
        for token in self.tokens.take().into_iter().rev() {
            unsafe { self.host.id.send(self.host.roots, self.release, &[token]) }.unwrap();
        }
    }
}
impl Drop for H<'_> {
    fn drop(&mut self) {
        self.clear();
    }
}
