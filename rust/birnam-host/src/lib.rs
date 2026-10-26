//! Executable handoff to native Host owners through Id messages.
mod resources;
pub use resources::{Overrides, Resources, resolve, resolve_from_exe_dir};
mod runtime;
pub use runtime::{HostError, HostRuntime};
mod startup;
pub use birnam_host_support::{MAX_SCRIPT_BYTES, StartupPolicy};
mod baked;
mod image;
mod interaction;
mod program;
pub mod project;
mod worker;
pub use baked::{bake_executable, baked_image, read_baked_image};
pub mod bootstrap_receipt;
pub mod publication_receipt;
mod receipt_validation;

#[cfg(test)]
mod test_hooks;
