//! Local value contracts and Id projections; no native owner or project state.
mod data;
pub use data::*;
pub mod git;
pub mod values;
use birnam_host_support::HostError;
