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
    active: RefCell<Vec<Oop>>,
    names: std::collections::HashMap<&'static [u8], Oop>,
    // Same retained Symbol as the inventory entry; never a cached method.
    symbol_bytes_selector: Oop,
    at_selector: Oop,
}
struct Scope<'a> { namespace: &'a Namespace, start: usize }
impl Scope<'_> {
    fn keep(&self, value: Oop) -> Oop { self.namespace.active.borrow_mut().push(value); value }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) { self.namespace.active.borrow_mut().truncate(self.start); }
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
    if unsafe { namespace.id.send(namespace.id.receiver, namespace.symbol_bytes_selector, &[key]) }?.kind() != native::BYTES {
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
        let result = namespace.id.send(r, namespace.at_selector, args)?;
        if result.kind() != native::UNIT { return Ok(result); }
        namespace.id.send(r, namespace.at_selector, &[Operand::object(namespace.names[b"nil".as_slice()])])
    })().map_or_else(|e| e, Outcome::loaded)
}
// Resolve source globals with the compiler's original precedence and Id calls.
// Namespace owns the temporary reference inventory, never Store/Globals payloads.
unsafe extern "C" fn resolve_global(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let namespace = &*c.cast::<Namespace>();
        let [key] = borrowed_slice(a,n).ok_or_else(invalid)? else { return Err(invalid()); };
        let key = object(*key)?;
        let scope = Scope { namespace, start: namespace.active.borrow().len() };
        scope.keep(r); scope.keep(key);
        let nil = scope.keep(object(namespace.id.send(r, namespace.at_selector,
            &[Operand::object(namespace.names[b"nil".as_slice()])])?)?);
        let globals = scope.keep(object(namespace.id.send(r, namespace.at_selector,
            &[Operand::object(namespace.names[b"Globals".as_slice()])])?)?);
        let found = object(namespace.id.send(globals, namespace.at_selector, &[Operand::object(key)])?)?;
        if found != nil { return Ok(Operand::object(found)); }
        let fallback = namespace.id.send(r, namespace.at_selector, &[Operand::object(key)])?;
        if fallback.kind() == native::UNIT { Ok(Operand::object(nil)) }
        else { object(fallback).map(Operand::object) }
    })().map_or_else(|e|e, Outcome::loaded)
}
unsafe extern "C" fn includes_key(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let namespace = &*c.cast::<Namespace>();
        let [key] = borrowed_slice(a, n).ok_or_else(invalid)? else { return Err(invalid()); };
        let value = namespace.id.send(r, namespace.at_selector, &[*key])?;
        namespace.id.send(namespace.receiver, namespace.at_selector, &[Operand::object(namespace.names[if value.kind() == native::UNIT { b"false".as_slice() } else { b"true".as_slice() }])])
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
        snapshot.extend(namespace.active.borrow().iter().copied());
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
    unsafe { visit_bindings(context, args, count, false) }
}
unsafe extern "C" fn live_bindings(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    unsafe { visit_bindings(context, args, count, true) }
}
unsafe fn visit_bindings(context: *mut c_void, args: *const Operand, count: usize, live_only: bool) -> Outcome {
    let Some([argument]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(visitor) = argument.as_object().filter(|o| !o.is_null()) else {
        return invalid();
    };
    let namespace = unsafe { &*context.cast::<Namespace>() };
    let scope = Scope { namespace, start: namespace.active.borrow().len() };
    scope.keep(namespace.receiver);
    scope.keep(visitor);
    let snapshot = namespace.storage.borrow().bindings().to_vec();
    // Replacement can remove the original namespace edges and collect in a
    // visitor. Retain both halves of every copied pair before the first send.
    for binding in &snapshot {
        scope.keep(binding.name);
        if !binding.value.is_null() { scope.keep(binding.value); }
    }
    for binding in snapshot {
        if live_only && binding.value.is_null() { continue; }
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
            for name in [b"symbolBytesOf:".as_slice(), b"at:", b"nil", b"true", b"false", b"Globals"] {
                let symbol = id.intern_name(name)?;
                tokens.push(id.send(registry, retain, &[Operand::object(symbol)])?.as_word().ok_or_else(invalid)?);
                names.insert(name, symbol);
            }
            let namespace = Box::into_raw(Box::new(Namespace {
                id,
                receiver,
                registry,
                storage: RefCell::new(Environment::new()),
                active: RefCell::new(Vec::new()),
                symbol_bytes_selector: names[b"symbolBytesOf:".as_slice()],
                at_selector: names[b"at:".as_slice()],
                names,
            }));
            for (name, method) in [
                (b"at:".as_slice(), find as Method),
                (b"valueAt:", value_at),
                (b"resolveGlobal:", resolve_global),
                (b"includesKey:", includes_key),
                (b"at:put:", bind),
                (b"count", count),
                (b"nameAt:", name_at),
                (b"nameOf:", reverse),
                (b"rootsDo:", roots),
                (b"bindingsDo:", bindings),
                (b"liveBindingsDo:", live_bindings),
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
