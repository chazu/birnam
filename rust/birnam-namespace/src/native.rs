//! Namespace takes a Roots object and returns an opaque Namespace object.
//!
//! Symbol keys: `at:`, `at:put:`, `nameOf:` and `nameAt:`; missing entries return
//! Unit. `count` and `nameAt:` use native Words until language Integers exist.
//! `bindingsDo:` sends `binding:value:` to a visitor Object; `rootsDo:` sends
//! `visit:` for each root. Both snapshot before sending.
use super::state::Environment;
use birnam_abi::id_protocol::{
    self as native, Method, Module, NativeEntry, Operand, Outcome, Protocol,
};
use birnam_abi::{Oop, borrowed_slice};
use core::{cell::RefCell, ffi::c_void, ptr};
use std::sync::atomic::{AtomicPtr, Ordering};

struct Namespace {
    id: Protocol,
    receiver: Oop,
    registry: Oop,
    storage: RefCell<Environment>,
    names: std::collections::HashMap<&'static [u8], Oop>,
}
static INSTANCE: AtomicPtr<Namespace> = AtomicPtr::new(ptr::null_mut());
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn object(value: Operand) -> Result<Oop, Outcome> {
    value
        .as_object()
        .filter(|o| !o.is_null())
        .ok_or_else(invalid)
}
fn value(object: Oop) -> Operand {
    if object.is_null() {
        Operand::unit()
    } else {
        Operand::object(object)
    }
}
unsafe fn symbol(namespace: &Namespace, key: Operand) -> Result<Oop, Outcome> {
    let object = object(key)?;
    if unsafe { namespace.id.send(namespace.id.receiver, namespace.names[b"symbolBytesOf:".as_slice()], &[key]) }?.kind() != native::BYTES {
        return Err(invalid());
    }
    Ok(object)
}
unsafe extern "C" fn find(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([key]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let namespace = unsafe { &*context.cast::<Namespace>() };
    match unsafe { symbol(namespace, *key) } {
        Ok(key) => Outcome::loaded(value(namespace.storage.borrow().import(key))),
        Err(error) => error,
    }
}
// Language-facing nullable lookup. Keep at:'s native Unit contract intact.
unsafe extern "C" fn value_at(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let namespace = &*c.cast::<Namespace>();
        let args = borrowed_slice(a, n).ok_or_else(invalid)?;
        if n != 1 { return Err(invalid()); }
        let result = namespace.id.send(r, namespace.names[b"at:".as_slice()], args)?;
        if result.kind() != native::UNIT { return Ok(result); }
        namespace.id.send(r, namespace.names[b"at:".as_slice()], &[Operand::object(namespace.names[b"nil".as_slice()])])
    })().map_or_else(|e| e, Outcome::loaded)
}
unsafe extern "C" fn includes_key(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let namespace = &*c.cast::<Namespace>();
        let [key] = borrowed_slice(a, n).ok_or_else(invalid)? else { return Err(invalid()); };
        let value = namespace.id.send(r, namespace.names[b"at:".as_slice()], &[*key])?;
        namespace.id.send(namespace.receiver, namespace.names[b"at:".as_slice()], &[Operand::object(namespace.names[if value.kind() == native::UNIT { b"false".as_slice() } else { b"true".as_slice() }])])
    })().map_or_else(|e| e, Outcome::loaded)
}
unsafe extern "C" fn bind(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([key, argument]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let object = match argument.kind() {
        native::UNIT => ptr::null_mut(),
        native::OBJECT => match object(*argument) {
            Ok(o) => o,
            Err(e) => return e,
        },
        _ => return invalid(),
    };
    let namespace = unsafe { &*context.cast::<Namespace>() };
    match unsafe { symbol(namespace, *key) } {
        Ok(key) => Outcome::loaded(value(namespace.storage.borrow_mut().export(key, object))),
        Err(error) => error,
    }
}
unsafe extern "C" fn count(
    context: *mut c_void,
    _: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    if count != 0 {
        return invalid();
    }
    Outcome::loaded(Operand::word(
        unsafe { &*context.cast::<Namespace>() }
            .storage
            .borrow()
            .count(),
    ))
}
unsafe extern "C" fn name_at(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([index]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(index) = index.as_word() else {
        return invalid();
    };
    Outcome::loaded(value(
        unsafe { &*context.cast::<Namespace>() }
            .storage
            .borrow()
            .name_at(index),
    ))
}
unsafe extern "C" fn reverse(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([argument]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(object) = argument.as_object() else {
        return invalid();
    };
    Outcome::loaded(value(
        unsafe { &*context.cast::<Namespace>() }
            .storage
            .borrow()
            .exported_name_of(object),
    ))
}
unsafe extern "C" fn roots(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([argument]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(visitor) = argument.as_object().filter(|o| !o.is_null()) else {
        return invalid();
    };
    let snapshot = {
        let namespace = unsafe { &*context.cast::<Namespace>() };
        let mut snapshot = namespace.storage.borrow().roots();
        snapshot.extend([receiver, namespace.registry]);
        snapshot.extend(namespace.names.values().copied());
        snapshot
    };
    for object in snapshot.into_iter().filter(|o| !o.is_null()) {
        let result = unsafe {
            (&*context.cast::<Namespace>()).id.send_named(visitor, b"visit:", &[Operand::object(object)])
        };
        if let Err(error) = result {
            return error;
        }
    }
    Outcome::loaded(Operand::unit())
}
unsafe extern "C" fn bindings(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([argument]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(visitor) = argument.as_object().filter(|o| !o.is_null()) else {
        return invalid();
    };
    let snapshot = unsafe { &*context.cast::<Namespace>() }
        .storage
        .borrow()
        .bindings()
        .to_vec();
    for binding in snapshot {
        let arguments = [Operand::object(binding.name), value(binding.value)];
        let result = unsafe {
            (&*context.cast::<Namespace>()).id.send_named(visitor, b"binding:value:", &arguments)
        };
        if let Err(error) = result {
            return error;
        }
    }
    Outcome::loaded(Operand::unit())
}
/// Compiler admission recognizes only this owner's canonical lookup entry.
/// A declaration about an entry does not select it or bypass future lookup.
unsafe extern "C" fn lookup_semantics(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([argument]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(entry) = argument.as_entry() else {
        return invalid();
    };
    Outcome::loaded(Operand::word(usize::from(
        entry == NativeEntry::new(find, context),
    )))
}
unsafe fn construct(id: Protocol, registry: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let mut tokens = Vec::new();
        let result = (|| {
            let prototype = id.send_named(id.receiver, b"objectPrototype", &[])?;
            let parent = object(id.send_named(id.receiver, b"behaviorOf:", &[prototype])?)?;
            let behavior = object(id.send_named(parent, b"delegated", &[])?)?;
            let receiver = object(id.send_named(behavior, b"allocate", &[Operand::word(0)])?)?;
            let retain = id.intern_name(b"retain:")?;
            tokens.push(id.send(registry, retain, &[Operand::object(receiver)])?.as_word().ok_or_else(invalid)?);
            let mut names = std::collections::HashMap::new();
            for name in [b"symbolBytesOf:".as_slice(), b"at:", b"nil", b"true", b"false"] {
                let symbol = id.intern_name(name)?;
                tokens.push(id.send(registry, retain, &[Operand::object(symbol)])?.as_word().ok_or_else(invalid)?);
                names.insert(name, symbol);
            }
            let namespace = Box::into_raw(Box::new(Namespace {
                id,
                receiver,
                registry,
                storage: RefCell::new(Environment::new()),
                names,
            }));
            for (name, method) in [
                (b"at:".as_slice(), find as Method),
                (b"valueAt:", value_at),
                (b"includesKey:", includes_key),
                (b"at:put:", bind),
                (b"count", count),
                (b"nameAt:", name_at),
                (b"nameOf:", reverse),
                (b"rootsDo:", roots),
                (b"bindingsDo:", bindings),
                (b"isCanonicalLookup:", lookup_semantics),
            ] {
                id.send_named(
                    behavior,
                    b"addMethod",
                    &[
                        Operand::object(id.intern_name(name)?),
                        Operand::entry(NativeEntry::new(method, namespace.cast())),
                    ],
                )?;
            }
            id.send_named(registry, b"register:", &[Operand::object(receiver)])?;
            for (name, object) in [(b"Namespace".as_slice(), receiver), (b"Roots", registry)] {
                id.send_named(
                    receiver,
                    b"at:put:",
                    &[
                        Operand::object(id.intern_name(name)?),
                        Operand::object(object),
                    ],
                )?;
            }
            INSTANCE.store(namespace, Ordering::Release);
            Ok(Operand::object(receiver))
        })();
        for token in tokens.into_iter().rev() {
            id.send_named(registry, b"release:", &[Operand::word(token)])?;
        }
        result
    }
}
unsafe extern "C" fn initialize(
    id: *const Protocol,
    args: *const Operand,
    count: usize,
) -> Outcome {
    if !unsafe { native::valid_protocol(id) } {
        return invalid();
    }
    let Some([registry]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Ok(registry) = object(*registry) else {
        return invalid();
    };
    let id = unsafe { *id };
    let existing = INSTANCE.load(Ordering::Acquire);
    if !existing.is_null() {
        return if unsafe { (*existing).id.same_contract(&id) && (*existing).registry == registry } {
            Outcome::loaded(Operand::object(unsafe { (*existing).receiver }))
        } else {
            invalid()
        };
    }
    match unsafe { construct(id, registry) } {
        Ok(value) => Outcome::loaded(value),
        Err(error) => error,
    }
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: Module = Module {
    version: native::VERSION,
    byte_size: size_of::<Module>() as u32,
    initialize: Some(initialize),
};
