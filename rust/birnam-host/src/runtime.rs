use crate::Resources;
use birnam_abi::Oop;
#[cfg(test)]
pub use birnam_host_support::object;
use birnam_host_support::{Context, Scope};
pub use birnam_host_support::{HostError, native_error};
use std::os::unix::ffi::OsStrExt;
pub struct HostRuntime {
    pub(crate) context: Context,
}
pub(crate) fn load_owners(h: &Context) -> Result<(), HostError> {
    let q = h.scope();
    let r = &h.resources;
    let mut arguments = vec![h.namespace];
    for path in [
        &r.prefix,
        &r.lib_dir,
        &r.rust_workspace_dir,
        &r.guide_dir,
        &r.id_dylib,
    ] {
        arguments.push(q.string(path.as_os_str().as_bytes())?);
    }
    for name in [
        "host_entry",
        "host_project",
        "project_build",
        "host_interaction",
    ] {
        let path = r.lib_dir.join(format!("libbirnam_{name}.dylib"));
        h.load_native(&q, &path, &arguments)?;
    }
    Ok(())
}
impl HostRuntime {
    pub(crate) fn start_native(resources: &Resources, precise_gc: bool) -> Result<Self, HostError> {
        let bootstrap = resources.lib_dir.join("libbirnam_bootstrap.dylib");
        let id = birnam_siphon::load(
            resources.id_dylib.as_os_str().as_bytes(),
            &[
                bootstrap.as_os_str().as_bytes(),
                resources.lib_dir.as_os_str().as_bytes(),
                b"nameWithBytes",
                b"sequence",
            ],
        )
        .map_err(|e| HostError(format!("load {}: {e}", resources.id_dylib.display())))?;
        let context = unsafe {
            let result = (|| -> Result<Context, birnam_abi::Oop> {
                let constructor = id.send(id.literal(2)?, id.intern, &[])?;
                let sequence = id.send(id.literal(3)?, id.intern, &[])?;
                let args = id.send(id.root, sequence, &[id.literal(1)?])?;
                let namespace = id.send(id.literal(0)?, id.load, &[args])?;
                Context::from_namespace(id, namespace, resources.clone(), constructor)
                    .map_err(|_| birnam_abi::object_reference(id.invalid))
            })();
            result.map_err(|e| native_error("bootstrap Host", e))?
        };
        let host = Self { context };
        load_owners(&host.context)?;
        host.send(
            host.named(b"GC")?,
            b"precisePayloads:",
            &[birnam_host_support::size(usize::from(precise_gc))?],
        )?;
        {
            let scope = host.scope();
            let directory = scope.string(resources.lib_dir.as_os_str().as_bytes())?;
            scope.call(host.named(b"Alien")?, b"libraryDir:", &[directory])?;
        }
        Ok(host)
    }

    pub(crate) fn scope(&self) -> Scope<'_> {
        self.context.scope()
    }
    pub(crate) fn named(&self, name: &[u8]) -> Result<Oop, HostError> {
        self.context.named(name)
    }
    pub(crate) fn call(&self, owner: &[u8], name: &[u8], args: &[Oop]) -> Result<(), HostError> {
        self.context
            .owner_call(&self.scope(), owner, name, args)
            .map(|_| ())
    }
    pub(crate) fn send(&self, o: Oop, name: &[u8], args: &[Oop]) -> Result<Oop, HostError> {
        self.context.send(o, name, args)
    }
    #[cfg(test)]
    pub(crate) fn global(&self, q: &Scope<'_>, name: &[u8]) -> Result<Oop, HostError> {
        self.context.global(q, name)
    }
}
#[cfg(test)]
impl HostRuntime {
    pub(crate) fn load_tool(&self, name: &[u8]) -> Result<(), HostError> {
        let q = self.scope();
        let name = q.string(name)?;
        self.call(b"HostInteraction", b"loadTool:", &[name])
    }
}
impl Drop for HostRuntime {
    fn drop(&mut self) {
        let _ = self.call(b"HostEntry", b"shutdown", &[]);
    }
}
