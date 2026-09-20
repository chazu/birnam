//! Borrowed live namespace-import selection (81BD).
use crate::Oop;
use core::ffi::{c_char, c_void};

pub type ImportV1 = unsafe extern "C" fn(*mut c_void, *const c_char) -> Oop;

/// Owner-thread descriptor. Select its function/context only at invocation.
/// The descriptor, selected context and code remain valid through each borrow.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub import: Option<ImportV1>,
}
const _: () = {
    assert!(size_of::<ApiV1>() == 32);
    assert!(core::mem::offset_of!(ApiV1, import) == 24);
};

/// # Safety
/// Readable descriptor and owner-thread access. No callback is invoked.
pub unsafe fn valid(api: *const ApiV1) -> bool {
    if api.is_null() {
        return false;
    }
    let selected = unsafe { *api };
    selected.version == 1 && selected.byte_size == size_of::<ApiV1>() && selected.import.is_some()
}

/// # Safety
/// The descriptor and selected callback/context are valid for this synchronous
/// call, which may reenter or collect. No shared descriptor borrow spans it.
pub unsafe fn import(api: *const ApiV1, name: *const c_char) -> Oop {
    let selected = unsafe { *api };
    unsafe { selected.import.unwrap()(selected.context, name) }
}

/// Immutable primitive access copied by Namespace during service construction.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FloorV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub access: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
    pub intern: Option<crate::namespace::InternV1>,
}
const _: () = {
    assert!(size_of::<FloorV1>() == 40);
    assert!(core::mem::offset_of!(FloorV1, access) == 24);
    assert!(core::mem::offset_of!(FloorV1, intern) == 32);
};
pub type ProviderV1 = unsafe extern "C" fn() -> *mut ApiV1;
#[cfg(test)]
mod tests {
    use super::*;
    use core::ptr;
    unsafe extern "C" fn explicit(context: *mut c_void, _: *const c_char) -> Oop {
        context
    }
    #[test]
    fn live_context_and_inert_validation() {
        assert!(!unsafe { valid(ptr::null()) });
        let mut api = ApiV1 {
            version: 1,
            byte_size: size_of::<ApiV1>(),
            context: 9usize as Oop,
            import: Some(explicit),
        };
        assert!(unsafe { valid(&api) });
        assert_eq!(unsafe { import(&api, c"name".as_ptr()) }, 9usize as Oop);
        api.context = ptr::null_mut();
        assert_eq!(unsafe { import(&api, c"name".as_ptr()) }, ptr::null_mut());
        api.version = 2;
        assert!(!unsafe { valid(&api) });
        api.version = 1;
        api.byte_size -= 1;
        assert!(!unsafe { valid(&api) });
        api.byte_size += 1;
        api.import = None;
        assert!(!unsafe { valid(&api) });
    }
}
