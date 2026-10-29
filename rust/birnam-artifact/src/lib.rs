//! Artifact formats and their optional native inspection owner.
#[path = "codec.rs"]
mod codec;
pub use codec::*;

#[cfg(feature = "module")]
mod module;
