//! Interned lookup policy; the Symbol interner remains an Id primitive.
use birnam_abi::Oop;
use birnam_abi::namespace::{FindV1, InternV1};
use core::ffi::{c_int, c_void};

pub(crate) unsafe extern "C" fn lookup(
    namespace: *const c_void,
    find: FindV1,
    interner: *mut c_void,
    intern: InternV1,
    bytes: *const u8,
    length: usize,
    result: *mut Oop,
) -> c_int {
    // Even absent names must be interned. A rejected name must never reach find.
    let symbol = unsafe { intern(interner, bytes, length) };
    if symbol.is_null() {
        return 0;
    }
    unsafe { *result = find(namespace, symbol) };
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ptr;

    struct Probe {
        events: Vec<&'static str>,
        bytes: *const u8,
        length: usize,
        symbol: Oop,
        value: Oop,
    }
    unsafe extern "C" fn intern(context: *mut c_void, bytes: *const u8, length: usize) -> Oop {
        let probe = unsafe { &mut *context.cast::<Probe>() };
        probe.events.push("intern");
        assert_eq!(bytes, probe.bytes);
        assert_eq!(length, probe.length);
        probe.symbol
    }
    unsafe extern "C" fn find(context: *const c_void, symbol: Oop) -> Oop {
        let probe = unsafe { &mut *context.cast_mut().cast::<Probe>() };
        probe.events.push("find");
        assert_eq!(symbol, probe.symbol);
        probe.value
    }
    #[test]
    fn forwards_borrowed_bytes_and_distinguishes_rejection_from_missing() {
        let bytes = b"\xff\0unscanned";
        let mut probe = Probe {
            events: Vec::new(),
            bytes: bytes.as_ptr(),
            length: bytes.len(),
            symbol: ptr::null_mut(),
            value: ptr::dangling_mut(),
        };
        let context = ptr::from_mut(&mut probe).cast();
        let sentinel = 7usize as Oop;
        let mut result = sentinel;
        assert_eq!(
            unsafe {
                lookup(
                    context,
                    find,
                    context,
                    intern,
                    bytes.as_ptr(),
                    bytes.len(),
                    &mut result,
                )
            },
            0
        );
        assert_eq!(result, sentinel);
        assert_eq!(probe.events, ["intern"]);

        for value in [ptr::null_mut(), sentinel] {
            probe.events.clear();
            probe.symbol = ptr::dangling_mut();
            probe.value = value;
            assert_eq!(
                unsafe {
                    lookup(
                        context,
                        find,
                        context,
                        intern,
                        bytes.as_ptr(),
                        bytes.len(),
                        &mut result,
                    )
                },
                1
            );
            assert_eq!(result, value);
            assert_eq!(probe.events, ["intern", "find"]);
        }
    }
    #[test]
    fn independent_storage_and_interner_contexts_are_not_retained() {
        let mut first = Probe {
            events: Vec::new(),
            bytes: ptr::null(),
            length: 0,
            symbol: ptr::dangling_mut(),
            value: 2usize as Oop,
        };
        let mut second = Probe {
            events: Vec::new(),
            value: 3usize as Oop,
            ..first
        };
        let mut result = ptr::null_mut();
        let mut run = |storage: &mut Probe, interner: &mut Probe| {
            let expected = storage.value;
            assert_eq!(
                unsafe {
                    lookup(
                        ptr::from_mut(storage).cast(),
                        find,
                        ptr::from_mut(interner).cast(),
                        intern,
                        ptr::null(),
                        0,
                        &mut result,
                    )
                },
                1
            );
            assert_eq!(result, expected);
        };
        run(&mut first, &mut second);
        run(&mut second, &mut first);
        assert_eq!(first.events, ["find", "intern"]);
        assert_eq!(second.events, ["intern", "find"]);
    }
}
