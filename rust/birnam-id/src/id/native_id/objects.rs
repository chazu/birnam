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
        (
            b"usesDefaultAllocation".as_slice(),
            default_allocation as native::Method,
        ),
        (b"delegated".as_slice(), delegated as native::Method),
        (b"delegatedWith:as:".as_slice(), delegated_with as native::Method),
        (b"newBehaviorWithParent:method:as:".as_slice(), behavior_with_method as native::Method),
        (
            b"objectPrototype".as_slice(),
            object_prototype as native::Method,
        ),
        (b"behaviorOf:".as_slice(), behavior_of as native::Method),
        (b"tryRepresentationIdentityOf:behavior:bytes:".as_slice(), representation_identity as native::Method),
        (b"tryRepresentationsOf:behavior:bytes:", representations as native::Method),
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

// Construction, not mutation of an already observable behavior. The initial
// entry is ready before the new identity escapes, and no callback is made while
// it is being initialized. This is generic Id dictionary policy, not Block policy.
unsafe extern "C" fn delegated_with(context: *mut c_void, parent: Oop, args: *const Operand, count: usize) -> Outcome {
    let Some([entry, selector]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else { return invalid() };
    let Some(entry) = entry.as_entry().filter(|e| e.method.is_some()) else { return invalid() };
    let Some(selector) = selector.as_object().filter(|o| !o.is_null()) else { return invalid() };
    let state = unsafe { &*context.cast::<State>() };
    let dictionaries = unsafe { &mut *state.dictionaries };
    if !dictionaries.is_registered(parent) { return invalid() }
    let object = dictionaries.create_with_method(unsafe { &mut *state.memory }, parent, state.protocol.receiver, selector, entry);
    Outcome::loaded(Operand::object(object))
}

// Raw storage constructor used by GC's ordinary allocation-policy wrapper.
unsafe extern "C" fn behavior_with_method(context: *mut c_void, _: Oop, args: *const Operand, count: usize) -> Outcome {
    let Some([parent, entry, selector]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else { return invalid() };
    let Some(parent) = parent.as_object().filter(|o| !o.is_null()) else { return invalid() };
    let arguments = [*entry, *selector];
    unsafe { delegated_with(context, parent, arguments.as_ptr(), arguments.len()) }
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
// One atomic owner query for the exact behavior, extent and identity of a
// live allocation. Unit declines when any omitted getter or this query has a
// replaced meaning. Callers then perform their original ordered getter sends.
unsafe extern "C" fn representation_identity(
    c: *mut c_void, r: Oop, args: *const Operand, count: usize,
) -> Outcome {
    let Some([value, behavior, bytes]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else { return invalid(); };
    let (Some(value), Some(behavior), Some(bytes)) = (value.as_object(), behavior.as_object(), bytes.as_word()) else { return invalid(); };
    let s = unsafe { &*c.cast::<State>() };
    let memory = unsafe { &*s.memory };
    if !representation_queries_current(s, c, r) { return Outcome::loaded(Operand::unit()); }
    let Some(allocation) = memory.get(value.addr()).filter(|a|
        a.requested_size() == bytes && memory.vtable_of(value) == Some(behavior))
    else { return Outcome::loaded(Operand::word(0)); };
    let identity = allocation.identity() as usize;
    // Zero is the mismatch carrier. An unrepresentable identity declines rather
    // than changing a legacy numeric read that did not query identity at all.
    Outcome::loaded(if identity == 0 { Operand::unit() } else { Operand::word(identity) })
}

fn representation_queries_current(s: &State, c: *mut c_void, r: Oop) -> bool {
    let memory = unsafe { &*s.memory };
    let dictionaries = unsafe { &*s.dictionaries };
    let standard = NativeEntry::new(id_dispatch::vtable_lookup_method, s.dictionaries.cast());
    r == s.protocol.receiver && memory.vtable_of(r) == Some(r)
        && dictionaries.lookup(r, s.lookup) == Some(standard)
        && [
            (b"tryRepresentationIdentityOf:behavior:bytes:".as_slice(), representation_identity as native::Method),
            (b"behaviorOf:", behavior_of),
            (b"allocationSizeOf:", allocation_size),
            (b"identityOf:", identity),
        ].into_iter().all(|(name, method)| {
            unsafe { (&*s.symbols).existing(name) }.is_some_and(|selector|
                dictionaries.lookup(r, selector) == Some(NativeEntry::new(method, c)))
        })

}

// Batch only callback-free allocation facts. Values are native ABI Oop words,
// not portable bytes or permission to inspect any upper owner's payload.
unsafe extern "C" fn representations(c: *mut c_void, r: Oop, args: *const Operand, count: usize) -> Outcome {
    let Some([values, behavior, size]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else { return invalid(); };
    let (Some(values), Some(behavior), Some(size)) = (values.as_bytes(), behavior.as_object(), size.as_word()) else { return invalid(); };
    let Some(values) = (unsafe { crate::abi::borrowed_bytes(values.pointer, values.length) }) else { return invalid(); };
    if values.len() % size_of::<usize>() != 0 || !(2..=32).contains(&(values.len()/size_of::<usize>())) { return invalid(); }
    let s = unsafe { &*c.cast::<State>() };
    let own = unsafe { (&*s.symbols).existing(b"tryRepresentationsOf:behavior:bytes:") }
        .is_some_and(|sel| unsafe { (&*s.dictionaries).lookup(r, sel) } == Some(NativeEntry::new(representations, c)));
    if !own || !representation_queries_current(s, c, r) { return Outcome::loaded(Operand::unit()); }
    let memory = unsafe { &*s.memory };
    for value in values.chunks_exact(size_of::<usize>()) {
        let address = usize::from_ne_bytes(value.try_into().unwrap());
        let object = core::ptr::with_exposed_provenance_mut(address);
        let Some(allocation) = memory.get(address).filter(|a|
            a.requested_size() == size && memory.vtable_of(object) == Some(behavior))
        else { return Outcome::loaded(Operand::word(0)); };
        if allocation.identity() as usize == 0 { return Outcome::loaded(Operand::unit()); }
    }
    Outcome::loaded(Operand::word(1))
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

// A semantic guard for omission of storage construction, not an allocation API.
// Inspect only Id-owned dictionaries; never retain a caller's implementation.
unsafe extern "C" fn default_allocation(
    c: *mut c_void,
    _: Oop,
    _: *const Operand,
    n: usize,
) -> Outcome {
    if n != 0 {
        return invalid();
    }
    let s = unsafe { &*c.cast::<State>() };
    let dictionaries = unsafe { &*s.dictionaries };
    let standard_lookup = dictionaries.lookup(s.protocol.receiver, s.lookup)
        == Some(NativeEntry::new(
            id_dispatch::vtable_lookup_method,
            s.dictionaries.cast(),
        ));
    let own = [
        (
            b"allocate:bytes:".as_slice(),
            allocate_explicit as native::Method,
        ),
        (b"allocationSizeOf:", allocation_size),
        (b"allocationChargeFor:", allocation_charge),
        (b"behaviorOf:", behavior_of),
        (b"identityOf:", identity),
        (b"tryRepresentationIdentityOf:behavior:bytes:", representation_identity),
        (b"tryRepresentationsOf:behavior:bytes:", representations),
    ]
    .into_iter()
    .all(|(name, method)| {
        unsafe { (&*s.symbols).existing(name) }.is_some_and(|selector| {
            dictionaries.lookup(s.protocol.receiver, selector) == Some(NativeEntry::new(method, c))
        })
    });
    Outcome::loaded(Operand::word(usize::from(
        standard_lookup && own && super::registries::allocation_queries_current(s, c),
    )))
}
