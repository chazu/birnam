//! Integer ownership only: Id decides journal admission, issuance and recovery.
use birnam_abi::deferred_identity::FactoryV1;
use core::{cell::Cell, ffi::c_void};

unsafe extern "C" fn create() -> *mut c_void {
    Box::into_raw(Box::new(Cell::new(0u64))).cast()
}
unsafe extern "C" fn destroy(context: *mut c_void) {
    unsafe { drop(Box::from_raw(context.cast::<Cell<u64>>())) };
}
unsafe extern "C" fn read(context: *const c_void) -> u64 {
    unsafe { (&*context.cast::<Cell<u64>>()).get() }
}
unsafe extern "C" fn write(context: *mut c_void, value: u64) {
    unsafe { (&*context.cast::<Cell<u64>>()).set(value) };
}
static FACTORY: FactoryV1 = FactoryV1 {
    version: 1,
    byte_size: size_of::<FactoryV1>(),
    create: Some(create),
    destroy: Some(destroy),
    read: Some(read),
    write: Some(write),
};
/// # Safety
/// The host pins this descriptor and its code for the lifetime of every owner.
pub unsafe fn deferred_identity_factory() -> *const FactoryV1 {
    &FACTORY
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zero_initialization_all_bits_and_unconditional_assignment() {
        unsafe {
            let context = create();
            assert!(!context.is_null());
            assert_eq!(read(context), 0);
            for value in [0, 1, u64::MAX, 19, u64::MAX - 1, 0]
                .into_iter()
                .chain((0..64).flat_map(|i| [1u64 << i, !(1u64 << i)]))
            {
                for _ in 0..3 {
                    write(context, value);
                    assert_eq!(read(context), value);
                }
            }
            destroy(context);
        }
    }
    #[test]
    fn distinct_moved_owners_survive_other_owner_destruction() {
        unsafe {
            let mut owners = vec![create(), create()];
            assert_ne!(owners[0], owners[1]);
            let moved = owners.remove(0);
            write(moved, u64::MAX);
            assert_eq!(read(owners[0]), 0);
            write(owners[0], 19);
            assert_eq!(read(moved), u64::MAX);
            destroy(moved);
            assert_eq!(read(owners[0]), 19);
            write(owners[0], 0);
            assert_eq!(read(owners[0]), 0);
            destroy(owners[0]);
        }
    }
}
