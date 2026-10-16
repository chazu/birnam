//! Native registry operations used by completed upper consumers.
use super::{NativeEntry, Oop, Operand, Outcome, State, id_dispatch, native};
use core::ffi::c_void;

pub(super) unsafe fn install(state: &State, context: *mut c_void, add: Oop) {
    for (name, method) in [
        (b"symbolNamed:".as_slice(), symbol_named as native::Method),
        (
            b"internBytes:".as_slice(),
            super::intern_bytes as native::Method,
        ),
        (b"symbolCount".as_slice(), symbol_count as native::Method),
        (b"symbolAt:".as_slice(), symbol_at as native::Method),
        (
            b"symbolNameBytes".as_slice(),
            symbol_name_bytes as native::Method,
        ),
        (b"behaviorEpoch".as_slice(), epoch as native::Method),
        (b"bindingEpoch".as_slice(), binding_epoch as native::Method),
        (b"nativeBindingsMatch:".as_slice(), native_bindings_match as native::Method),
        (b"nativeBindingQuerySemantics:for:".as_slice(), binding_query_semantics as native::Method),
        (b"isBehavior:".as_slice(), is_behavior as native::Method),
        (b"behaviorView".as_slice(), behaviors as native::Method),
        (b"selectorsIn:".as_slice(), selectors as native::Method),
        (b"bindingOf:in:".as_slice(), binding as native::Method),
        (b"restoreBinding:of:in:expected:previous:atEpoch:".as_slice(), restore_binding as native::Method),
        (b"methodChangesTo:".as_slice(), method_changes_to as native::Method),
        (b"addMethod".as_slice(), dictionary_add as native::Method),
        (b"removeMethod:".as_slice(), dictionary_remove as native::Method),
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
        (
            b"usesDefaultLookup:".as_slice(),
            uses_default_lookup as native::Method,
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
unsafe extern "C" fn binding_epoch(
    context: *mut c_void, _: Oop, _: *const Operand, count: usize,
) -> Outcome {
    if count != 0 { return invalid(); }
    done(Operand::word(unsafe {
        (&*(*context.cast::<State>()).dictionaries).binding_epoch() as usize
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
fn view(view: &[Oop]) -> Outcome {
    done(Operand::bytes(
        unsafe { crate::abi::borrowed_slice(view.as_ptr().cast::<u8>(), size_of_val(view)) }
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
// A conditional inverse is one Id operation. No outgoing send occurs between
// checking the target/epoch/entry and changing the dictionary. Native callers
// retain any owned Methods represented by these entries across this operation.
unsafe extern "C" fn restore_binding(context: *mut c_void, _: Oop, args: *const Operand, count: usize) -> Outcome {
    arguments!(args, count, [key, target, behavior, expected, previous, epoch]);
    objects!(key, target, behavior);
    let Some(epoch) = epoch.as_word() else { return invalid(); };
    let decode = |v: Operand| if v.kind() == native::UNIT { Some(None) }
        else { v.as_entry().filter(|e| e.method.is_some()).map(Some) };
    let Some(expected) = decode(*expected) else { return invalid(); };
    let Some(previous) = decode(*previous) else { return invalid(); };
    let state = unsafe { &*context.cast::<State>() };
    let tables = unsafe { &mut *state.dictionaries };
    if !tables.is_registered(behavior) { return invalid(); }
    let actual = if tables.is_registered(target) { Some(target) }
        else { unsafe { (&*state.memory).vtable_of(target) } };
    if actual != Some(behavior) || tables.binding_epoch() as usize != epoch || epoch == usize::MAX {
        return done(Operand::unit());
    }
    let record = tables.record(behavior);
    let current = record.key_index(key).map(|i| record.values[i]);
    let current = match current { None => None, Some(v) => match decode(v) {
        Some(entry) => entry, None => return done(Operand::unit()),
    }};
    if current == previous { return done(Operand::word(epoch)); }
    if current != expected { return done(Operand::unit()); }
    if let Some(entry) = previous { tables.add_method(behavior, key, entry); }
    else { tables.remove_method(behavior, key); }
    let committed = tables.binding_epoch() as usize;
    // Notification is after commit. Its reentrant writes survive; a subsequent
    // conditional inverse sees an epoch different from this returned value.
    if let Err(error) = unsafe { state.method_changed(key, behavior) } { return error; }
    done(Operand::word(committed))
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
    dictionaries: &crate::id::vtable::Vtables,
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
    let state = unsafe { &*context.cast::<State>() };
    let Some(behavior) = behavior_address(unsafe { &*state.dictionaries }, behavior, false) else {
        return invalid();
    };
    let old = match unsafe { (&mut *state.dictionaries).add_method(behavior, key, method) } {
        crate::id::vtable::AddMethod::Added => None,
        crate::id::vtable::AddMethod::Overwrote(entry) => Some(entry),
    };
    if let Err(error) = unsafe { state.method_changed(key, behavior) } { return error; }
    entry(old)
}
unsafe extern "C" fn remove_method(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    arguments!(args, count, [key, behavior]);
    objects!(key);
    let state = unsafe { &*context.cast::<State>() };
    let Some(behavior) = behavior_address(unsafe { &*state.dictionaries }, behavior, false) else {
        return invalid();
    };
    let old = unsafe { (&mut *state.dictionaries).remove_method(behavior, key) };
    if old.is_some() {
        if let Err(error) = unsafe { state.method_changed(key, behavior) } { return error; }
    }
    entry(old)
}
unsafe extern "C" fn add_weak(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    arguments!(args, count, [key, behavior, value]);
    objects!(key, value);
    let state = unsafe { &*context.cast::<State>() };
    let Some(behavior) = behavior_address(unsafe { &*state.dictionaries }, behavior, false) else {
        return invalid();
    };
    unsafe { (&mut *state.dictionaries).add_weak_opaque_binding(behavior, key, value) };
    if let Err(error) = unsafe { state.method_changed(key, behavior) } { return error; }
    done(Operand::unit())
}

unsafe extern "C" fn method_changes_to(context: *mut c_void, _: Oop, args: *const Operand, count: usize) -> Outcome {
    arguments!(args, count, [observer]);
    let observer = match observer.kind() {
        native::UNIT => core::ptr::null_mut(),
        native::OBJECT => match observer.as_object().filter(|o| !o.is_null()) { Some(o) => o, None => return invalid() },
        _ => return invalid(),
    };
    let state = unsafe { &*context.cast::<State>() };
    if !observer.is_null() && unsafe { (&*state.memory).get(observer.addr()) }.is_none() { return invalid(); }
    let old = state.method_change_observer.replace(observer);
    object((!old.is_null()).then_some(old))
}
unsafe extern "C" fn dictionary_add(context: *mut c_void, receiver: Oop, args: *const Operand, count: usize) -> Outcome {
    arguments!(args, count, [key, method]);
    let result = unsafe { add_method(context, receiver, [*key, Operand::word(receiver.addr()), *method].as_ptr(), 3) };
    if result.status != native::LOADED { return result; }
    done(*method)
}
unsafe extern "C" fn dictionary_remove(context: *mut c_void, receiver: Oop, args: *const Operand, count: usize) -> Outcome {
    arguments!(args, count, [key]);
    unsafe { remove_method(context, receiver, [*key, Operand::word(receiver.addr())].as_ptr(), 2) }
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
    let Some(visitor) = visitor.as_object().filter(|o| !o.is_null()) else {
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
            unsafe { (&*context.cast::<State>()).protocol.send(visitor, (&*context.cast::<State>()).visits[2], &row) }.map_or_else(|e| e, Outcome::loaded);
        if answer.status != native::LOADED {
            return answer;
        }
    }
    done(Operand::unit())
}

// Describe only this owner's implementations. A saved entry's immutable
// semantics are distinct from whether it is currently selected for a message.
unsafe extern "C" fn binding_query_semantics(
    context: *mut c_void, _: Oop, args: *const Operand, count: usize,
) -> Outcome {
    arguments!(args, count, [argument, name]);
    let Some(entry) = argument.as_entry() else { return invalid(); };
    let Some(name) = name.as_bytes() else { return invalid(); };
    let Some(name) = (unsafe { crate::abi::borrowed_bytes(name.pointer, name.length) }) else { return invalid(); };
    let expected = match name {
        b"usesDefaultLookup:" => Some(uses_default_lookup as native::Method),
        b"bind:to:" => Some(bind as native::Method),
        b"symbolNamed:" => Some(symbol_named as native::Method),
        b"nativeBindingsMatch:" => Some(native_bindings_match as native::Method),
        _ => None,
    };
    let admitted = expected.is_some_and(|method| entry == NativeEntry::new(method, context));
    done(Operand::word(usize::from(admitted)))
}
// Concrete execution guard: Id compares its own lookup entry and context.
// One callback-free owner query. The native Word quadruples (receiver, selector, method, context) are caller-owned
// ABI data, never a portable format. Id reads only its own heap/dictionaries;
// expected NativeEntry contexts are opaque equality witnesses, never invoked.
unsafe extern "C" fn native_bindings_match(
    context: *mut c_void, _: Oop, args: *const Operand, count: usize,
) -> Outcome {
    arguments!(args,count,[request]);
    let Some(bytes)=request.as_bytes() else {return invalid();};
    let Some(bytes)=(unsafe {crate::abi::borrowed_bytes(bytes.pointer,bytes.length)}) else {return invalid();};
    let row_bytes=4*size_of::<usize>();
    if bytes.is_empty() || bytes.len()%row_bytes!=0 || bytes.len()/row_bytes>32 {return invalid();}
    let state=unsafe {&*context.cast::<State>()};
    let memory=unsafe {&*state.memory};
    let dictionaries=unsafe {&*state.dictionaries};
    if dictionaries.mutation_epoch()>=usize::MAX as u64 {return done(Operand::word(0));}
    let default=NativeEntry::new(id_dispatch::vtable_lookup_method,state.dictionaries.cast());
    for row in bytes.chunks_exact(row_bytes) {
        let words:[usize;4]=std::array::from_fn(|i|usize::from_ne_bytes(row[i*size_of::<usize>()..(i+1)*size_of::<usize>()].try_into().unwrap()));
        let receiver=core::ptr::with_exposed_provenance_mut(words[0]);
        let selector=core::ptr::with_exposed_provenance_mut(words[1]);
        if memory.get(words[0]).is_none() || memory.get(words[1]).is_none() || words[2]==0 {
            return done(Operand::word(0));
        }
        let behavior=unsafe {crate::id::object_memory::ObjectMemory::vtable_of_valid_object(receiver)};
        if !dictionaries.is_registered(behavior) {return done(Operand::word(0));}
        let meta=unsafe {crate::id::object_memory::ObjectMemory::vtable_of_valid_object(behavior)};
        // Decline custom metabehaviors before invoking any lookup callback.
        if meta!=state.protocol.receiver || dictionaries.lookup(meta,state.lookup)!=Some(default) {
            return done(Operand::word(0));
        }
        let actual=dictionaries.lookup(behavior,selector);
        if actual.and_then(|e|e.method.map(|f|(f as usize,e.context as usize)))!=Some((words[2],words[3])) {return done(Operand::word(0));}
    }
    done(Operand::word(1))
}
unsafe extern "C" fn uses_default_lookup(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    arguments!(args, count, [receiver]);
    objects!(receiver);
    let state = unsafe { &*context.cast::<State>() };
    let behavior =
        unsafe { crate::id::object_memory::ObjectMemory::vtable_of_valid_object(receiver) };
    let lookup = unsafe { id_dispatch::bind(state.dispatch_context(), behavior, state.lookup) };
    match lookup {
        Ok(entry) => done(Operand::word(usize::from(
            entry
                == Some(NativeEntry::new(
                    id_dispatch::vtable_lookup_method,
                    state.dictionaries.cast(),
                )),
        ))),
        Err(error) => error,
    }
}

pub(super) fn allocation_queries_current(s: &State, c: *mut c_void) -> bool {
    let dictionaries = unsafe { &*s.dictionaries };
    [
        (b"isBehavior:".as_slice(), is_behavior as native::Method),
        (b"behaviorEpoch", epoch),
        (b"bind:to:", bind),
        (b"usesDefaultLookup:", uses_default_lookup),
    ]
    .into_iter()
    .all(|(name, method)| {
        unsafe { (&*s.symbols).existing(name) }.is_some_and(|selector| {
            dictionaries.lookup(s.protocol.receiver, selector) == Some(NativeEntry::new(method, c))
        })
    })
}
