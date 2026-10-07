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
    selectors: [Oop; 7],
    proof_services: [Option<NativeEntry>; 3],
    output: RefCell<Vec<usize>>,
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
// Bulk cleanup preserves the complete logical token list. It does not
// coalesce roots or change the callback-visible rootsDo: snapshot.
unsafe fn can_batch(
    registry: &Registry,
    context: *mut c_void,
    method: Method,
    selector: Oop,
) -> bool {
    // Explicit Words have no uninitialized ABI-union padding.
    let mut request = [0usize; 16];
    for (i, (name, expected)) in registry.selectors[..3]
        .iter()
        .zip(registry.proof_services)
        .enumerate()
    {
        let Some(expected) = expected else {
            return false;
        };
        let Some(method) = expected.method else {
            return false;
        };
        request[i * 4] = registry.id.receiver.addr();
        request[i * 4 + 1] = name.addr();
        request[i * 4 + 2] = method as usize;
        request[i * 4 + 3] = expected.context as usize;
    }
    request[12] = registry.receiver.addr();
    request[13] = selector.addr();
    request[14] = method as usize;
    request[15] = context as usize;
    let bytes = unsafe {
        core::slice::from_raw_parts(request.as_ptr().cast::<u8>(), size_of_val(&request))
    };
    unsafe {
        registry.id.send(
            registry.id.receiver,
            registry.selectors[5],
            &[Operand::bytes(bytes)],
        )
    }
    .ok()
    .and_then(|v| v.as_word())
        == Some(1)
}
unsafe fn release_many(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
    forward: bool,
) -> Outcome {
    let Some([argument]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(bytes) = argument.as_bytes() else {
        return invalid();
    };
    let Some(bytes) = (unsafe { birnam_abi::borrowed_bytes(bytes.pointer, bytes.length) }) else {
        return invalid();
    };
    if bytes.len() % size_of::<usize>() != 0 {
        return invalid();
    }
    // Copy ABI bytes before permission messages can reenter the caller.
    let tokens: Vec<usize> = bytes
        .chunks_exact(size_of::<usize>())
        .map(|chunk| usize::from_ne_bytes(chunk.try_into().unwrap()))
        .collect();
    let registry = unsafe { &*context.cast::<Registry>() };
    if receiver != registry.receiver
        || !unsafe { can_batch(registry, context, release, registry.selectors[3]) }
    {
        return Outcome::loaded(Operand::word(0));
    }
    let mut roots = registry.roots.borrow_mut();
    if forward
        && (tokens.iter().any(|t| !roots.positions.contains_key(t))
            || tokens
                .iter()
                .copied()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != tokens.len())
    {
        return Outcome::loaded(Operand::word(0));
    }
    let tokens: Vec<_> = if forward {
        tokens
    } else {
        tokens.into_iter().rev().collect()
    };
    for token in tokens {
        // Ordinary generated finish ignores release's 0/1 result. Preserve
        // that behavior for unknown/repeated tokens as well as exact order.
        roots.release(token);
    }
    Outcome::loaded(Operand::word(1))
}
unsafe extern "C" fn try_release_many(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    unsafe { release_many(c, r, a, n, false) }
}
unsafe extern "C" fn try_release_forward(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    unsafe { release_many(c, r, a, n, true) }
}
unsafe extern "C" fn try_retain_many(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([argument]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(bytes) = argument
        .as_bytes()
        .and_then(|b| unsafe { birnam_abi::borrowed_bytes(b.pointer, b.length) })
    else {
        return invalid();
    };
    if bytes.is_empty()
        || bytes.len() % size_of::<usize>() != 0
        || bytes.len() / size_of::<usize>() > 33
    {
        return invalid();
    }
    let values: Vec<Oop> = bytes
        .chunks_exact(size_of::<usize>())
        .map(|b| {
            core::ptr::with_exposed_provenance_mut(usize::from_ne_bytes(b.try_into().unwrap()))
        })
        .collect();
    let registry = unsafe { &*context.cast::<Registry>() };
    if receiver != registry.receiver
        || values.iter().any(|o| o.is_null())
        || !unsafe { can_batch(registry, context, retain, registry.selectors[6]) }
    {
        return Outcome::loaded(Operand::word(0));
    }
    // Proof-message dispatch may have reentered before the pure Id query ran.
    // Recheck capacity after it returns, then perform no further sends.
    if registry
        .roots
        .borrow()
        .next
        .checked_add(values.len())
        .is_none()
    {
        return Outcome::loaded(Operand::word(0));
    }
    let mut roots = registry.roots.borrow_mut();
    let mut output = registry.output.borrow_mut();
    output.clear();
    for value in values {
        output.push(roots.retain(value).unwrap());
    }
    let bytes = unsafe {
        core::slice::from_raw_parts(
            output.as_ptr().cast::<u8>(),
            output.len() * size_of::<usize>(),
        )
    };
    Outcome::loaded(Operand::bytes(bytes))
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
        .chain(unsafe { (&*context.cast::<Registry>()).selectors }.into_iter())
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
            selectors: [
                b"behaviorEpoch".as_slice(),
                b"usesDefaultLookup:",
                b"bind:to:",
                b"release:",
                b"bindingOf:in:",
                b"nativeBindingsMatch:",
                b"retain:",
            ]
            .map(|name| id.intern_name(name))
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .unwrap(),
            proof_services: [
                b"behaviorEpoch".as_slice(),
                b"usesDefaultLookup:",
                b"bind:to:",
            ]
            .map(|name| {
                id.send_named(
                    id.receiver,
                    b"bindingOf:in:",
                    &[
                        Operand::object(id.intern_name(name).unwrap()),
                        Operand::object(id.receiver),
                    ],
                )
                .ok()
                .and_then(|v| v.as_entry())
            }),
            output: RefCell::new(Vec::new()),
        }));
        // Id pins this library and the installed native contexts for process life.
        for (name, method) in [
            (b"retain:".as_slice(), retain as Method),
            (b"release:", release),
            (b"tryReleaseMany:", try_release_many),
            (b"tryRetainMany:", try_retain_many),
            (b"tryReleaseManyForward:", try_release_forward),
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

#[cfg(test)]
mod batch_reentry_tests {
    use super::*;
    struct Reentry {
        registry: *mut Registry,
    }
    unsafe extern "C" fn proof(
        c: *mut c_void,
        _: Oop,
        _: Oop,
        _: *const Operand,
        _: usize,
    ) -> Outcome {
        let r = unsafe { &*(*c.cast::<Reentry>()).registry };
        // Simulate lookup dispatch reentry before the callback-free Id query.
        r.roots.borrow_mut().next = usize::MAX;
        Outcome::loaded(Operand::word(1))
    }
    unsafe extern "C" fn dummy(_: *mut c_void, _: Oop, _: *const Operand, _: usize) -> Outcome {
        Outcome::loaded(Operand::unit())
    }
    #[test]
    fn reentrant_proof_cannot_invalidate_a_cached_capacity_check() {
        unsafe {
            let mut reentry = Reentry {
                registry: ptr::null_mut(),
            };
            let id = Protocol {
                version: native::VERSION,
                byte_size: size_of::<Protocol>() as u32,
                message: Some(proof),
                context: (&raw mut reentry).cast(),
                receiver: ptr::without_provenance_mut(8),
                intern: ptr::without_provenance_mut(16),
                load: ptr::without_provenance_mut(24),
            };
            let mut r = Registry {
                id,
                receiver: ptr::without_provenance_mut(32),
                roots: RefCell::new(Roots::default()),
                selectors: [ptr::without_provenance_mut(40); 7],
                proof_services: [Some(NativeEntry::new(dummy, ptr::null_mut())); 3],
                output: RefCell::new(Vec::new()),
            };
            reentry.registry = &raw mut r;
            let values = [48usize, 56, 64];
            let bytes =
                core::slice::from_raw_parts(values.as_ptr().cast::<u8>(), size_of_val(&values));
            let result =
                try_retain_many((&raw mut r).cast(), r.receiver, &Operand::bytes(bytes), 1);
            assert_eq!(result.status, native::LOADED);
            assert_eq!(result.value.as_word(), Some(0));
            assert!(r.roots.borrow().retained.is_empty());
            assert!(r.output.borrow().is_empty());
        }
    }
}
