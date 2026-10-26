use birnam_abi::Oop;
use birnam_host_support::{Context, HostError, native};
use std::ops::Deref;
struct HostRuntime<'a> {
    context: &'a Context,
}
impl Deref for HostRuntime<'_> {
    type Target = Context;
    fn deref(&self) -> &Context {
        self.context
    }
}
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
    pub use birnam_host_support::read_source_file;
}
#[cfg(test)]
#[path = "../../birnam-host/src/resources.rs"]
#[allow(dead_code)] // Fixture needs only the executable-relative resolver.
mod test_resources;
#[cfg(test)]
mod test_support;
#[cfg(test)]
pub use test_resources::{Overrides, resolve_from_exe_dir};
fn dispatch(context: &Context, operation: &str, args: &[Oop]) -> Result<Oop, HostError> {
    if args.len() != operation.bytes().filter(|b| *b == b':').count() {
        return Err(HostError(format!("{operation}: wrong arity")));
    }
    let host = HostRuntime { context };
    let q = context.scope();

    let project = project::Project { host: context };
    use birnam_host_support::project::*;
    let result = (|| {
        match operation {
            "check:" => {
                return native::encode(&q, &project.check_project(&native::path(&q, args, 0)?)?);
            }
            "fetch:" => {
                return native::encode(
                    &q,
                    &project.fetch_project_dependencies(&native::path(&q, args, 0)?)?,
                );
            }
            "build:target:profile:version:" => {
                return native::encode(
                    &q,
                    &project.build_project(&ProjectBuildRequest {
                        start: &native::path(&q, args, 0)?,
                        target: &native::text(&q, args, 1)?,
                        profile: &native::text(&q, args, 2)?,
                        tool_version: &native::text(&q, args, 3)?,
                    })?,
                );
            }
            "locate:profile:" => {
                return native::encode(
                    &q,
                    &project.locate_project_build(
                        &native::path(&q, args, 0)?,
                        &native::text(&q, args, 1)?,
                    )?,
                );
            }
            "install:directory:" => {
                let count = project.install_project_build(
                    &native::decode(&q, args, 0)?,
                    &native::path(&q, args, 1)?,
                )?;
                return Ok(q.call(
                    host.named(b"Integer")?,
                    b"fromWord:",
                    &[birnam_host_support::size(count)?],
                )?);
            }
            "test:source:profile:" => project.run_project_test_source(
                &native::path(&q, args, 0)?,
                &native::path(&q, args, 1)?,
                &native::text(&q, args, 2)?,
            )?,
            "run:version:" => project.run_project(
                &native::decode::<RunOptions>(&q, args, 0)?,
                &native::text(&q, args, 1)?,
            )?,
            _ => return Err(HostError("unknown ProjectBuild operation".into())),
        }
        Ok(host.named(b"nil")?)
    })();
    result.and_then(|value| q.finish(value))
}
birnam_host_support::host_owner!(
    b"ProjectBuild",
    dispatch,
    (operation_0, "check:"),
    (operation_1, "fetch:"),
    (operation_2, "build:target:profile:version:"),
    (operation_3, "locate:profile:"),
    (operation_4, "install:directory:"),
    (operation_5, "test:source:profile:"),
    (operation_6, "run:version:"),
);
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
