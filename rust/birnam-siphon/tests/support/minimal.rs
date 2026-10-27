//! Test-only native composition without the whole Bootstrap or optional providers.
use birnam_abi::{Oop, size};
use birnam_host_support::{Context, Resources};
use std::{os::unix::ffi::OsStrExt, path::PathBuf};
pub fn native_floor() -> Context {
    let lib = PathBuf::from(std::env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
    let prefix = lib.parent().unwrap().to_path_buf();
    let resources = Resources {
        id_dylib: lib.join("libbirnam_id.dylib"),
        lib_dir: lib.clone(),
        rust_workspace_dir: prefix.join("unavailable-src"),
        guide_dir: prefix.join("unavailable-guide"),
        prefix,
    };
    let roots_path = lib.join("libbirnam_root_registry.dylib");
    let namespace_path = lib.join("libbirnam_namespace.dylib");
    let id = birnam_siphon::load(
        resources.id_dylib.as_os_str().as_bytes(),
        &[
            roots_path.as_os_str().as_bytes(),
            namespace_path.as_os_str().as_bytes(),
            b"sequence",
            b"nameWithBytes",
        ],
    )
    .unwrap();
    let host = unsafe {
        let sequence = id.send(id.literal(2).unwrap(), id.intern, &[]).unwrap();
        let empty = id.send(id.root, sequence, &[]).unwrap();
        let roots = id.send(id.literal(0).unwrap(), id.load, &[empty]).unwrap();
        let arguments = id.send(id.root, sequence, &[roots]).unwrap();
        let namespace = id
            .send(id.literal(1).unwrap(), id.load, &[arguments])
            .unwrap();
        let constructor = id.send(id.literal(3).unwrap(), id.intern, &[]).unwrap();
        Context::from_namespace(id, namespace, resources, constructor).unwrap()
    };
    {
        let q = host.scope();
        let key = |n: &[u8]| q.keep(host.intern(n).unwrap()).unwrap();
        let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
        let publish = |n: &[u8], v| {
            call(host.namespace, b"at:put:", &[key(n), v]);
        };
        let mount = |n: &str, args: &[Oop]| {
            host.load_native(&q, &lib.join(format!("libbirnam_{n}.dylib")), args)
                .unwrap()
        };
        let base = call(id.root, b"objectBehavior", &[]);
        publish(b"ObjectVt", base);
        publish(b"Vtable", id.root);
        for name in ["boolean", "nil"] {
            mount(name, &[host.namespace]);
        }
        let gc = mount("collect", &[host.roots]);
        publish(b"GC", gc);
        call(gc, b"activate", &[]);
        call(gc, b"precisePayloads:", &[size(1).unwrap()]);
        for name in [
            "object_model",
            "word",
            "bytes",
            "execution",
            "string",
            "integer",
            "float",
            "prelude",
            "file_io",
        ] {
            mount(name, &[host.namespace]);
        }
        let collection = call(base, b"delegated", &[]);
        publish(b"Collection", collection);
        publish(b"CollectionVt", collection);
        let symbol = key(b"Symbol");
        publish(b"SymbolVt", call(id.root, b"behaviorOf:", &[symbol]));
        for name in [
            "vector",
            "map",
            "portable_code",
            "artifact",
            "tcc",
            "native_backend",
            "codegen_runtime",
        ] {
            mount(name, &[host.namespace]);
        }
    }
    host
}

/// Add the native services required by the actual authored language initializers.
pub fn service_floor() -> Context {
    let host = native_floor();
    {
        let q = host.scope();
        let call = |r, n: &[u8], a: &[Oop]| q.call(r, n, a).unwrap();
        let named = |n: &[u8]| host.named(n).unwrap();
        for name in [
            "system_output",
            "system_input",
            "environment",
            "storage",
            "peg",
            "generation",
        ] {
            host.load_native(
                &q,
                &host
                    .resources
                    .lib_dir
                    .join(format!("libbirnam_{name}.dylib")),
                &[host.namespace],
            )
            .unwrap();
        }
        let string = named(b"String");
        let integer = named(b"Integer");
        let float = named(b"Float");
        for name in [b"true".as_slice(), b"nil"] {
            call(named(name), b"installPrinting:", &[string]);
        }
        call(integer, b"installFloat:", &[float]);
        call(string, b"installNumbers:", &[integer, float]);
        call(
            host.id.root,
            b"parentOf:put:",
            &[named(b"StringVt"), named(b"Collection")],
        );
        call(string, b"installCollections:", &[named(b"Vector")]);
    }
    host
}

/// Releasable test roots keep lifetime assertions independent of caller helpers.
pub struct H<'a> {
    pub host: &'a Context,
    tokens: std::cell::RefCell<Vec<Oop>>,
}
impl<'a> H<'a> {
    pub fn new(host: &'a Context) -> Self {
        Self {
            host,
            tokens: std::cell::RefCell::new(vec![]),
        }
    }
    pub fn named(&self, n: &[u8]) -> Oop {
        self.host.named(n).unwrap()
    }
    pub fn keep(&self, o: Oop) -> Oop {
        self.tokens
            .borrow_mut()
            .push(self.host.send(self.host.roots, b"retain:", &[o]).unwrap());
        o
    }
    pub fn key(&self, n: &[u8]) -> Oop {
        self.keep(self.host.intern(n).unwrap())
    }
    pub fn call(&self, r: Oop, n: &[u8], a: &[Oop]) -> Oop {
        self.keep(self.host.send(r, n, a).unwrap())
    }
    pub fn text(&self, b: &[u8]) -> Oop {
        self.keep(self.host.scope().string(b).unwrap())
    }
    pub fn eval(&self, b: &[u8]) -> Oop {
        let value = self.call(self.named(b"Codegen"), b"evalPersisting:", &[self.text(b)]);
        self.host.scope().check_result(value).unwrap();
        value
    }
    pub fn artifact(&self, name: &str) -> Oop {
        eprintln!("loading actual initializer: {name}");
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
        assert_ne!(value, self.named(b"nil"), "nil initializer: {name}");
        self.host.scope().check_result(value).unwrap();
        value
    }
    pub fn mount(&self, name: &str) {
        self.host
            .load_native(
                &self.host.scope(),
                &self
                    .host
                    .resources
                    .lib_dir
                    .join(format!("libbirnam_{name}.dylib")),
                &[self.host.namespace],
            )
            .unwrap();
    }
    pub fn mark(&self) -> usize {
        self.tokens.borrow().len()
    }
    pub fn clear_from(&self, i: usize) {
        for token in self.tokens.borrow_mut().drain(i..).rev() {
            self.host
                .send(self.host.roots, b"release:", &[token])
                .unwrap();
        }
    }
    pub fn collect(&self) {
        self.host
            .send(self.named(b"GC"), b"collectPrecisely", &[])
            .unwrap();
    }
}
impl Drop for H<'_> {
    fn drop(&mut self) {
        self.clear_from(0);
    }
}
