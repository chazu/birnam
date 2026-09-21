//! Live reverse-name capability, owned by one Namespace instance (81AX).
use crate::Oop;
use core::ffi::{c_char, c_int, c_void};
use core::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
/// Immutable floor access. Preflight preserves owner/observation checks. The
/// Symbol operation borrows bytes after preflight; neither operation allocates,
/// collects, sends, mutates, or invokes user callbacks. Context/code stay live.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FloorV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub preflight: Option<unsafe extern "C" fn(*mut c_void)>,
    pub symbol_name: Option<unsafe extern "C" fn(*mut c_void, Oop) -> *const c_char>,
}
pub type NameOfV1 = unsafe extern "C" fn(*mut c_void, Oop) -> *const c_char;
/// Mutable operation/context, read together before each owner-thread call.
/// Descriptor/code and context stay live; returned bytes borrow the selected
/// name Symbol, whose namespace binding keeps it rooted. Native replacement
/// remains subject to the same borrowed-return and owner-thread contracts.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ApiV1 {
    pub version: u32,
    pub byte_size: usize,
    pub context: *mut c_void,
    pub name_of: Option<NameOfV1>,
}
const _: () = {
    assert!(size_of::<FloorV1>() == 40);
    assert!(size_of::<ApiV1>() == 32);
    assert!(core::mem::offset_of!(ApiV1, name_of) == 24);
};
pub type ProviderV1 = unsafe extern "C" fn() -> *mut ApiV1;
pub type DependencyV1 = unsafe extern "C" fn(*mut ApiV1) -> c_int;
static DEPENDENCY: AtomicPtr<ApiV1> = AtomicPtr::new(ptr::null_mut());
/// # Safety
/// Readable aligned descriptor on the owner thread.
pub unsafe fn valid(pointer: *mut ApiV1) -> bool {
    if pointer.is_null() {
        return false;
    }
    let a = unsafe { *pointer };
    a.version == 1
        && a.byte_size == size_of::<ApiV1>()
        && !a.context.is_null()
        && a.name_of.is_some()
}
/// # Safety
/// Descriptor, current context and code stay pinned; owner-thread admission.
pub unsafe fn attach(pointer: *mut ApiV1) -> c_int {
    if !unsafe { valid(pointer) } {
        return 0;
    }
    match DEPENDENCY.compare_exchange(
        ptr::null_mut(),
        pointer,
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => 1,
        Err(existing) => c_int::from(existing == pointer),
    }
}
/// # Safety
/// Owner-thread use after attachment; no racing native mutation.
pub unsafe fn operations() -> ApiV1 {
    let pointer = DEPENDENCY.load(Ordering::Acquire);
    assert!(
        !pointer.is_null(),
        "native Namespace names dependency was not mounted"
    );
    unsafe { *pointer }
}
/// # Safety
/// Native owner-thread and borrowed Symbol-name lifetime rules apply.
pub unsafe fn name_of(value: Oop) -> *const c_char {
    let ops = unsafe { operations() };
    unsafe { ops.name_of.unwrap()(ops.context, value) }
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" fn first(context: *mut c_void, value: Oop) -> *const c_char {
        assert_eq!(context as usize, 1);
        assert_eq!(value as usize, 3);
        c"first".as_ptr()
    }
    unsafe extern "C" fn second(context: *mut c_void, value: Oop) -> *const c_char {
        assert_eq!(context as usize, 2);
        assert_eq!(value as usize, 3);
        ptr::null()
    }
    #[test]
    fn admission_and_live_operation_context_selection() {
        let api = Box::leak(Box::new(ApiV1 {
            version: 1,
            byte_size: size_of::<ApiV1>(),
            context: ptr::dangling_mut::<c_void>(),
            name_of: Some(first),
        }));
        unsafe {
            assert_eq!(attach(ptr::null_mut()), 0);
            for mut bad in [
                ApiV1 { version: 2, ..*api },
                ApiV1 {
                    byte_size: api.byte_size - 1,
                    ..*api
                },
                ApiV1 {
                    context: ptr::null_mut(),
                    ..*api
                },
                ApiV1 {
                    name_of: None,
                    ..*api
                },
            ] {
                assert_eq!(attach(&mut bad), 0);
            }
            assert_eq!(attach(api), 1);
            assert_eq!(attach(api), 1);
            let mut copy = *api;
            assert_eq!(attach(&mut copy), 0);
            assert_eq!(name_of(3usize as Oop), c"first".as_ptr());
            let snapshot = operations();
            api.context = 2usize as *mut c_void;
            api.name_of = Some(second);
            assert!(name_of(3usize as Oop).is_null());
            assert_eq!(
                snapshot.name_of.unwrap()(snapshot.context, 3usize as Oop),
                c"first".as_ptr()
            );
        }
    }
}
