//! Id's own object construction and facts, available before upper modules.
use super::{NativeEntry, Oop, Operand, Outcome, State, id_dispatch, native};
use core::ffi::c_void;

pub(super) unsafe fn install(state: &State, context: *mut c_void, add: Oop) {
    for (name, method) in [
        (b"allocate".as_slice(), allocate as native::Method),
        (
            b"allocate:bytes:".as_slice(),
            allocate_explicit as native::Method,
        ),
        (b"allocationAt:".as_slice(), allocation_at as native::Method),
        (b"allocationCount".as_slice(), allocation_count as native::Method),
        (b"delegated".as_slice(), delegated as native::Method),
        (
            b"objectPrototype".as_slice(),
            object_prototype as native::Method,
        ),
        (b"behaviorOf:".as_slice(), behavior_of as native::Method),
        (b"reassociateBehaviorOf:with:".as_slice(), reassociate as native::Method),
        (b"behaviorCount".as_slice(), behavior_count as native::Method),
        (b"behaviorAt:".as_slice(), behavior_at as native::Method),
        (b"selectorAt:in:".as_slice(), selector_at as native::Method),
        (b"parentOf:".as_slice(), parent_of as native::Method),
        (b"parentOf:put:".as_slice(), parent_put as native::Method),
        (
            b"allocationSizeOf:".as_slice(),
            allocation_size as native::Method,
        ),
        (
            b"allocationChargeFor:".as_slice(),
            allocation_charge as native::Method,
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
                    state.dispatch_context(),
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


fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
unsafe extern "C" fn allocation_at(context: *mut c_void, _: Oop, args: *const Operand, count: usize) -> Outcome {
    let Some([address]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else { return invalid(); };
    let Some(address) = address.as_word() else { return invalid(); };
    let state = unsafe { &*context.cast::<State>() };
    let allocation = unsafe { (&*state.memory).get(address) };
    Outcome::loaded(allocation.map(|entry| Operand::object(entry.payload().as_ptr().cast()))
        .unwrap_or_else(Operand::unit))
}
unsafe extern "C" fn allocation_count(context: *mut c_void, _: Oop, _: *const Operand, count: usize) -> Outcome {
    if count != 0 { return invalid(); }
    let state = unsafe { &*context.cast::<State>() };
    Outcome::loaded(Operand::word(unsafe { (&*state.memory).len() }))
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
    let Some(size) = size.as_word().filter(|size| super::ObjectMemory::valid_payload_size(*size)) else {
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

// The non-collecting storage operation has an explicit behavior argument.
// Upper construction sends it after applying its own policy; no entry is cached.
unsafe extern "C" fn allocate_explicit(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([behavior, size]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(behavior) = behavior.as_object() else {
        return invalid();
    };
    unsafe { allocate(context, behavior, size, 1) }
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
pub(crate) unsafe extern "C" fn behavior_of(
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
pub(crate) unsafe extern "C" fn allocation_size(
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
/// Storage geometry stays with Id. This is a non-allocating size query, not an
/// allocation reservation or an upper collector-policy operation.
unsafe extern "C" fn allocation_charge(
    _: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([size]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(size) = size.as_word() else {
        return invalid();
    };
    let charge = super::ObjectMemory::valid_payload_size(size)
        .then(|| super::ObjectMemory::allocation_total_size(size).unwrap());
    Outcome::loaded(charge.map_or_else(Operand::unit, Operand::word))
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
pub(crate) unsafe extern "C" fn symbol_bytes(
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

unsafe extern "C" fn reassociate(context: *mut c_void, _: Oop, args: *const Operand, count: usize) -> Outcome {
    let Some([object, behavior]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else { return invalid(); };
    let (Some(object), Some(behavior)) = (object.as_object(), behavior.as_object()) else { return invalid(); };
    let state = unsafe { &*context.cast::<State>() };
    let memory = unsafe { &mut *state.memory };
    if memory.get(behavior.addr()).is_none() || !memory.set_vtable(object, behavior) { return invalid(); }
    unsafe { (&mut *state.dictionaries).note_mutation() };
    Outcome::loaded(Operand::object(object))
}
unsafe extern "C" fn behavior_count(context: *mut c_void, _: Oop, _: *const Operand, count: usize) -> Outcome {
    if count != 0 { return invalid(); }
    let state = unsafe { &*context.cast::<State>() };
    Outcome::loaded(Operand::word(unsafe { (&*state.dictionaries).count() }))
}
unsafe extern "C" fn behavior_at(context: *mut c_void, _: Oop, args: *const Operand, count: usize) -> Outcome {
    let Some([index]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else { return invalid(); };
    let Some(index) = index.as_word() else { return invalid(); };
    let state = unsafe { &*context.cast::<State>() };
    let object = unsafe { (&*state.dictionaries).at(index) };
    Outcome::loaded(if object.is_null() { Operand::unit() } else { Operand::object(object) })
}
unsafe extern "C" fn selector_at(context: *mut c_void, _: Oop, args: *const Operand, count: usize) -> Outcome {
    let Some([index, behavior]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else { return invalid(); };
    let (Some(index), Some(behavior)) = (index.as_word(), behavior.as_object()) else { return invalid(); };
    let state = unsafe { &*context.cast::<State>() };
    let dictionaries = unsafe { &*state.dictionaries };
    if !dictionaries.is_registered(behavior) { return invalid(); }
    let selector = dictionaries.selector_at(behavior, index);
    Outcome::loaded(if selector.is_null() { Operand::unit() } else { Operand::object(selector) })
}
unsafe extern "C" fn parent_of(context: *mut c_void, _: Oop, args: *const Operand, count: usize) -> Outcome {
    let Some(behavior) = (unsafe { candidate(args, count) }) else { return invalid(); };
    let state = unsafe { &*context.cast::<State>() };
    let parent = unsafe { (&*state.dictionaries).parent_of(behavior) };
    Outcome::loaded(if parent.is_null() { Operand::unit() } else { Operand::object(parent) })
}
unsafe extern "C" fn parent_put(context: *mut c_void, _: Oop, args: *const Operand, count: usize) -> Outcome {
    let Some([behavior, parent]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else { return invalid(); };
    let Some(behavior) = behavior.as_object() else { return invalid(); };
    let parent = if parent.kind() == native::UNIT { core::ptr::null_mut() }
        else { let Some(parent) = parent.as_object() else { return invalid(); }; parent };
    let state = unsafe { &*context.cast::<State>() };
    let dictionaries = unsafe { &mut *state.dictionaries };
    if !dictionaries.is_registered(behavior) || (!parent.is_null() && !dictionaries.is_registered(parent)) { return invalid(); }
    dictionaries.set_parent(behavior, parent);
    Outcome::loaded(Operand::object(behavior))
}
