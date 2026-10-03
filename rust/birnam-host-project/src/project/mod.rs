//! One native project responsibility over current Id dependencies.
pub(crate) mod data;
pub(crate) mod files;
mod operations;
mod query;
mod source;
use birnam_host_support::Context;
pub use birnam_host_support::project::{ProjectSourceMetadata, ProjectSourceObservation};
use birnam_project_support::values;
pub struct Project<'h> {
    pub(crate) host: &'h Context,
}
