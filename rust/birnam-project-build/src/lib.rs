use birnam_abi::id_protocol::{self as abi, Operand, Outcome, Protocol};
use birnam_host_support::{
    Context, HostError,
    native::{self, Owner},
};
use std::{
    ops::Deref,
    sync::atomic::{AtomicBool, AtomicPtr},
};
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
fn dispatch(context: &Context, operation: &str, args: &[Operand]) -> Result<Operand, HostError> {
    if args.len() != operation.bytes().filter(|b| *b == b':').count() {
        return Err(HostError(format!("{operation}: wrong arity")));
    }
    let host = HostRuntime { context };
    let q = context.scope();

    let project = project::Project { host: context };
    use birnam_host_support::project::*;
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
            return Ok(Operand::object(q.call(
                host.named(b"Integer")?,
                b"fromWord:",
                &[Operand::word(count)],
            )?));
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
    Ok(Operand::object(host.named(b"nil")?))
}
static INSTANCE: AtomicPtr<Owner> = AtomicPtr::new(std::ptr::null_mut());
static BUSY: AtomicBool = AtomicBool::new(false);
unsafe extern "C" fn initialize(id: *const Protocol, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        native::initialize(
            id,
            a,
            n,
            &INSTANCE,
            &BUSY,
            b"ProjectBuild",
            &[
                "check:",
                "fetch:",
                "build:target:profile:version:",
                "locate:profile:",
                "install:directory:",
                "test:source:profile:",
                "run:version:",
            ],
            dispatch,
        )
    }
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: abi::Module = abi::Module {
    version: 1,
    byte_size: size_of::<abi::Module>() as u32,
    initialize: Some(initialize),
};
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
