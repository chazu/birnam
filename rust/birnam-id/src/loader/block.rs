//! Native constructor facts pinned to their actual resident implementation.
use super::numeric::{ImagePin, resident_address_declaration};
use crate::abi::Oop;
use core::ffi::c_void;

pub(crate) struct BlockConstructionSummary {
    constructor: *const c_void,
    query: unsafe extern "C" fn(
        *const c_void,
        Oop,
        *const usize,
        Oop,
        Oop,
        usize,
        Oop,
        *const u8,
        usize,
        Oop,
    ) -> u32,
    _image: ImagePin,
}

impl BlockConstructionSummary {
    pub(crate) fn discover(constructor: *const c_void) -> Option<Self> {
        let (image, declaration) =
            resident_address_declaration(constructor, c"birnam_native_block_construction_v4")?;
        Some(Self {
            constructor,
            query: unsafe { core::mem::transmute(declaration) },
            _image: image,
        })
    }

    pub(crate) fn matches(&self, constructor: *const c_void) -> bool {
        self.constructor == constructor
    }

    /// No result is cached: carrier, code meaning and the producer's current
    /// API must still agree at the actual constructor boundary. Caller holds
    /// all input objects live and separately proves allocation/GC eligibility.
    pub(crate) unsafe fn warm(
        &self,
        runtime: &crate::runtime::Runtime,
        block_vtable: Oop,
        code_object: Oop,
        capture_count: usize,
        materialization_owner: Oop,
        program: &[u8],
        materialized: bool,
    ) -> bool {
        let Some(owner) = runtime.default_block_behavior_owner(block_vtable) else {
            return false;
        };
        let globals = runtime.globals();
        let context = crate::public_api::block_construction_context(
            runtime,
            globals.symbol_vt,
            globals.vtable_vt,
        );
        let mode = unsafe {
            (self.query)(
                self.constructor,
                runtime.canonical().nil,
                context.as_ptr(),
                block_vtable,
                code_object,
                capture_count,
                materialization_owner,
                program.as_ptr(),
                program.len(),
                owner,
            )
        };
        mode == if materialized { 2 } else { 1 }
    }
}
