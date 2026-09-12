//! Pinned declarations of exact, non-observing native payload projections.
use super::numeric::{ImagePin, resident_declaration};
use super::*;
use crate::abi::{Method, Oop};

pub(crate) struct FieldSummary {
    method: Method,
    query: unsafe extern "C" fn(*const c_void, Oop) -> u32,
    _image: ImagePin,
}
impl FieldSummary {
    pub(crate) fn discover(method: Method, nil: Oop) -> Option<Self> {
        let (image, declaration) =
            resident_declaration(method, c"birnam_native_field_projection_v1")?;
        // The native producer owns this pure-query ABI and semantic promise.
        let summary = Self {
            method,
            query: unsafe { core::mem::transmute(declaration) },
            _image: image,
        };
        summary.field(nil)?;
        Some(summary)
    }
    pub(crate) fn field(&self, nil: Oop) -> Option<usize> {
        let code = unsafe { (self.query)(self.method as *const c_void, nil) };
        (1..=128).contains(&code).then(|| code as usize - 1)
    }
}
