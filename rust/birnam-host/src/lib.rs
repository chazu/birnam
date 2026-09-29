//! Owner-thread source and image execution through the Id message boundary.
//! Project and interaction adapters remain pending migration.
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
