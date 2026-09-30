//! Ordered root providers and retained references, exposed only as Id messages.
//!
//! `retain:` takes an Object and returns a nonzero Word token; `release:` takes
//! that token and returns Word 1/0. `register:` adds an Object implementing
//! `rootsDo:`. `rootsDo:` takes an Object receiving `visit:` for each root.
//! Enumeration is synchronous. No storage borrow crosses a send.
use birnam_abi::id_protocol::{
    self as native, Method, Module, NativeEntry, Operand, Outcome, Protocol,
};
use birnam_abi::{Oop, borrowed_slice};
use core::{cell::RefCell, ffi::c_void, ptr};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicPtr, Ordering},
};

#[derive(Default)]
struct Roots {
    providers: Vec<Oop>,
    retained: Vec<(usize, Oop)>,
    positions: HashMap<usize, usize>,
    next: usize,
}
impl Roots {
    fn retain(&mut self, object: Oop) -> Option<usize> {
        if object.is_null() {
            return None;
        }
        self.next = self.next.checked_add(1)?;
        self.positions.insert(self.next, self.retained.len());
        self.retained.push((self.next, object));
        Some(self.next)
    }
    fn release(&mut self, token: usize) -> bool {
        let Some(index) = self.positions.remove(&token) else {
            return false;
        };
        self.retained.swap_remove(index);
        if let Some((moved, _)) = self.retained.get(index) {
            // Preserve the existing swap-removal enumeration order without
            // scanning the whole live root set for every parser temporary.
            *self.positions.get_mut(moved).unwrap() = index;
        }
        true
    }
    fn register(&mut self, provider: Oop) -> bool {
        if provider.is_null() {
            return false;
        }
        self.providers.push(provider);
        true
    }
}
struct Registry {
    id: Protocol,
    receiver: Oop,
    roots: RefCell<Roots>,
}
static INSTANCE: AtomicPtr<Registry> = AtomicPtr::new(ptr::null_mut());
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn object(value: Operand) -> Result<Oop, Outcome> {
    value
        .as_object()
        .filter(|o| !o.is_null())
        .ok_or_else(invalid)
}
unsafe extern "C" fn retain(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([argument]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Ok(object) = object(*argument) else {
        return invalid();
    };
    let registry = unsafe { &*context.cast::<Registry>() };
    match registry.roots.borrow_mut().retain(object) {
        Some(token) => Outcome::loaded(Operand::word(token)),
        None => invalid(),
    }
}
unsafe extern "C" fn release(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([argument]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(token) = argument.as_word() else {
        return invalid();
    };
    let registry = unsafe { &*context.cast::<Registry>() };
    Outcome::loaded(Operand::word(usize::from(
        registry.roots.borrow_mut().release(token),
    )))
}
unsafe extern "C" fn register(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([argument]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Ok(provider) = object(*argument) else {
        return invalid();
    };
    let registry = unsafe { &*context.cast::<Registry>() };
    if registry.roots.borrow_mut().register(provider) {
        Outcome::loaded(Operand::unit())
    } else {
        invalid()
    }
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
    let (id, providers, retained) = {
        let registry = unsafe { &*context.cast::<Registry>() };
        let roots = registry.roots.borrow();
        (registry.id, roots.providers.clone(), roots.retained.clone())
    };
    // Snapshot retains callback order; providers themselves are also roots.
    for object in [receiver]
        .into_iter()
        .chain(providers.iter().copied())
        .chain(retained.into_iter().map(|(_, o)| o))
    {
        let result = unsafe { id.send_named(visitor, b"visit:", &[Operand::object(object)]) };
        if let Err(error) = result {
            return error;
        }
    }
    for provider in providers {
        if let Err(error) = unsafe { id.send_named(provider, b"rootsDo:", &[*argument]) } {
            return error;
        }
    }
    Outcome::loaded(Operand::unit())
}
unsafe fn construct(id: Protocol) -> Result<Operand, Outcome> {
    unsafe {
        let prototype = id.send_named(id.receiver, b"objectPrototype", &[])?;
        let parent = object(id.send_named(id.receiver, b"behaviorOf:", &[prototype])?)?;
        let behavior = object(id.send_named(parent, b"delegated", &[])?)?;
        let receiver = object(id.send_named(behavior, b"allocate", &[Operand::word(0)])?)?;
        let registry = Box::into_raw(Box::new(Registry {
            id,
            receiver,
            roots: RefCell::new(Roots::default()),
        }));
        // Id pins this library and the installed native contexts for process life.
        for (name, method) in [
            (b"retain:".as_slice(), retain as Method),
            (b"release:", release),
            (b"register:", register),
            (b"rootsDo:", roots),
        ] {
            id.send_named(
                behavior,
                b"addMethod",
                &[
                    Operand::object(id.intern_name(name)?),
                    Operand::entry(NativeEntry::new(method, registry.cast())),
                ],
            )?;
        }
        INSTANCE.store(registry, Ordering::Release);
        Ok(Operand::object(receiver))
    }
}
unsafe extern "C" fn initialize(id: *const Protocol, _: *const Operand, count: usize) -> Outcome {
    if count != 0 || !unsafe { native::valid_protocol(id) } {
        return invalid();
    }
    let id = unsafe { *id };
    let existing = INSTANCE.load(Ordering::Acquire);
    if !existing.is_null() {
        return if unsafe { (*existing).id.same_contract(&id) } {
            Outcome::loaded(Operand::object(unsafe { (*existing).receiver }))
        } else {
            invalid()
        };
    }
    match unsafe { construct(id) } {
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
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tokens_are_unique_release_is_exact_and_exhaustion_is_inert() {
        let mut roots = Roots::default();
        let object = ptr::without_provenance_mut(8);
        let first = roots.retain(object).unwrap();
        let second = roots.retain(object).unwrap();
        assert_ne!(first, second);
        assert!(roots.release(first));
        assert!(!roots.release(first));
        assert_eq!(roots.retained, [(second, object)]);
        roots.next = usize::MAX;
        assert_eq!(roots.retain(object), None);
        assert_eq!(roots.retained, [(second, object)]);
    }
    #[test]
    fn releasing_moved_slots_keeps_duplicate_references_independent() {
        let mut roots = Roots::default();
        let object = ptr::without_provenance_mut(8);
        for _ in 0..3 {
            let tokens: Vec<_> = (0..4096).map(|_| roots.retain(object).unwrap()).collect();
            // Removing odd tokens moves both later odd and even entries. Every
            // remaining lease must still refer to exactly its original root.
            for token in tokens.iter().step_by(2) {
                assert!(roots.release(*token));
                assert!(!roots.release(*token));
            }
            let mut remaining: Vec<_> = roots
                .retained
                .iter()
                .map(|(token, value)| {
                    assert_eq!(*value, object);
                    *token
                })
                .collect();
            remaining.sort_unstable();
            assert_eq!(
                remaining,
                tokens
                    .iter()
                    .skip(1)
                    .step_by(2)
                    .copied()
                    .collect::<Vec<_>>()
            );
            for token in tokens.iter().skip(1).step_by(2).rev() {
                assert!(roots.release(*token));
            }
            assert!(roots.retained.is_empty());
            assert!(roots.positions.is_empty());
        }
    }
    #[test]
    fn registration_grows_preserving_order_duplicates_and_null_rejection() {
        let mut roots = Roots::default();
        for index in 0..128 {
            assert!(roots.register(ptr::without_provenance_mut(index % 2 + 1)));
        }
        let before = roots.providers.clone();
        assert!(!roots.register(ptr::null_mut()));
        assert_eq!(roots.providers, before);
        assert_eq!(
            roots.providers.iter().map(|p| p.addr()).collect::<Vec<_>>(),
            (0..128).map(|i| i % 2 + 1).collect::<Vec<_>>()
        );
    }
}
