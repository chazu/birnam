//! Lazy native row ownership. Id retains identity lookup and index repair.
use birnam_abi::deferred_index::{RowV1};
use core::ffi::{c_int, c_void};
type Rows = Vec<RowV1>;

unsafe extern "C" fn destroy(context: *mut c_void) {
    if !context.is_null() {
        unsafe { drop(Box::from_raw(context.cast::<Rows>())) };
    }
}
unsafe extern "C" fn len(context: *const c_void) -> usize {
    if context.is_null() {
        0
    } else {
        unsafe { (&*context.cast::<Rows>()).len() }
    }
}
unsafe extern "C" fn capacity(context: *const c_void) -> usize {
    if context.is_null() {
        0
    } else {
        unsafe { (&*context.cast::<Rows>()).capacity() }
    }
}
unsafe extern "C" fn push(slot: *mut *mut c_void, row: RowV1) {
    let context = unsafe { &mut *slot };
    if context.is_null() {
        *context = Box::into_raw(Box::new(Rows::new())).cast();
    }
    unsafe { (&mut *context.cast::<Rows>()).push(row) };
}
unsafe extern "C" fn read(context: *const c_void, position: usize, output: *mut RowV1) -> c_int {
    if context.is_null() {
        return 0;
    }
    let Some(row) = (unsafe { &*context.cast::<Rows>() }).get(position) else {
        return 0;
    };
    unsafe { output.write(*row) };
    1
}
unsafe extern "C" fn write(context: *mut c_void, position: usize, row: RowV1) -> c_int {
    if context.is_null() {
        return 0;
    }
    let Some(stored) = (unsafe { &mut *context.cast::<Rows>() }).get_mut(position) else {
        return 0;
    };
    *stored = row;
    1
}
unsafe extern "C" fn swap_remove(
    context: *mut c_void,
    position: usize,
    output: *mut RowV1,
) -> c_int {
    if context.is_null() {
        return 0;
    }
    let rows = unsafe { &mut *context.cast::<Rows>() };
    if position >= rows.len() {
        return 0;
    }
    unsafe { output.write(rows.swap_remove(position)) };
    1
}
unsafe extern "C" fn clear(context: *mut c_void) {
    if !context.is_null() {
        unsafe { (&mut *context.cast::<Rows>()).clear() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn null_all_bits_duplicate_rows_misses_and_exact_vector_order() {
        unsafe {
            let mut context = core::ptr::null_mut();
            let sentinel = RowV1 {
                identity: 9,
                index: 73,
            };
            let mut out = sentinel;
            assert_eq!(len(context), 0);
            assert_eq!(capacity(context), 0);
            assert_eq!(read(context, 0, &mut out), 0);
            assert_eq!(write(context, 0, sentinel), 0);
            assert_eq!(swap_remove(context, 0, &mut out), 0);
            clear(context);
            destroy(context);
            assert!(context.is_null());
            assert_eq!(out, sentinel);
            let mut oracle = Vec::new();
            for bits in [0, u64::MAX]
                .into_iter()
                .chain((0..64).flat_map(|i| [1u64 << i, !(1u64 << i)]))
            {
                for _ in 0..2 {
                    let row = RowV1 {
                        identity: bits,
                        index: !bits as usize,
                    };
                    push(&mut context, row);
                    oracle.push(row);
                    assert_eq!(len(context), oracle.len());
                    assert_eq!(capacity(context), oracle.capacity());
                }
            }
            let retained = capacity(context);
            assert_eq!(read(context, usize::MAX, &mut out), 0);
            assert_eq!(write(context, oracle.len(), sentinel), 0);
            assert_eq!(swap_remove(context, oracle.len(), &mut out), 0);
            assert_eq!(out, sentinel);
            for index in (0..oracle.len()).step_by(3) {
                assert_eq!(write(context, index, sentinel), 1);
                oracle[index] = sentinel;
            }
            while !oracle.is_empty() {
                let position = oracle.len() / 3;
                assert_eq!(swap_remove(context, position, &mut out), 1);
                assert_eq!(out, oracle.swap_remove(position));
                for (i, expected) in oracle.iter().enumerate() {
                    assert_eq!(read(context, i, &mut out), 1);
                    assert_eq!(out, *expected);
                }
                assert_eq!(capacity(context), retained);
            }
            clear(context);
            assert_eq!(capacity(context), retained);
            let owner = context;
            push(&mut context, sentinel);
            assert_eq!(context, owner);
            assert_eq!(capacity(context), retained);
            destroy(context);
        }
    }
    #[test]
    fn moved_slots_and_independent_owners_keep_capacity_after_clear() {
        unsafe {
            let mut first = core::ptr::null_mut();
            let mut second = core::ptr::null_mut();
            push(
                &mut first,
                RowV1 {
                    identity: 0,
                    index: usize::MAX,
                },
            );
            push(
                &mut second,
                RowV1 {
                    identity: u64::MAX,
                    index: 0,
                },
            );
            let mut moved = vec![first].pop().unwrap();
            destroy(second);
            let retained = capacity(moved);
            clear(moved);
            push(
                &mut moved,
                RowV1 {
                    identity: 2,
                    index: 3,
                },
            );
            assert_eq!(capacity(moved), retained);
            let mut row = RowV1::default();
            assert_eq!(read(moved, 0, &mut row), 1);
            assert_eq!(
                row,
                RowV1 {
                    identity: 2,
                    index: 3
                }
            );
            destroy(moved);
        }
    }
}
