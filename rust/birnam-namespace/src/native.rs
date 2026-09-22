//! Namespace takes a Roots object and returns an opaque Namespace object.
//!
//! Symbol keys: `at:`, `at:put:`, `nameOf:` and `nameAt:`; missing entries return
//! Unit. `count` and `nameAt:` use native Words until language Integers exist.
//! `bindingsDo:` passes two Objects to a synchronous NativeEntry visitor;
//! `rootsDo:` passes one Object. Both snapshot before calling foreign code.
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
unsafe fn symbol(id: Protocol, key: Operand) -> Result<Oop, Outcome> {
    let object = object(key)?;
    if unsafe { id.send_named(id.receiver, b"symbolBytesOf:", &[key]) }?.kind() != native::BYTES {
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
    match unsafe { symbol(namespace.id, *key) } {
        Ok(key) => Outcome::loaded(value(namespace.storage.borrow().import(key))),
        Err(error) => error,
    }
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
    match unsafe { symbol(namespace.id, *key) } {
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
    let Some(visitor) = argument.as_entry().filter(|v| v.method.is_some()) else {
        return invalid();
    };
    let snapshot = {
        let namespace = unsafe { &*context.cast::<Namespace>() };
        let mut snapshot = namespace.storage.borrow().roots();
        snapshot.extend([receiver, namespace.registry]);
        snapshot
    };
    for object in snapshot.into_iter().filter(|o| !o.is_null()) {
        let result = unsafe {
            visitor.method.unwrap()(visitor.context, receiver, &Operand::object(object), 1)
        };
        if result.status != native::LOADED {
            return result;
        }
    }
    Outcome::loaded(Operand::unit())
}
unsafe extern "C" fn bindings(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([argument]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(visitor) = argument.as_entry().filter(|v| v.method.is_some()) else {
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
            visitor.method.unwrap()(
                visitor.context,
                receiver,
                arguments.as_ptr(),
                arguments.len(),
            )
        };
        if result.status != native::LOADED {
            return result;
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
        let prototype = id.send_named(id.receiver, b"objectPrototype", &[])?;
        let parent = object(id.send_named(id.receiver, b"behaviorOf:", &[prototype])?)?;
        let behavior = object(id.send_named(parent, b"delegated", &[])?)?;
        let receiver = object(id.send_named(behavior, b"allocate", &[Operand::word(0)])?)?;
        let namespace = Box::into_raw(Box::new(Namespace {
            id,
            receiver,
            registry,
            storage: RefCell::new(Environment::new()),
        }));
        for (name, method) in [
            (b"at:".as_slice(), find as Method),
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
