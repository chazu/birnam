//! Installed native test caller. Runtime interactions remain ordinary Id sends.
use birnam_host_support::{Context, Resources};
use std::{env, os::unix::ffi::OsStrExt, path::PathBuf};
#[path = "../../../birnam-host/src/test_hooks.rs"]
#[allow(dead_code)]
pub mod hooks;

pub fn bootstrap() -> Context {
    let lib = PathBuf::from(env::var_os("BIRNAM_TEST_MODULE_DIRECTORY").unwrap());
    let prefix = lib.parent().unwrap().to_path_buf();
    let resources = Resources {
        id_dylib: lib.join("libbirnam_id.dylib"),
        rust_workspace_dir: prefix.join("share/birnam/src"),
        guide_dir: prefix.join("share/birnam/docs/guide"),
        prefix,
        lib_dir: lib.clone(),
    };
    let bootstrap = lib.join("libbirnam_bootstrap.dylib");
    let id = birnam_siphon::load(
        resources.id_dylib.as_os_str().as_bytes(),
        &[
            bootstrap.as_os_str().as_bytes(),
            lib.as_os_str().as_bytes(),
            b"nameWithBytes",
            b"sequence",
        ],
    )
    .unwrap();
    unsafe {
        let constructor = id.send(id.literal(2).unwrap(), id.intern, &[]).unwrap();
        let sequence = id.send(id.literal(3).unwrap(), id.intern, &[]).unwrap();
        let args = id
            .send(id.root, sequence, &[id.literal(1).unwrap()])
            .unwrap();
        let namespace = id.send(id.literal(0).unwrap(), id.load, &[args]).unwrap();
        Context::from_namespace(id, namespace, resources, constructor).unwrap()
    }
}
