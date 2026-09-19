//! One-way IdApi classification for the authority-free query kernel.
//!
//! The adapter copies scalar bytes before returning. It never returns a Rust
//! reference into a floor object and never stores the supplied IdApi pointer.

use crate::Value;
use birnam_abi::{IdApi, Oop, borrowed_bytes};

/// Classify one live floor object for query matching.
///
/// Null is represented as identity zero, preserving null equality without
/// inventing a second scalar type. SmallInts, floats, and strings are copied
/// into value classes; symbols, entities, vtables, and all other objects remain
/// pointer identities. A malformed string buffer is rejected instead of being
/// read through an unchecked pointer.
///
/// # Safety
///
/// `api` must be the process-owned frozen IdApi table, and `object` must be a
/// live Oop accepted by that table for the duration of this call.
pub unsafe fn classify(api: &IdApi, object: Oop) -> Option<Value> {
    if object.is_null() {
        return Some(Value::Identity(0));
    }
    // SAFETY: the caller supplies a live floor object and process-owned table.
    if unsafe { (birnam_abi::integer::is_integer)(object) } != 0 {
        // SAFETY: isSmallInt accepted the same object for this accessor.
        return Some(Value::Integer(unsafe { (birnam_abi::integer::value)(object) }));
    }
    // SAFETY: the caller supplies a live floor object and process-owned table.
    if unsafe { (birnam_abi::float::is_float)(object) } != 0 {
        // SAFETY: isFloat accepted the same object for this accessor.
        return Some(Value::Float(unsafe { (birnam_abi::float::value)(object) }));
    }
    // SAFETY: vtableOf is valid for every non-null live Oop.
    if unsafe { (api.vtableOf)(object) } == api.string_vt {
        let mut length = 0usize;
        // SAFETY: stringBytes is the frozen accessor for a live String object.
        let bytes = unsafe { (api.stringBytes)(object, &mut length) };
        // SAFETY: borrowed_bytes validates null/count/alignment before copying.
        let bytes = unsafe { borrowed_bytes(bytes, length) }?;
        return Some(Value::String(bytes.to_vec()));
    }
    Some(Value::Identity(object as usize as u64))
}
