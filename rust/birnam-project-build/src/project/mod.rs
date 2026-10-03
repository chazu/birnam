//! One native project responsibility over current Id dependencies.
pub(crate) mod data;
mod files;
mod observation;
mod operations;
use birnam_host_support::Context;
use birnam_project_support::values;
pub struct Project<'h> {
    pub(crate) host: &'h Context,
}
