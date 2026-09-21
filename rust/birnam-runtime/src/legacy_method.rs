//! Upper Runtime adaptation of the existing all-Oop native Method ABI.
//! Id itself stores and invokes typed entries. Remove this adapter when upper
//! native consumers have migrated; it is not part of independent Id.
use crate::abi::{
    Method, Oop,
    id_protocol::{self as native, NativeEntry, Operand, Outcome},
};
use crate::raw::MethodAddress;
use crate::boundary;
use core::cell::RefCell;
use core::ffi::c_void;

thread_local! {
    // A legacy lookup can return only a Method address. Preserve the complete
    // entries converted by its delegates for this call, without changing that
    // legacy ABI or retaining anything after the lookup returns.
    static RESULTS: RefCell<Vec<Vec<(MethodAddress, NativeEntry)>>> = const { RefCell::new(Vec::new()) };
}
struct ResultScope;
impl ResultScope {
    fn new() -> Self {
        RESULTS.with(|results| results.borrow_mut().push(Vec::new()));
        Self
    }
    fn selected(&self, method: MethodAddress) -> Result<Option<NativeEntry>, Outcome> {
        RESULTS.with(|results| {
            let results = results.borrow();
            let mut selected = None;
            for (_, entry) in results
                .last()
                .unwrap()
                .iter()
                .filter(|(address, _)| *address == method)
            {
                if selected.is_some_and(|previous| previous != *entry) {
                    // Distinct contexts with the same legacy address cannot be
                    // distinguished by that ABI. Require a typed result instead
                    // of guessing which callable the author intended to return.
                    return Err(Outcome::failed(native::INVALID_REQUEST, 0));
                }
                selected = Some(*entry);
            }
            Ok(selected)
        })
    }
}
impl Drop for ResultScope {
    fn drop(&mut self) {
        RESULTS.with(|results| {
            results
                .borrow_mut()
                .pop()
                .expect("missing legacy result scope");
        });
    }
}

pub(crate) fn encode(method: Method, lookup: bool) -> NativeEntry {
    let address = method as usize;
    let invoke = if lookup || address == lookup_method as Method as usize {
        invoke_lookup as native::Method
    } else if address == crate::dispatch::vtable_add_method as *const () as usize
        || address == crate::dispatch::vtable_remove_method as *const () as usize
        || address == add_method as Method as usize
        || address == remove_method as Method as usize
    {
        invoke_entry as native::Method
    } else {
        invoke_object as native::Method
    };
    NativeEntry::new(invoke, MethodAddress::from_method(method).as_oop())
}
/// Only these adapters contain an actual all-Oop function in their context.
/// `try_decode` also recognizes native equivalents for legacy reflection and
/// registration, and must never be used to choose which function to invoke.
pub(crate) fn direct_method(entry: NativeEntry) -> Option<MethodAddress> {
    let address = entry.method.map(|method| method as usize);
    [invoke_object as native::Method, invoke_lookup, invoke_entry]
        .iter()
        .any(|method| address == Some(*method as usize))
        .then(|| unsafe { MethodAddress::from_oop(entry.context) })
        .flatten()
}

/// Temporary argument adaptation at the old all-Oop boundary. The selected
/// native function and context are retained; only the operand carrier changes.
pub(crate) fn native_arguments(
    entry: NativeEntry,
    arguments: &[Oop],
    runtime: &crate::runtime::Runtime,
    legacy_operands: bool,
) -> Vec<Operand> {
    let address = entry.method.map(|method| method as usize);
    if (address == Some(crate::runtime::id_protocol::add_method as native::Method as usize)
        || runtime.core_legacy_method(entry) == Some(MethodAddress::from_method(add_method)))
        && let [selector, object] = arguments
        && let Some(method) = runtime.owned_method_entry(*object)
    {
        // An all-Oop language implementation installed as lookup answers a
        // Method object. Adapt that result to Id's typed entry protocol while
        // preserving the selected underlying implementation.
        let method = if *selector == runtime.globals().lookup {
            direct_method(method).map_or(method, |address| runtime.encode_legacy_method(address.to_method(), true))
        } else { method };
        return vec![Operand::object(*selector), Operand::entry(method)];
    }
    if legacy_operands && address == Some(crate::runtime::id_protocol::allocate as native::Method as usize)
        && let [size] = arguments
    {
        return vec![Operand::word(size.addr())];
    }
    if (address == Some(crate::runtime::id_protocol::add_method as native::Method as usize)
        || runtime.core_legacy_method(entry) == Some(MethodAddress::from_method(add_method)))
        && let [selector, method] = arguments
        && legacy_operands
        && let Some(method) = unsafe { MethodAddress::from_oop(*method) }
    {
        return vec![
            Operand::object(*selector),
            Operand::entry(
                runtime.encode_legacy_method(
                    method.to_method(),
                    *selector == runtime.globals().lookup,
                ),
            ),
        ];
    }
    arguments.iter().copied().map(Operand::object).collect()
}

