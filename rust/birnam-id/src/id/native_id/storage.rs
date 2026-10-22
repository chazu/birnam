//! Id storage facts. Visitors receive copied facts, never registry borrows.
use super::{NativeEntry, Oop, Operand, Outcome, State, id_dispatch, native};
use core::ffi::c_void;

pub(super) unsafe fn install(state: &State, context: *mut c_void, add: Oop) {
    for (name, method) in [
        (b"rootsDo:".as_slice(), roots as native::Method),
        (b"allocationsDo:".as_slice(), allocations as native::Method),
        (b"referencesOf:do:".as_slice(), references as native::Method),
        (
            b"retireIndexesFor:".as_slice(),
            retire_indexes as native::Method,
        ),
        (b"detachAllocationAt:".as_slice(), detach as native::Method),
        (
            b"releaseDetachedAllocation:".as_slice(),
            release as native::Method,
        ),
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

unsafe extern "C" fn retire_indexes(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([argument]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(bytes) = argument.as_bytes() else {
        return invalid();
    };
    let Some(bytes) = (unsafe { crate::abi::borrowed_bytes(bytes.pointer, bytes.length) }) else {
        return invalid();
    };
    if bytes.len() % size_of::<usize>() != 0 {
        return invalid();
    }
    // All three indexes retire against the same address set before any storage
    // can be reused. Do not rebuild this potentially large set for each index.
    let dead: crate::raw::AddressMap<()> = bytes
        .chunks_exact(size_of::<usize>())
        .map(|word| (usize::from_ne_bytes(word.try_into().unwrap()), ()))
        .collect();
    let state = unsafe { &*context.cast::<State>() };
    let dictionaries = unsafe { &mut *state.dictionaries };
    let behaviors: Vec<_> = dictionaries
        .ordered
        .iter()
        .copied()
        .filter(|object| dead.contains_key(&object.addr()))
        .collect();
    dictionaries.remove_objects(&behaviors);
    dictionaries.remove_weak_keys(&dead);
    unsafe { &mut *state.symbols }.remove_objects(&dead);
    Outcome::loaded(Operand::unit())
}

unsafe extern "C" fn detach(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([argument]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(address) = argument.as_word() else {
        return invalid();
    };
    let state = unsafe { &*context.cast::<State>() };
    let Some(allocation) = (unsafe { &mut *state.memory }).detach(address) else {
        return Outcome::loaded(Operand::unit());
    };
    // Id retains detached storage privately until the explicit release send.
    // Allocation identities never repeat, so stale handles cannot release reuse.
    let identity = allocation.identity() as usize;
    state.detached.borrow_mut().insert(identity, allocation);
    Outcome::loaded(Operand::word(identity))
}

unsafe extern "C" fn release(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([handle]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(handle) = handle.as_word().filter(|h| *h != 0) else {
        return invalid();
    };
    let state = unsafe { &*context.cast::<State>() };
    let Some(allocation) = state.detached.borrow_mut().remove(&handle) else {
        return invalid();
    };
    drop(allocation);
    Outcome::loaded(Operand::unit())
}

fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn visitor(argument: Operand) -> Option<Oop> {
    argument.as_object().filter(|o| !o.is_null())
}
unsafe fn visit(state: &State, visitor: Oop, selector: Oop, arguments: &[Operand]) -> Outcome {
    unsafe { state.protocol.send(visitor, selector, arguments) }.map_or_else(|e| e, Outcome::loaded)
}

unsafe extern "C" fn roots(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([argument]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(visitor) = visitor(*argument) else {
        return invalid();
    };
    let state = unsafe { &*context.cast::<State>() };
    let roots = state.roots();
    for object in roots {
        let answer = unsafe { visit(state, visitor, state.visits[0], &[Operand::object(object)]) };
        if answer.status != native::LOADED {
            return answer;
        }
    }
    Outcome::loaded(Operand::unit())
}

unsafe extern "C" fn allocations(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([argument]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(visitor) = visitor(*argument) else {
        return invalid();
    };
    let state = unsafe { &*context.cast::<State>() };
    let allocations: Vec<_> = unsafe { &*state.memory }
        .iter()
        .map(|(_, allocation)| {
            (allocation.payload().as_ptr().cast(),
             allocation.requested_size(), allocation.identity() as usize)
        })
        .collect();
    // Reentry can allocate or query. The caller keeps these snapshot objects
    // live; later allocations are not retroactively included in this visit.
    for (object, size, identity) in allocations {
        let answer = unsafe { visit(state, visitor, state.visits[1], &[
            Operand::object(object), Operand::word(size), Operand::word(identity),
        ]) };
        if answer.status != native::LOADED {
            return answer;
        }
    }
    Outcome::loaded(Operand::unit())
}

unsafe extern "C" fn references(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([object, argument]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else {
        return invalid();
    };
    let (Some(object), Some(visitor)) = (object.as_object(), visitor(*argument)) else {
        return invalid();
    };
    let state = unsafe { &*context.cast::<State>() };
    let Some(behavior) = (unsafe { &*state.memory }).vtable_of(object) else {
        return Outcome::loaded(Operand::unit());
    };
    // Header first, then a behavior's parent and strong keys in dictionary order.
    // Native entry contexts and upper payload/owner records are not Id edges.
    let mut edges = vec![behavior];
    let kind = if unsafe { &*state.dictionaries }.is_registered(object) {
        // Keep a parent position even for a parentless behavior. This lets the
        // upper owner insert its own reference after parent and before keys.
        if unsafe { &*state.dictionaries }.parent_of(object).is_null() {
            edges.push(core::ptr::null_mut());
        }
        edges.extend(unsafe { &*state.dictionaries }.strong_edges(object));
        1
    } else if unsafe { &*state.symbols }.bytes_of(object).is_some() {
        2
    } else {
        0
    };
    for edge in edges {
        let answer = unsafe { visit(state, visitor, state.visits[0], &[Operand::object(edge)]) };
        if answer.status != native::LOADED {
            return answer;
        }
    }
    Outcome::loaded(Operand::word(kind))
}
