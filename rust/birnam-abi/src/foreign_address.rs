//! Shared process-local ForeignAddress object contract.
//!
//! This is a Rust-side helper over the current IdApi, not a new IdApi field. A
//! ForeignAddress carries a native address plus an ownership kind in two raw
//! words. Its family prototype is exported, while every non-prototype instance
//! is a process-local capability and must be rejected by image persistence.

use crate::{IdApi, Oop, SLOT_RAW};
use core::mem::size_of;
use core::ptr;

pub const VTABLE_NAME: &[u8] = b"ForeignAddressVt\0";
pub const PROTOTYPE_NAME: &[u8] = b"ForeignAddress\0";

pub const KIND_PROTOTYPE: usize = 0;
pub const KIND_BORROWED: usize = 1;
pub const KIND_ALIEN_MALLOC: usize = 2;

pub const PAYLOAD_WORDS: usize = 2;
pub const PAYLOAD_BYTES: usize = PAYLOAD_WORDS * size_of::<usize>();

/// Ensure the canonical family exists and return `(vtable, prototype)`.
///
/// # Safety
///
/// The caller must own the Runtime thread and supply its live IdApi.
pub unsafe fn ensure_family(api: &IdApi) -> Option<(Oop, Oop)> {
    let existing_vtable = unsafe { (api.isExported)(VTABLE_NAME.as_ptr().cast()) } != 0;
    let existing_prototype = unsafe { (api.isExported)(PROTOTYPE_NAME.as_ptr().cast()) } != 0;
    if existing_vtable || existing_prototype {
        if !(existing_vtable && existing_prototype) {
            return None;
        }
        let vtable = unsafe { (api.import)(VTABLE_NAME.as_ptr().cast()) };
        let prototype = unsafe { (api.import)(PROTOTYPE_NAME.as_ptr().cast()) };
        return validate_family(api, vtable, prototype).then_some((vtable, prototype));
    }

    let vtable = unsafe { (api.proto)(api.object_vt) };
    if vtable.is_null() {
        return None;
    }
    unsafe { (api.setLayout)(vtable, ptr::null(), 0, SLOT_RAW) };
    let prototype = unsafe { (api.alloc)(vtable, PAYLOAD_BYTES) };
    if prototype.is_null() {
        return None;
    }
    unsafe {
        prototype.cast::<usize>().write(0);
        prototype.cast::<usize>().add(1).write(KIND_PROTOTYPE);
        crate::set_vtable_owner(api, vtable, prototype);
    }
    if unsafe { (api.exportObj)(VTABLE_NAME.as_ptr().cast(), vtable) }.is_null()
        || unsafe { (api.exportObj)(PROTOTYPE_NAME.as_ptr().cast(), prototype) }.is_null()
    {
        return None;
    }
    Some((vtable, prototype))
}

/// Allocate one address capability in the canonical family.
///
/// # Safety
///
/// The caller must own the Runtime thread. `kind` must describe the authority
/// that will govern the address's lifetime.
pub unsafe fn allocate(api: &IdApi, address: usize, kind: usize) -> Option<Oop> {
    if kind == KIND_PROTOTYPE {
        return None;
    }
    let (vtable, _) = unsafe { ensure_family(api) }?;
    let object = unsafe { (api.alloc)(vtable, PAYLOAD_BYTES) };
    if object.is_null() {
        return None;
    }
    unsafe {
        object.cast::<usize>().write(address);
        object.cast::<usize>().add(1).write(kind);
    }
    Some(object)
}

/// Return `(address, kind)` only for a live member of the ForeignAddress family.
///
/// # Safety
///
/// The caller must preserve the Oop across the query and subsequent use.
pub unsafe fn value(api: &IdApi, object: Oop) -> Option<(usize, usize)> {
    if object.is_null() || unsafe { (api.objectPayloadSize)(object) } != PAYLOAD_BYTES {
        return None;
    }
    let family = unsafe { (api.import)(VTABLE_NAME.as_ptr().cast()) };
    if family.is_null() {
        return None;
    }
    let mut vtable = unsafe { (api.vtableOf)(object) };
    let mut remaining = unsafe { (api.vtableCount)() }.saturating_add(1);
    while !vtable.is_null() && remaining != 0 {
        if vtable == family {
            return Some(unsafe {
                (
                    object.cast::<usize>().read(),
                    object.cast::<usize>().add(1).read(),
                )
            });
        }
        vtable = unsafe { crate::vtable_parent(api, vtable) };
        remaining -= 1;
    }
    None
}

/// Clear a validated capability after its owner releases the address.
///
/// # Safety
///
/// The caller must own the Runtime thread and ensure no foreign operation is
/// concurrently using the object.
pub unsafe fn clear(api: &IdApi, object: Oop) -> bool {
    if unsafe { value(api, object) }.is_none() {
        return false;
    }
    unsafe { object.cast::<usize>().write(0) };
    true
}

fn validate_family(api: &IdApi, vtable: Oop, prototype: Oop) -> bool {
    !vtable.is_null()
        && !prototype.is_null()
        && unsafe { (api.vtableOf)(prototype) } == vtable
        && unsafe { crate::vtable_owner(api, vtable) } == prototype
        && unsafe { (api.objectPayloadSize)(prototype) } == PAYLOAD_BYTES
        && unsafe { prototype.cast::<usize>().read() } == 0
        && unsafe { prototype.cast::<usize>().add(1).read() } == KIND_PROTOTYPE
}
