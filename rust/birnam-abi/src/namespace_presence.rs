//! Borrowed live namespace-presence selection (81BA).
use core::ffi::{c_char, c_int, c_void};

pub type ContainsV1 = unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int;

/// Caller-owned, owner-thread descriptor. Its current function and context are
/// selected together at each invocation, not when the capability is handed off.
/// The descriptor, context and code stay valid through the synchronous borrow;
/// retaining recipients require the owner to pin them for the recipient lifetime.
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

/// Immutable primitive access copied by Namespace during service construction.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FloorV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub intern: Option<crate::namespace::InternV1>,
}
pub type ProviderV1 = unsafe extern "C" fn() -> *mut ApiV1;
/// # Safety
/// Readable descriptor and owner-thread access; this calls no operation.
pub unsafe fn valid(api: *const ApiV1) -> bool {
    if api.is_null() { return false; }
    let api = unsafe { *api };
    api.version == 1 && api.byte_size == size_of::<ApiV1>() && api.contains.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ptr;
    unsafe extern "C" fn selected(context: *mut c_void, name: *const c_char) -> c_int {
        assert_eq!(unsafe { core::ffi::CStr::from_ptr(name) }, c"Error");
        context as usize as c_int
    }
    unsafe extern "C" fn changed(context: *mut c_void, _: *const c_char) -> c_int {
        -(context as usize as c_int)
    }
    #[test]
    fn selection_is_live_and_validation_does_not_invoke_it() {
        assert!(!unsafe { valid(ptr::null()) });
        let mut api = ApiV1 { version: 1, byte_size: size_of::<ApiV1>(), context: 3usize as *mut c_void, contains: Some(selected) };
        assert!(unsafe { valid(&api) });
        assert_eq!(unsafe { contains(&api, c"Error".as_ptr()) }, 3);
        api.context = 7usize as *mut c_void; api.contains = Some(changed);
        assert_eq!(unsafe { contains(&api, c"Error".as_ptr()) }, -7);
        api.version = 2; assert!(!unsafe { valid(&api) });
        api.version = 1; api.byte_size -= 1; assert!(!unsafe { valid(&api) });
        api.byte_size += 1; api.contains = None; assert!(!unsafe { valid(&api) });
    }
}