/// Preserve the old lookup/mutation result convention only for representable
/// entries. Arbitrary native entries go through the checked language boundary.
pub(crate) fn native_result(entry: NativeEntry, outcome: Outcome, runtime: &crate::runtime::Runtime) -> Option<Oop> {
    let address = entry.method.map(|method| method as usize);
    let old_entry_result = runtime.core_legacy_method(entry).is_some() || [
        crate::runtime::id_protocol::add_method as native::Method,
        crate::runtime::id_protocol::remove_method,
    ]
    .iter()
    .any(|method| address == Some(*method as usize));
    if old_entry_result
        && outcome.status == native::LOADED
        && (outcome.value.kind() == native::UNIT
            || outcome.value.as_entry().and_then(|entry| runtime.try_decode_legacy_method(entry)).is_some())
    {
        Some(old_result(runtime, outcome))
    } else {
        None
    }
}

/// These legacy functions return native entries rather than language Objects.
pub(crate) fn returns_entry(entry: NativeEntry) -> bool {
    entry.method.is_some_and(|method| method as usize == invoke_lookup as native::Method as usize
        || method as usize == invoke_entry as native::Method as usize)
}

#[cfg(test)]
pub(crate) fn decode(entry: NativeEntry) -> MethodAddress {
    try_decode(entry).unwrap_or_else(|| {
        boundary::fatal_invariant("typed native entry has no legacy Method adapter")
    })
}
pub(crate) fn try_decode(entry: NativeEntry) -> Option<MethodAddress> {
    let address = entry.method.map(|method| method as usize);
    if let Some(method) = direct_method(entry) {
        return Some(method);
    }
    for (new, old) in [
        (
            crate::runtime::id_protocol::allocate as native::Method,
            crate::dispatch::vtable_allocate as Method,
        ),
        (
            crate::runtime::id_protocol::delegated as native::Method,
            crate::dispatch::vtable_delegated as Method,
        ),
        (
            crate::runtime::id_protocol::add_method as native::Method,
            crate::dispatch::vtable_add_method as Method,
        ),
        (
            crate::runtime::id_protocol::remove_method as native::Method,
            crate::dispatch::vtable_remove_method as Method,
        ),
    ] {
        if address == Some(new as usize) {
            return Some(MethodAddress::from_method(old));
        }
    }
    None
}
unsafe fn invoke(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Result<Oop, Outcome> {
    if count != 0 && args.is_null() {
        return Err(Outcome::failed(native::INVALID_REQUEST, 0));
    }
    let args = if count == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(args, count) }
    };
    let converted: Option<Vec<_>> = args
        .iter()
        .map(|arg| {
            arg.as_object()
                .or_else(|| arg.as_entry().and_then(|entry| crate::runtime::with_active(|runtime| runtime.try_decode_legacy_method(entry).map(MethodAddress::as_oop))))
        })
        .collect();
    let Some(converted) = converted else {
        return Err(Outcome::failed(native::INVALID_REQUEST, 0));
    };
    let method = unsafe { MethodAddress::from_oop(context) }.expect("legacy adapter has no method");
    Ok(unsafe { method.to_method()(receiver, converted.as_ptr(), converted.len()) })
}
unsafe extern "C" fn invoke_object(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    match unsafe { invoke(context, receiver, args, count) } {
        Ok(result) => Outcome::loaded(Operand::object(result)),
        Err(error) => error,
    }
}
unsafe fn invoke_method_result(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
    lookup: bool,
) -> Outcome {
    let scope = ResultScope::new();
    match unsafe { invoke(context, receiver, args, count) } {
        Ok(result) => {
            if let Some(entry) = crate::runtime::with_active(|runtime| runtime.owned_method_entry(result)) {
                return Outcome::loaded(Operand::entry(entry));
            }
            // A language lookup returns Method or nil. Other live objects must
            // never be treated as executable addresses by the native adapter.
            // Raw addresses remain the explicit unsafe native ABI contract.
            if let Some(outcome) = crate::runtime::with_active(|runtime| {
                if runtime.payload_size(result).is_none() { return None; }
                Some(if result == runtime.canonical().nil { Outcome::loaded(Operand::unit()) }
                    else { Outcome::failed(native::INVALID_REQUEST, 0) })
            }) { return outcome; }
            let Some(method) = (unsafe { MethodAddress::from_oop(result) }) else {
                return Outcome::loaded(Operand::unit());
            };
            match scope.selected(method) {
                Ok(entry) => Outcome::loaded(Operand::entry(entry.unwrap_or_else(|| {
                    crate::runtime::with_active(|runtime| {
                        runtime.encode_legacy_method(method.to_method(), lookup)
                    })
                }))),
                Err(error) => error,
            }
        }
        Err(error) => error,
    }
}
unsafe extern "C" fn invoke_lookup(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let lookup = count != 0
        && !args.is_null()
        && crate::runtime::with_active(|runtime| {
            unsafe { args.read() }.as_object() == Some(runtime.globals().lookup)
        });
    unsafe { invoke_method_result(context, receiver, args, count, lookup) }
}
unsafe extern "C" fn invoke_entry(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    unsafe { invoke_method_result(context, receiver, args, count, false) }
}
fn old_result(runtime: &crate::runtime::Runtime, result: Outcome) -> Oop {
    if result.status != native::LOADED {
        boundary::fatal_invariant("Id primitive rejected legacy Method arguments");
    }
    if result.value.kind() == native::UNIT {
        return core::ptr::null_mut();
    }
    let entry = result
        .value
        .as_entry()
        .expect("Id primitive returned a non-entry");
    let method = runtime.decode_legacy_method(entry);
    RESULTS.with(|results| {
        if let Some(scope) = results.borrow_mut().last_mut() {
            scope.push((method, entry));
        }
    });
    method.as_oop()
}
pub(crate) unsafe extern "C" fn lookup_method(
    receiver: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    if count == 0 || args.is_null() {
        boundary::fatal_invariant("lookup requires selector");
    }
    let selector = unsafe { args.read() };
    crate::runtime::with_active(|runtime| {
        let entry = unsafe { runtime.lookup_native_method_active(receiver, selector) };
        old_result(runtime, Outcome::loaded(entry.map(Operand::entry).unwrap_or_else(Operand::unit)))
    })
}
pub(crate) unsafe extern "C" fn add_method(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    if count < 2 || args.is_null() {
        boundary::fatal_invariant("addMethod requires selector and method");
    }
    let method = unsafe { MethodAddress::from_oop(args.add(1).read()) }.expect("null method");
    let selector = unsafe { args.read() };
    crate::runtime::with_active(|runtime| {
        let entry =
            runtime.encode_legacy_method(method.to_method(), selector == runtime.globals().lookup);
        runtime.primitive_add_method(receiver, selector, entry);
        old_result(runtime, Outcome::loaded(Operand::entry(entry)))
    })
}
pub(crate) unsafe extern "C" fn remove_method(
    receiver: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    if count == 0 || args.is_null() {
        boundary::fatal_invariant("removeMethod: requires selector");
    }
    let selector = unsafe { args.read() };
    crate::runtime::with_active(|runtime| {
        let removed = runtime.primitive_remove_method(receiver, selector);
        old_result(runtime, Outcome::loaded(removed.map(Operand::entry).unwrap_or_else(Operand::unit)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{dispatch::build_bootstrap, runtime::with_active};

    unsafe extern "C" fn identity(receiver: Oop, _: *const Oop, _: usize) -> Oop {
        receiver
    }

    unsafe extern "C" fn replacement(_: Oop, _: *const Oop, _: usize) -> Oop {
        MethodAddress::from_method(identity).as_oop()
    }

    unsafe extern "C" fn select_then_replace(receiver: Oop, args: *const Oop, _: usize) -> Oop {
        with_active(|runtime| {
            let selector = unsafe { args.read() };
            let entry = unsafe { runtime.lookup_native_method_active(receiver, selector) }.unwrap();
            let selected = old_result(runtime, Outcome::loaded(Operand::entry(entry)));
            runtime.add_method(receiver, selector, identity);
            // A nested lookup returns a newly supplied legacy callback. It must
            // neither inherit nor erase the outer lookup's selected entry.
            let nested = unsafe {
                invoke_method_result(
                    MethodAddress::from_method(replacement).as_oop(),
                    receiver,
                    core::ptr::null(),
                    0,
                    false,
                )
            };
            assert_eq!(nested.value.as_entry(), Some(encode(identity, false)));
            selected
        })
    }

    #[test]
    fn legacy_lookup_retains_selected_native_allocator_across_nesting_and_replacement() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        let behavior = runtime.create_vtable(runtime.globals().object_vt);
        let selector = runtime.intern(b"legacyAllocatorProbe").unwrap();
        let allocator = NativeEntry::new(
            crate::runtime::id_protocol::allocate,
            core::ptr::from_ref(runtime).cast_mut().cast(),
        );
        runtime.add_native_method(behavior, selector, allocator);
        runtime.enter(|| {
            let selected = unsafe {
                invoke_method_result(
                    MethodAddress::from_method(select_then_replace).as_oop(),
                    behavior,
                    [Operand::object(selector)].as_ptr(),
                    1,
                    false,
                )
            };
            assert_eq!(selected.status, native::LOADED);
            let entry = selected.value.as_entry().unwrap();
            assert_eq!(entry, allocator);
            let allocated = unsafe {
                entry.method.unwrap()(entry.context, behavior, [Operand::word(24)].as_ptr(), 1)
            };
            assert_eq!(allocated.status, native::LOADED);
            let object = allocated.value.as_object().unwrap();
            assert!(!object.is_null());
            assert_eq!(runtime.checked_vtable_of(object), behavior);
            assert_eq!(
                unsafe { runtime.lookup_native_method_active(behavior, selector) },
                Some(encode(identity, false)),
                "next lookup sees the replacement"
            );
        });
        RESULTS.with(|results| assert!(results.borrow().is_empty()));
    }

    #[test]
    fn identical_legacy_addresses_cannot_silently_choose_between_native_contexts() {
        let first = build_bootstrap();
        let second = build_bootstrap();
        let entry = |runtime: &crate::runtime::Runtime| {
            NativeEntry::new(
                crate::runtime::id_protocol::allocate,
                core::ptr::from_ref(runtime).cast_mut().cast(),
            )
        };
        let first_entry = entry(&first.runtime);
        let second_entry = entry(&second.runtime);
        {
            let outer = ResultScope::new();
            let address = old_result(&first.runtime, Outcome::loaded(Operand::entry(first_entry)));
            {
                let inner = ResultScope::new();
                assert_eq!(
                    old_result(&second.runtime, Outcome::loaded(Operand::entry(second_entry))),
                    address
                );
                assert_eq!(
                    inner.selected(decode(second_entry)).ok().unwrap(),
                    Some(second_entry)
                );
            }
            assert_eq!(
                outer.selected(decode(first_entry)).ok().unwrap(),
                Some(first_entry)
            );
            old_result(&first.runtime, Outcome::loaded(Operand::entry(first_entry)));
            assert_eq!(
                outer.selected(decode(first_entry)).ok().unwrap(),
                Some(first_entry)
            );
            old_result(&second.runtime, Outcome::loaded(Operand::entry(second_entry)));
            assert_eq!(
                outer.selected(decode(first_entry)).unwrap_err().status,
                native::INVALID_REQUEST
            );
        }
        RESULTS.with(|results| assert!(results.borrow().is_empty()));
    }
}
