//! Reverse lookup composes Namespace selection with the floor's Symbol borrow.
use birnam_abi::Oop;
use birnam_abi::namespace_names::{ApiV1, FloorV1};
use core::ffi::{c_char, c_void};
#[repr(C)]
struct Names {
    api: ApiV1,
    namespace: *const c_void,
    reverse: unsafe extern "C" fn(*const c_void, Oop) -> Oop,
    floor: FloorV1,
}
unsafe extern "C" fn name_of(context: *mut c_void, value: Oop) -> *const c_char {
    let names = unsafe { &*context.cast::<Names>() };
    unsafe { names.floor.preflight.unwrap()(names.floor.context) };
    let symbol = unsafe { (names.reverse)(names.namespace, value) };
    unsafe { names.floor.symbol_name.unwrap()(names.floor.context, symbol) }
}
pub(crate) unsafe extern "C" fn create(
    context: *mut c_void,
    reverse: Option<unsafe extern "C" fn(*const c_void, Oop) -> Oop>,
    floor: *const FloorV1,
) -> *mut ApiV1 {
    if context.is_null() || reverse.is_none() || floor.is_null() {
        return core::ptr::null_mut();
    }
    let floor = unsafe { *floor };
    if floor.version != 1
        || floor.byte_size != size_of::<FloorV1>()
        || floor.preflight.is_none()
        || floor.symbol_name.is_none()
    {
        return core::ptr::null_mut();
    }
    let mut names = Box::new(Names {
        api: ApiV1 {
            version: 1,
            byte_size: size_of::<ApiV1>(),
            context: core::ptr::null_mut(),
            name_of: Some(name_of),
        },
        namespace: context,
        reverse: reverse.unwrap(),
        floor,
    });
    names.api.context = core::ptr::from_mut(names.as_mut()).cast();
    Box::into_raw(names).cast()
}
pub(crate) unsafe extern "C" fn destroy(api: *mut ApiV1) {
    // The live context may have been replaced. Recover ownership from the
    // stable first field, never through that replaceable context.
    unsafe { drop(Box::from_raw(api.cast::<Names>())) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ptr;

    struct Probe {
        events: Vec<(&'static str, Oop)>,
        value: Oop,
        symbol: Oop,
        bytes: &'static core::ffi::CStr,
    }
    unsafe extern "C" fn preflight(context: *mut c_void) {
        unsafe { &mut *context.cast::<Probe>() }
            .events
            .push(("preflight", ptr::null_mut()));
    }
    unsafe extern "C" fn reverse(context: *const c_void, value: Oop) -> Oop {
        let probe = unsafe { &mut *context.cast_mut().cast::<Probe>() };
        probe.events.push(("reverse", value));
        if !value.is_null() && value == probe.value {
            probe.symbol
        } else {
            ptr::null_mut()
        }
    }
    unsafe extern "C" fn symbol_name(context: *mut c_void, symbol: Oop) -> *const c_char {
        let probe = unsafe { &mut *context.cast::<Probe>() };
        probe.events.push(("symbol", symbol));
        if symbol == probe.symbol {
            probe.bytes.as_ptr()
        } else {
            ptr::null()
        }
    }
    fn access(probe: &mut Probe) -> FloorV1 {
        FloorV1 {
            version: 1,
            byte_size: size_of::<FloorV1>(),
            context: ptr::from_mut(probe).cast(),
            preflight: Some(preflight),
            symbol_name: Some(symbol_name),
        }
    }
    fn probe(bytes: &'static core::ffi::CStr) -> Probe {
        Probe {
            events: Vec::new(),
            value: ptr::dangling_mut::<c_void>(),
            symbol: 2usize as Oop,
            bytes,
        }
    }
    #[test]
    fn rejects_incomplete_floor_without_effects() {
        let mut probe = probe(c"first");
        let floor = access(&mut probe);
        let context = floor.context;
        unsafe {
            assert!(create(ptr::null_mut(), Some(reverse), &floor).is_null());
            assert!(create(context, None, &floor).is_null());
            assert!(create(context, Some(reverse), ptr::null()).is_null());
            for bad in [
                FloorV1 {
                    version: 2,
                    ..floor
                },
                FloorV1 {
                    byte_size: floor.byte_size - 1,
                    ..floor
                },
                FloorV1 {
                    preflight: None,
                    ..floor
                },
                FloorV1 {
                    symbol_name: None,
                    ..floor
                },
            ] {
                assert!(create(context, Some(reverse), &bad).is_null());
            }
        }
        assert!(probe.events.is_empty());
    }
    #[test]
    fn selected_storage_order_borrow_and_independent_live_contexts() {
        let mut first = probe(c"first");
        let mut second = probe(c"second");
        let mut floor = access(&mut first);
        let other = access(&mut second);
        unsafe {
            let left = create(floor.context, Some(reverse), &floor);
            let right = create(other.context, Some(reverse), &other);
            assert!(!left.is_null() && !right.is_null() && left != right);
            assert!(first.events.is_empty() && second.events.is_empty());
            // Access is copied, and the selected storage callback is honored.
            floor.preflight = None;
            assert!(floor.preflight.is_none());
            let selected = *left;
            assert_eq!(
                selected.name_of.unwrap()(selected.context, first.value),
                first.bytes.as_ptr()
            );
            assert_eq!(
                first.events,
                [
                    ("preflight", ptr::null_mut()),
                    ("reverse", first.value),
                    ("symbol", first.symbol)
                ]
            );
            first.events.clear();
            assert!(selected.name_of.unwrap()(selected.context, ptr::null_mut()).is_null());
            assert_eq!(
                first.events,
                [
                    ("preflight", ptr::null_mut()),
                    ("reverse", ptr::null_mut()),
                    ("symbol", ptr::null_mut())
                ]
            );
            assert!(second.events.is_empty());
            // A live descriptor can select another service; destroying its
            // original owner must not free that substituted context.
            (*left).context = (*right).context;
            let live = *left;
            assert_eq!(
                live.name_of.unwrap()(live.context, second.value),
                second.bytes.as_ptr()
            );
            assert_eq!(second.events.len(), 3);
            destroy(left);
            let live = *right;
            assert_eq!(
                live.name_of.unwrap()(live.context, second.value),
                second.bytes.as_ptr()
            );
            destroy(right);
            assert_eq!(second.events.len(), 6);
        }
    }
}
