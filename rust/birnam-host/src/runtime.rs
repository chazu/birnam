use crate::Resources;
use birnam_abi::{Oop, id_protocol::Operand};
use birnam_host_support::{Context, Scope};
pub use birnam_host_support::{HostError, native_error, object};
use std::os::unix::ffi::OsStrExt;
pub struct HostRuntime {
    pub(crate) context: Context,
}
pub(crate) fn load_owners(h: &Context) -> Result<(), HostError> {
    let q = h.scope();
    let r = &h.resources;
    let mut arguments = vec![Operand::object(h.namespace)];
    for path in [
        &r.prefix,
        &r.lib_dir,
        &r.rust_workspace_dir,
        &r.guide_dir,
        &r.id_dylib,
    ] {
        arguments.push(Operand::object(q.string(path.as_os_str().as_bytes())?));
    }
    for name in [
        "host_entry",
        "host_project",
        "project_build",
        "host_interaction",
    ] {
        let path = r.lib_dir.join(format!("libbirnam_{name}.dylib"));
        let mut request = vec![Operand::bytes(path.as_os_str().as_bytes())];
        request.extend_from_slice(&arguments);
        let result = unsafe { h.id.send(h.id.receiver, h.id.load, &request) }
            .map_err(|e| native_error(&format!("load {}", path.display()), e))?;
        q.keep(result)?;
    }
    Ok(())
}
impl HostRuntime {
    pub(crate) fn start_native(resources: &Resources, precise_gc: bool) -> Result<Self, HostError> {
        let id = birnam_siphon::load(resources.id_dylib.as_os_str().as_bytes()).map_err(
            |(status, detail)| {
                HostError(format!(
                    "load {}: native status={status} detail={detail}",
                    resources.id_dylib.display()
                ))
            },
        )?;
        let bootstrap = resources.lib_dir.join("libbirnam_bootstrap.dylib");
        let namespace = object(
            unsafe {
                id.send(
                    id.receiver,
                    id.load,
                    &[
                        Operand::bytes(bootstrap.as_os_str().as_bytes()),
                        Operand::bytes(resources.lib_dir.as_os_str().as_bytes()),
                    ],
                )
            }
            .map_err(|e| native_error(&format!("load {}", bootstrap.display()), e))?,
        )?;
        let context = Context::from_namespace(id, namespace, resources.clone())?;
        let host = Self { context };
        load_owners(&host.context)?;
        host.send(
            host.named(b"GC")?,
            b"precisePayloads:",
            &[Operand::word(usize::from(precise_gc))],
        )?;
        {
            let scope = host.scope();
            let directory = scope.string(resources.lib_dir.as_os_str().as_bytes())?;
            scope.call(
                host.named(b"Alien")?,
                b"libraryDir:",
                &[Operand::object(directory)],
            )?;
        }
        Ok(host)
    }

    pub(crate) fn scope(&self) -> Scope<'_> {
        self.context.scope()
    }
    pub(crate) fn named(&self, name: &[u8]) -> Result<Oop, HostError> {
        self.context.named(name)
    }
    pub(crate) fn call(
        &self,
        owner: &[u8],
        name: &[u8],
        args: &[Operand],
    ) -> Result<(), HostError> {
        self.context
            .owner_call(&self.scope(), owner, name, args)
            .map(|_| ())
    }
    pub(crate) fn send(&self, o: Oop, name: &[u8], args: &[Operand]) -> Result<Operand, HostError> {
        self.context.send(o, name, args)
    }
    #[cfg(test)]
    pub(crate) fn global(&self, q: &Scope<'_>, name: &[u8]) -> Result<Oop, HostError> {
        self.context.global(q, name)
    }
}
#[cfg(test)]
impl HostRuntime {
    pub(crate) fn enter_running(&self) -> Result<(), HostError> {
        self.context.enter_running()
    }
    pub(crate) fn load_tool(&self, name: &[u8]) -> Result<(), HostError> {
        let q = self.scope();
        let name = q.string(name)?;
        self.call(b"HostInteraction", b"loadTool:", &[Operand::object(name)])
    }
}
impl Drop for HostRuntime {
    fn drop(&mut self) {
        let _ = self.call(b"HostEntry", b"shutdown", &[]);
    }
}
