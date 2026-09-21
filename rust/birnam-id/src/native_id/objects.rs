//! Id's own object construction and facts, available before upper modules.
use super::{NativeEntry, Oop, Operand, Outcome, State, id_dispatch, native};
use crate::object_memory::ObjectMemory;
use core::ffi::c_void;

pub(super) unsafe fn install(state: &State, context: *mut c_void, add: Oop) {
    for (name, method) in [
        (b"allocate".as_slice(), allocate as native::Method),
        (b"delegated".as_slice(), delegated as native::Method),
        (
            b"objectPrototype".as_slice(),
            object_prototype as native::Method,
        ),
        (b"behaviorOf:".as_slice(), behavior_of as native::Method),
        (
            b"allocationSizeOf:".as_slice(),
            allocation_size as native::Method,
        ),
        (b"identityOf:".as_slice(), identity as native::Method),
        (b"symbolBytesOf:".as_slice(), symbol_bytes as native::Method),
    ] {
        let selector =
            unsafe { (&mut *state.symbols).intern(&mut *state.memory, state.symbol_vtable, name) }
                .unwrap();
        assert_eq!(
            unsafe {
                id_dispatch::send(
                    state.protocol.receiver,
                    add,
                    &[
                        Operand::object(selector),
                        Operand::entry(NativeEntry::new(method, context)),
                    ],
                )
            }
            .status,
            native::LOADED
        );
    }
}

pub(crate) fn valid_size(size: usize) -> bool {
    ObjectMemory::allocation_total_size(size).is_some_and(|total| {
        std::alloc::Layout::from_size_align(total, crate::object_memory::BLOCK_ALIGNMENT).is_ok()
    })
}
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
unsafe fn candidate(args: *const Operand, count: usize) -> Option<Oop> {
    let [argument] = (unsafe { crate::abi::borrowed_slice(args, count) })? else {
        return None;
    };
    argument.as_object()
}

pub(crate) unsafe extern "C" fn allocate(
    context: *mut c_void,
    behavior: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([size]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(size) = size.as_word().filter(|size| valid_size(*size)) else {
        return invalid();
    };
    let state = unsafe { &*context.cast::<State>() };
    if !unsafe { (&*state.dictionaries).is_registered(behavior) } {
        return invalid();
    }
    let memory = unsafe { &mut *state.memory };
    let object = memory.allocate(size);
    assert!(memory.set_vtable(object, behavior));
    Outcome::loaded(Operand::object(object))
}

pub(crate) unsafe extern "C" fn delegated(
    context: *mut c_void,
    parent: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    if count != 0 {
        return invalid();
    }
    let state = unsafe { &*context.cast::<State>() };
    let dictionaries = unsafe { &mut *state.dictionaries };
    if !dictionaries.is_registered(parent) {
        return invalid();
    }
    let object = dictionaries.create(
        unsafe { &mut *state.memory },
        parent,
        state.protocol.receiver,
    );
    Outcome::loaded(Operand::object(object))
}

unsafe extern "C" fn object_prototype(
    context: *mut c_void,
    _: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    if count != 0 {
        return invalid();
    }
    Outcome::loaded(Operand::object(unsafe {
        (*context.cast::<State>()).object_prototype
    }))
}
unsafe extern "C" fn behavior_of(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some(object) = (unsafe { candidate(args, count) }) else {
        return invalid();
    };
    let state = unsafe { &*context.cast::<State>() };
    let behavior =
        unsafe { (&*state.memory).vtable_of(object) }.filter(|behavior| !behavior.is_null());
    Outcome::loaded(behavior.map(Operand::object).unwrap_or_else(Operand::unit))
}
unsafe extern "C" fn allocation_size(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some(object) = (unsafe { candidate(args, count) }) else {
        return invalid();
    };
    let state = unsafe { &*context.cast::<State>() };
    let allocation = unsafe { (&*state.memory).get(object.addr()) };
    Outcome::loaded(
        allocation
            .map(|allocation| Operand::word(allocation.requested_size()))
            .unwrap_or_else(Operand::unit),
    )
}
unsafe extern "C" fn identity(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some(object) = (unsafe { candidate(args, count) }) else {
        return invalid();
    };
    let state = unsafe { &*context.cast::<State>() };
    let allocation = unsafe { (&*state.memory).get(object.addr()) };
    Outcome::loaded(
        allocation
            .map(|allocation| Operand::word(allocation.identity() as usize))
            .unwrap_or_else(Operand::unit),
    )
}
unsafe extern "C" fn symbol_bytes(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some(object) = (unsafe { candidate(args, count) }) else {
        return invalid();
    };
    let state = unsafe { &*context.cast::<State>() };
    // The caller retains the Symbol for the complete borrowed-byte lifetime.
    let bytes = unsafe { (&*state.symbols).bytes_of(object) };
    Outcome::loaded(bytes.map(Operand::bytes).unwrap_or_else(Operand::unit))
}
