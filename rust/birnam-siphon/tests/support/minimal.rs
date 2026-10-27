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
