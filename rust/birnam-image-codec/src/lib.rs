//! Provider-free view of the Birnam image framing codec.
//!
//! The implementation is shared with `birnam-image`, but this crate compiles
//! only its codec module. It cannot contribute an Image provider initializer or
//! identity symbol to a consumer dylib.

#![cfg(not(test))]

#[path = "../../birnam-image/src/lib.rs"]
mod provider_source;

pub use provider_source::codec::*;
