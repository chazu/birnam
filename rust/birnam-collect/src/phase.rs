//! Per-Runtime phase ownership, separate from the driver's thread-local ACTIVE.
use birnam_abi::collect::{FINALIZING, IDLE, MARKING};
use birnam_abi::collection_phase::FactoryV1;
use core::{
    cell::Cell,
    ffi::{c_int, c_void},
};

unsafe extern "C" fn create() -> *mut c_void {
    Box::into_raw(Box::new(Cell::new(IDLE))).cast()
}
unsafe extern "C" fn destroy(context: *mut c_void) {
    unsafe { drop(Box::from_raw(context.cast::<Cell<u32>>())) };
}
unsafe extern "C" fn read(context: *const c_void) -> u32 {
    unsafe { (&*context.cast::<Cell<u32>>()).get() }
}
unsafe extern "C" fn set(context: *mut c_void, phase: u32) -> c_int {
    if !matches!(phase, IDLE | MARKING | FINALIZING) {
        return 0;
    }
    unsafe { (&*context.cast::<Cell<u32>>()).set(phase) };
    1
}
static FACTORY: FactoryV1 = FactoryV1 {
    version: 1,
    byte_size: size_of::<FactoryV1>(),
    create: Some(create),
    destroy: Some(destroy),
    read: Some(read),
    set: Some(set),
};
/// # Safety
/// The host pins this immutable descriptor and code for all created contexts.
pub unsafe fn collection_phase_factory() -> *const FactoryV1 {
    &FACTORY
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owners_are_independent_and_valid_assignments_have_no_transition_rules() {
        unsafe {
            let a = create();
            let b = create();
            assert!(!a.is_null() && !b.is_null() && a != b);
            assert_eq!(read(a), IDLE);
            assert_eq!(read(b), IDLE);
            for before in [IDLE, MARKING, FINALIZING] {
                for after in [IDLE, MARKING, FINALIZING] {
                    assert_eq!(set(a, before), 1);
                    assert_eq!(set(a, after), 1);
                    assert_eq!(read(a), after);
                    assert_eq!(read(b), IDLE);
                    assert_eq!(set(a, after), 1);
                    assert_eq!(read(a), after);
                }
            }
            destroy(a);
            assert_eq!(set(b, MARKING), 1);
            assert_eq!(read(b), MARKING);
            destroy(b);
        }
    }
    #[test]
    fn invalid_phase_preserves_each_previous_value() {
        unsafe {
            let owner = create();
            for phase in [IDLE, MARKING, FINALIZING] {
                assert_eq!(set(owner, phase), 1);
                for invalid in [3, 17, u32::MAX] {
                    assert_eq!(set(owner, invalid), 0);
                    assert_eq!(read(owner), phase);
                }
            }
            destroy(owner);
        }
    }
}
