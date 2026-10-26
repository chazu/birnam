use birnam_abi::Oop;
use birnam_host_support::{Context, HostError, native};
mod project;
// Preserve the pure Rust validation contracts; execution crosses Id only.
pub use birnam_host_support::{MAX_SCRIPT_BYTES, StartupPolicy};
pub use project::data::*;
mod runtime {
    #[cfg(test)]
    pub use super::test_support::HostRuntime;
    pub use birnam_host_support::{HostError, Scope, native_error};
}
mod program {
    pub use birnam_host_support::{MAX_SCRIPT_BYTES, read_source_file};
}
#[cfg(test)]
#[path = "../../birnam-host/src/resources.rs"]
#[allow(dead_code)] // Fixture needs only the executable-relative resolver.
mod test_resources;
#[cfg(test)]
mod test_support;
#[cfg(test)]
pub use test_resources::{Overrides, resolve_from_exe_dir};
#[cfg(test)]
mod project_query_contract;
fn dispatch(context: &Context, operation: &str, args: &[Oop]) -> Result<Oop, HostError> {
    if args.len() != operation.bytes().filter(|b| *b == b':').count() {
        return Err(HostError(format!("{operation}: wrong arity")));
    }
    let q = context.scope();

    let project = project::Project { host: context };
    let result = (|| {
        match operation {
            "support" => project.project_support()?,
            "root:" => {
                use std::os::unix::ffi::OsStrExt;
                let path = project::files::nearest_project_root(&native::path(&q, args, 0)?)
                    .map_err(|e| HostError(format!("{e:?}")))?;
                return Ok(q.string(path.as_os_str().as_bytes())?);
            }
            "manifest:" => {
                return native::encode(&q, &project.project_manifest(&native::path(&q, args, 0)?)?);
            }
            "snapshot:" => {
                return native::encode(
                    &q,
                    &project.observe_project_snapshot(&native::path(&q, args, 0)?)?,
                );
            }
            "resolveSnapshot:" => {
                return native::encode(
                    &q,
                    &project.project_resolution(&native::decode(&q, args, 0)?)?,
                );
            }
            "observeBytes:path:root:" => {
                return native::encode(
                    &q,
                    &project.project_source_bytes(
                        native::bytes(&q, args, 0)?,
                        &native::path(&q, args, 1)?,
                        &native::text(&q, args, 2)?,
                    )?,
                );
            }
            "dependencyPaths:dependencies:" => {
                return native::encode(
                    &q,
                    &project::files::resolve_project_dependency_paths(
                        &native::path(&q, args, 0)?,
                        &native::decode::<Vec<project::data::ProjectDependencyObservation>>(
                            &q, args, 1,
                        )?,
                    )
                    .map_err(|e| HostError(format!("{e:?}")))?,
                );
            }
            "testSources:" => {
                use std::os::unix::ffi::OsStrExt;
                let (path, files) = project.project_test_sources(&native::path(&q, args, 0)?)?;
                return native::encode(
                    &q,
                    &(
                        path.as_os_str().as_bytes(),
                        files
                            .iter()
                            .map(|p| p.as_os_str().as_bytes())
                            .collect::<Vec<_>>(),
                    ),
                );
            }
            "pathForUri:" => {
                return project.path_for_uri(&native::text(&q, args, 0)?);
            }
            "snapshotForUri:" => {
                return project.snapshot_for_uri(&native::text(&q, args, 0)?);
            }
            "index:" => {
                return native::encode(
                    &q,
                    &project.index_project_source(&native::path(&q, args, 0)?)?,
                );
            }
            "observe:root:composition:" => {
                return native::encode(
                    &q,
                    &project.observe_project_source(
                        &native::path(&q, args, 0)?,
                        &native::text(&q, args, 1)?,
                        &native::decode::<Vec<String>>(&q, args, 2)?,
                    )?,
                );
            }
            "embeddedFiles:" => {
                return native::encode(
                    &q,
                    &project.project_embedded_files(&native::path(&q, args, 0)?)?,
                );
            }
            _ => return Err(HostError("unknown HostProject operation".into())),
        }
        Ok(context.named(b"nil")?)
    })();
    result.and_then(|value| q.finish(value))
}
birnam_host_support::host_owner!(
    b"HostProject",
    dispatch,
    (operation_0, "support"),
    (operation_1, "root:"),
    (operation_2, "manifest:"),
    (operation_3, "snapshot:"),
    (operation_4, "resolveSnapshot:"),
    (operation_5, "observeBytes:path:root:"),
    (operation_6, "dependencyPaths:dependencies:"),
    (operation_7, "testSources:"),
    (operation_8, "index:"),
    (operation_9, "observe:root:composition:"),
    (operation_10, "embeddedFiles:"),
    (operation_11, "pathForUri:"),
    (operation_12, "snapshotForUri:"),
);
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
