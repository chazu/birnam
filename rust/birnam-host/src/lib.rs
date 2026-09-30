//! Owner-thread source, image, artifact and project execution through Id messages.
//! Console, socket and authored tools share the same scoped owner-thread roots.
mod resources;
pub use resources::{Overrides, Resources, resolve, resolve_from_exe_dir};
mod startup;
pub use startup::{
    HostError, HostRuntime, MAX_SCRIPT_BYTES, ProjectSourceMetadata, ProjectSourceObservation,
    StartupPolicy,
};
pub mod bootstrap_receipt;
pub mod publication_receipt;
mod receipt_validation;

pub mod project_data;
mod project_files;
pub use project_data::*;

mod baked;
pub use baked::{bake_executable, baked_image, read_baked_image};
