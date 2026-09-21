//! Complete upper lifetime records, initialized before Runtime or collection.
use crate::{Oop, finalizers, payload_layout as layout, trace_hooks as hooks};
use core::ffi::c_void;

pub const ALLOCATED: u32 = 1;
pub const BEHAVIOR: u32 = 2;
pub const PERMANENT: u32 = 4;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct EdgeV1 {
    pub key: usize,
    pub target: Oop,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct EdgesV1 {
    pub entries: *const EdgeV1,
    pub count: usize,
}
/// A complete working policy. Replacement changes admission and facts together.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AccessV1 {
    pub context: *mut c_void,
    pub admit: unsafe extern "C" fn(*mut c_void),
    pub facts: unsafe extern "C" fn(*mut c_void, Oop) -> u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OwnedV2 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub layouts: *mut layout::ApiV1,
    pub hooks: *mut hooks::ApiV1,
    pub finalizers: *mut finalizers::ApiV1,
    pub layout_factory: *const layout::FactoryV1,
    pub hook_factory: *const hooks::FactoryV1,
    pub finalizer_factory: *const finalizers::FactoryV1,
    pub layout: unsafe extern "C" fn(*mut c_void, Oop) -> layout::ViewV1,
    pub replace_layout: unsafe extern "C" fn(*mut c_void, Oop, layout::OwnedV1),
    pub hook: unsafe extern "C" fn(*mut c_void, Oop) -> hooks::SlotV1,
    pub replace_hook: unsafe extern "C" fn(*mut c_void, Oop, hooks::SlotV1),
    pub finalizer: unsafe extern "C" fn(*mut c_void, Oop) -> finalizers::SlotV1,
    pub replace_finalizer: unsafe extern "C" fn(*mut c_void, Oop, finalizers::SlotV1),
    /// Remove weak layout/hook records before address reuse. Finalizers are detached separately.
    pub retire: unsafe extern "C" fn(*mut c_void, *const usize, usize),
    pub set_access: unsafe extern "C" fn(*mut c_void, AccessV1),
    pub destroy: unsafe extern "C" fn(*mut c_void),
    /// Borrowed until the next edge mutation. Keys are bookkeeping, not roots.
    pub edges: unsafe extern "C" fn(*mut c_void, Oop) -> EdgesV1,
    /// Register a strong reference in native state owned by `object`. Null
    /// removes the key. The object is a weak key; registration does not root it.
    pub set_edge: unsafe extern "C" fn(*mut c_void, Oop, usize, Oop),
}
const _: () = assert!(size_of::<OwnedV2>() == 160 && size_of::<AccessV1>() == 24);
/// # Safety
/// Readable initialized descriptor; its owner and all callback code stay live.
pub unsafe fn valid(pointer: *const OwnedV2) -> bool {
    if pointer.is_null() || !pointer.addr().is_multiple_of(align_of::<OwnedV2>()) {
        return false;
    }
    if unsafe { core::ptr::addr_of!((*pointer).version).read() } != 2
        || unsafe { core::ptr::addr_of!((*pointer).byte_size).read() } != size_of::<OwnedV2>()
    {
        return false;
    }
    let owner = unsafe { *pointer };
    owner.version == 2
        && owner.byte_size == size_of::<OwnedV2>()
        && !owner.context.is_null()
        && unsafe {
            layout::valid(owner.layouts)
                && hooks::valid(owner.hooks)
                && finalizers::valid(owner.finalizers)
                && layout::valid_factory(owner.layout_factory)
                && hooks::valid_factory(owner.hook_factory)
                && finalizers::valid_factory(owner.finalizer_factory)
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_old_and_short_descriptors_before_reading_operations() {
        #[repr(C)]
        struct Header {
            version: u32,
            byte_size: usize,
        }
        for header in [
            Header {
                version: 1,
                byte_size: 144,
            },
            Header {
                version: 2,
                byte_size: 16,
            },
        ] {
            assert!(!unsafe { valid(core::ptr::from_ref(&header).cast()) });
        }
        assert!(!unsafe { valid(core::ptr::null()) });
    }
}
