//! Borrowed live namespace-presence selection (81BA).
use core::ffi::{c_char, c_int, c_void};

pub type ContainsV1 = unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int;
pub type SlotV1 = unsafe extern "C" fn(*const c_char) -> c_int;

/// Caller-owned, owner-thread descriptor. Its current function and context are
/// selected together at each invocation, not when the capability is handed off.
/// The descriptor, context and code stay valid through the synchronous borrow;
/// neither this adapter nor a recipient gains ownership or may retain them.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub contains: Option<ContainsV1>,
}
const _: () = {
    assert!(size_of::<ApiV1>() == 32);
    assert!(core::mem::offset_of!(ApiV1, contains) == 24);
};

/// # Safety
/// The descriptor remains readable and valid through the call. Its selected
/// callback accepts the name and context and may reenter or collect. No shared
/// Rust reference to the descriptor is retained across that callback.
pub unsafe fn contains(api: *const ApiV1, name: *const c_char) -> c_int {
    let selected = unsafe { *api };
    unsafe { selected.contains.unwrap()(selected.context, name) }
}

unsafe extern "C" fn invoke_slot(context: *mut c_void, name: *const c_char) -> c_int {
    let selected = unsafe { *context.cast::<SlotV1>() };
    unsafe { selected(name) }
}

/// Borrow an existing live function slot while its callers migrate. This does
/// not copy the callback: changes during preceding callbacks remain observable.
/// The helper knows nothing about IdApi or where the slot is stored.
///
/// # Safety
/// The aligned slot and its selected code stay valid through every use of the
/// returned capability. Access and mutation are serialized on the owner thread.
pub unsafe fn borrow_slot(slot: *const SlotV1) -> ApiV1 {
    ApiV1 {
        version: 1,
        byte_size: size_of::<ApiV1>(),
        context: slot.cast_mut().cast(),
        contains: Some(invoke_slot),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" fn first(_: *const c_char) -> c_int {
        7
    }
    unsafe extern "C" fn second(_: *const c_char) -> c_int {
        -3
    }
    unsafe extern "C" fn context_value(context: *mut c_void, _: *const c_char) -> c_int {
        unsafe { *context.cast::<c_int>() }
    }
    #[test]
    fn handoff_keeps_independent_slots_and_live_function_context_selection() {
        let mut left: SlotV1 = first;
        let right: SlotV1 = second;
        let mut capability = unsafe { borrow_slot(&raw const left) };
        let other = unsafe { borrow_slot(&right) };
        assert_eq!(unsafe { contains(&capability, c"Error".as_ptr()) }, 7);
        unsafe { (&raw mut left).write(second) };
        assert_eq!(unsafe { contains(&capability, c"Error".as_ptr()) }, -3);
        // The callback's source slot is still independent from the other loan.
        unsafe { (&raw mut left).write(first) };
        assert_eq!(unsafe { contains(&other, c"Error".as_ptr()) }, -3);
        assert_eq!(unsafe { contains(&capability, c"Error".as_ptr()) }, 7);
        let mut result = 11;
        capability.contains = Some(context_value);
        capability.context = (&raw mut result).cast();
        assert_eq!(unsafe { contains(&capability, c"ErrorVt".as_ptr()) }, 11);
        let mut replacement = 0;
        capability.context = (&raw mut replacement).cast();
        assert_eq!(unsafe { contains(&capability, c"ErrorVt".as_ptr()) }, 0);
    }
}
