use birnam_host_support::{Context, HostError, Resources, StartupPolicy, native_error};
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
            let key = scope.keep(host.context.intern(b"Alien")
                .map_err(|e| native_error("intern Alien", e))?)?;
            let alien = host.context.send(host.context.namespace, b"at:", &[key])?;
            // The build-only compiler seed has no Alien owner. Published
            // Bootstrap still requires it before this Host boundary is reached.
            if !alien.is_null() {
                scope.keep(alien)?;
                let directory = scope.string(resources.lib_dir.as_os_str().as_bytes())?;
                scope.call(alien, b"libraryDir:", &[directory])?;
            }
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
                    h.named(if p.source_bootstrap {
                        b"true"
                    } else {
                        b"false"
                    })?,
                    h.named(if p.tool_bootstrap { b"true" } else { b"false" })?,
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
