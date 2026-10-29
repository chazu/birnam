//! Provider-free view of the Birnam artifact descriptor codec.
//!
//! The pure implementation is shared with `birnam-artifact`. This crate has no
//! provider module or provider feature. It is the artifact surface imported by
//! the canonical Rust Codegen dylib.

#![cfg(not(test))]

#[path = "../../birnam-artifact/src/codec.rs"]
mod codec;

pub use codec::*;
