//! Native registry operations used by completed upper consumers.
use super::{NativeEntry, Oop, Operand, Outcome, State, id_dispatch, native};
use core::ffi::c_void;

pub(super) unsafe fn install(state: &State, context: *mut c_void, add: Oop) {
    for (name, method) in [
        (b"symbolNamed:".as_slice(), symbol_named as native::Method),
        (b"symbolCount".as_slice(), symbol_count as native::Method),
        (b"symbolAt:".as_slice(), symbol_at as native::Method),
        (
            b"symbolNameBytes".as_slice(),
            symbol_name_bytes as native::Method,
        ),
        (b"behaviorEpoch".as_slice(), epoch as native::Method),
        (b"isBehavior:".as_slice(), is_behavior as native::Method),
        (b"behaviorView".as_slice(), behaviors as native::Method),
        (b"selectorsIn:".as_slice(), selectors as native::Method),
        (b"bindingOf:in:".as_slice(), binding as native::Method),
        (b"lookup:in:".as_slice(), lookup as native::Method),
        (b"bindingsIn:do:".as_slice(), bindings as native::Method),
        (b"addMethod:to:as:".as_slice(), add_method as native::Method),
        (
            b"removeMethod:from:".as_slice(),
            remove_method as native::Method,
        ),
        (
            b"addWeakBinding:to:as:".as_slice(),
            add_weak as native::Method,
        ),
        (
            b"newBehaviorWithParent:".as_slice(),
            new_behavior as native::Method,
        ),
        (b"bind:to:".as_slice(), bind as native::Method),
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
fn done(value: Operand) -> Outcome {
    Outcome::loaded(value)
}
fn object(value: Option<Oop>) -> Outcome {
    done(value.map(Operand::object).unwrap_or_else(Operand::unit))
}
fn entry(value: Option<NativeEntry>) -> Outcome {
    done(value.map(Operand::entry).unwrap_or_else(Operand::unit))
}
macro_rules! arguments {
    ($args:ident, $count:ident, [$($name:ident),*]) => {
        let Some([$($name),*]) = (unsafe { crate::abi::borrowed_slice($args, $count) }) else { return invalid(); };
    };
}
macro_rules! objects {
    ($($name:ident),*) => { $(let Some($name) = $name.as_object() else { return invalid(); };)* };
}
unsafe extern "C" fn symbol_named(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    arguments!(args, count, [name]);
    let Some(bytes) = name.as_bytes() else {
        return invalid();
    };
    let Some(bytes) = (unsafe { crate::abi::borrowed_bytes(bytes.pointer, bytes.length) }) else {
        return invalid();
    };
    let state = unsafe { &*context.cast::<State>() };
    object(unsafe { (&*state.symbols).existing(bytes) })
}
unsafe extern "C" fn symbol_count(
    context: *mut c_void,
    _: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    if count != 0 {
        return invalid();
    }
    done(Operand::word(unsafe {
        (&*(*context.cast::<State>()).symbols).count()
    }))
}
unsafe extern "C" fn symbol_name_bytes(
    context: *mut c_void,
    _: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    if count != 0 {
        return invalid();
    }
    done(Operand::word(unsafe {
        (&*(*context.cast::<State>()).symbols).owned_name_bytes()
    }))
}
unsafe extern "C" fn symbol_at(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    arguments!(args, count, [index]);
    let Some(index) = index.as_word() else {
        return invalid();
    };
    let value = unsafe { (&*(*context.cast::<State>()).symbols).at(index) };
    object((!value.is_null()).then_some(value))
}
unsafe extern "C" fn epoch(
    context: *mut c_void,
    _: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    if count != 0 {
        return invalid();
    }
    done(Operand::word(unsafe {
        (&*(*context.cast::<State>()).dictionaries).mutation_epoch() as usize
    }))
}
unsafe extern "C" fn is_behavior(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    arguments!(args, count, [value]);
    objects!(value);
    done(Operand::word(usize::from(unsafe {
        (&*(*context.cast::<State>()).dictionaries).is_registered(value)
    })))
}
fn view(view: birnam_abi::vtable_enumeration::ViewV1) -> Outcome {
    done(Operand::bytes(
        unsafe {
            crate::abi::borrowed_slice(view.entries.cast::<u8>(), view.count * size_of::<Oop>())
        }
        .unwrap(),
    ))
}
unsafe extern "C" fn behaviors(
    context: *mut c_void,
    _: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    if count != 0 {
        return invalid();
    }
    view(unsafe { (&*(*context.cast::<State>()).dictionaries).view() })
}
unsafe extern "C" fn selectors(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    arguments!(args, count, [behavior]);
    objects!(behavior);
    let dictionaries = unsafe { &*(*context.cast::<State>()).dictionaries };
    if !dictionaries.is_registered(behavior) {
        return invalid();
    }
    view(dictionaries.selector_view(behavior))
}
unsafe extern "C" fn binding(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    arguments!(args, count, [key, behavior]);
    objects!(key, behavior);
    let dictionaries = unsafe { &*(*context.cast::<State>()).dictionaries };
    if !dictionaries.is_registered(behavior) {
        return invalid();
    }
    let record = dictionaries.record(behavior);
    done(
        record
            .key_index(key)
            .map(|index| record.values[index])
            .unwrap_or_else(Operand::unit),
    )
}
unsafe extern "C" fn lookup(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    arguments!(args, count, [key, behavior]);
    objects!(key, behavior);
    let dictionaries = unsafe { &*(*context.cast::<State>()).dictionaries };
    if !behavior.is_null() && !dictionaries.is_registered(behavior) {
        return invalid();
    }
    entry(dictionaries.lookup(behavior, key))
}
// A Word address keeps these policy-free mutations outside ordinary language
// sends. Resolve it through Id to preserve the original allocation pointer.
fn behavior_address(
    dictionaries: &crate::vtable::Vtables,
    argument: &Operand,
    allow_null: bool,
) -> Option<Oop> {
    let address = argument.as_word()?;
    if address == 0 && allow_null {
        return Some(core::ptr::null_mut());
    }
    if !dictionaries.is_registered_address(address) {
        return None;
    }
    Some(
        dictionaries
            .record(core::ptr::without_provenance_mut(address))
            .raw
            .as_ptr()
            .cast(),
    )
}
unsafe extern "C" fn add_method(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    arguments!(args, count, [key, behavior, method]);
    objects!(key);
    let Some(method) = method.as_entry().filter(|entry| entry.method.is_some()) else {
        return invalid();
    };
    let dictionaries = unsafe { &mut *(*context.cast::<State>()).dictionaries };
    let Some(behavior) = behavior_address(dictionaries, behavior, false) else {
        return invalid();
    };
    entry(match dictionaries.add_method(behavior, key, method) {
        crate::vtable::AddMethod::Added => None,
        crate::vtable::AddMethod::Overwrote(entry) => Some(entry),
    })
}
unsafe extern "C" fn remove_method(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    arguments!(args, count, [key, behavior]);
    objects!(key);
    let dictionaries = unsafe { &mut *(*context.cast::<State>()).dictionaries };
    let Some(behavior) = behavior_address(dictionaries, behavior, false) else {
        return invalid();
    };
    entry(dictionaries.remove_method(behavior, key))
}
unsafe extern "C" fn add_weak(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    arguments!(args, count, [key, behavior, value]);
    objects!(key, value);
    let dictionaries = unsafe { &mut *(*context.cast::<State>()).dictionaries };
    let Some(behavior) = behavior_address(dictionaries, behavior, false) else {
        return invalid();
    };
    dictionaries.add_weak_opaque_binding(behavior, key, value);
    done(Operand::unit())
}
unsafe extern "C" fn new_behavior(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    arguments!(args, count, [parent]);
    let state = unsafe { &*context.cast::<State>() };
    let dictionaries = unsafe { &mut *state.dictionaries };
    let Some(parent) = behavior_address(dictionaries, parent, true) else {
        return invalid();
    };
    object(Some(dictionaries.create(
        unsafe { &mut *state.memory },
        parent,
        state.protocol.receiver,
    )))
}
unsafe extern "C" fn bind(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    arguments!(args, count, [selector, receiver]);
    objects!(selector, receiver);
    let state = unsafe { &*context.cast::<State>() };
    unsafe {
        match id_dispatch::bind(state.dispatch_context(), receiver, selector) {
            Ok(answer) => entry(answer),
            Err(failure) => failure,
        }
    }
}
unsafe extern "C" fn bindings(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    arguments!(args, count, [behavior, visitor]);
    objects!(behavior);
    let Some(visitor) = visitor.as_entry().filter(|entry| entry.method.is_some()) else {
        return invalid();
    };
    let rows: Vec<_> = {
        let dictionaries = unsafe { &*(*context.cast::<State>()).dictionaries };
        if !dictionaries.is_registered(behavior) {
            return invalid();
        }
        let record = dictionaries.record(behavior);
        record
            .keys
            .iter()
            .copied()
            .zip(record.values.iter().copied())
            .zip(record.weak_keys.iter().copied())
            .map(|((key, value), weak)| {
                [
                    Operand::object(key),
                    value,
                    Operand::word(usize::from(weak)),
                ]
            })
            .collect()
    };
    for row in rows {
        let answer =
            unsafe { visitor.method.unwrap()(visitor.context, behavior, row.as_ptr(), row.len()) };
        if answer.status != native::LOADED {
            return answer;
        }
    }
    done(Operand::unit())
}
