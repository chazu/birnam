//! Completed compiler/admission extension returned by Runtime's native entry.
use core::ffi::c_void;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub resolve: Option<unsafe extern "C" fn(*mut c_void, *const u8, usize) -> *mut c_void>,
}
/// # Safety
/// `api` is null or points to a readable complete native descriptor.
pub unsafe fn valid(api: *const ApiV1) -> bool {
    !api.is_null() && unsafe {
        (*api).version == 1 && (*api).byte_size == size_of::<ApiV1>()
            && !(*api).context.is_null() && (*api).resolve.is_some()
    }
}
pub const SYMBOLS: &[&[u8]] = &[
    b"birnam_register_resumable_v1", b"birnam_register_resumable_v2",
    b"birnam_id_begin_resumable_unit_v1", b"birnam_id_begin_portable_resumable_unit_v1",
    b"birnam_id_finish_resumable_unit_v1", b"birnam_id_cancel_resumable_unit_v1",
    b"birnam_id_try_acquire_stackless_generation_lease_v1", b"birnam_id_release_stackless_generation_lease_v1",
    b"birnam_id_retire_stackless_code_generation_v1", b"birnam_id_stackless_block_vtable_image_reference_v1",
    b"birnam_id_stackless_block_vtable_matches_v1", b"birnam_id_staged_portable_block_vtable_v1",
    b"birnam_id_stackless_resumable_manifest_matches_v1",
    b"birnam_id_warm_block_construction_v1", b"birnam_id_try_closure_elision_v2",
];
