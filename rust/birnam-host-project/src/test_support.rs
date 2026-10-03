use birnam_abi::id_protocol::Operand;
use birnam_host_support::{Context, HostError, Resources, StartupPolicy, native_error, object};
use std::{ops::Deref, os::unix::ffi::OsStrExt};
pub struct HostRuntime {
    context: Context,
}
impl Deref for HostRuntime {
    type Target = Context;
    fn deref(&self) -> &Context {
        &self.context
    }
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

    pub fn start(r: &Resources, p: StartupPolicy) -> Result<Self, HostError> {
        let h = Self::start_native(r, p.precise_gc)?;
        if !p.skip_stdlib {
            let q = h.scope();
            h.owner_call(
                &q,
                b"HostEntry",
                b"bootstrap:tools:",
                &[
                    Operand::object(h.named(if p.source_bootstrap {
                        b"true"
                    } else {
                        b"false"
                    })?),
                    Operand::object(h.named(if p.tool_bootstrap { b"true" } else { b"false" })?),
                ],
            )?;
        }
        Ok(h)
    }
    pub fn project(&self) -> super::project::Project<'_> {
        super::project::Project {
            host: &self.context,
        }
    }
}

// Temporary files belong only to validation, not the observation implementation.
pub(crate) struct CheckTempDirectory(pub(crate) std::path::PathBuf);
impl Drop for CheckTempDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub(crate) fn create_check_temp_directory() -> Result<CheckTempDirectory, std::io::ErrorKind> {
    let path = std::env::temp_dir().join(format!(
        "birnam-project-query-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&path).map_err(|e| e.kind())?;
    Ok(CheckTempDirectory(path))
}
