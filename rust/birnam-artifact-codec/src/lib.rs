//! Provider-free view of the Birnam artifact descriptor codec.
//!
//! The implementation is shared with `birnam-artifact`, but this crate does
//! not enable its provider module. It is the only artifact surface imported by
//! the canonical Rust Codegen dylib.

#![cfg(not(test))]

#[path = "../../birnam-artifact/src/lib.rs"]
mod provider_source;

pub use provider_source::*;
