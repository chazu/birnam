//! Upper Runtime bridge from Object-valued C methods to Id's typed entries.
//! Lookup answers owned Method objects or nil. No raw result convention crosses
//! this bridge; Id itself stores and invokes exact native entries.
use crate::abi::{
    Method, Oop,
    id_protocol::{self as native, NativeEntry, Operand, Outcome},
};
use crate::raw::MethodAddress;
use crate::boundary;
use core::ffi::c_void;

pub(crate) fn encode(method: Method, lookup: bool) -> NativeEntry {
    let address = method as usize;
    let invoke = if lookup || address == lookup_method as Method as usize {
        invoke_lookup as native::Method
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
    [invoke_object as native::Method, invoke_lookup]
        .iter()
        .any(|method| address == Some(*method as usize))
        .then(|| unsafe { MethodAddress::from_oop(entry.context) })
        .flatten()
}

/// Argument adaptation at the Object-valued boundary. The selected
/// native function and context are retained; only the operand carrier changes.
pub(crate) fn native_arguments(
    entry: NativeEntry,
    arguments: &[Oop],
    runtime: &crate::runtime::Runtime,
) -> Vec<Operand> {
    let address = entry.method.map(|method| method as usize);
    if (address == Some(crate::runtime::id_protocol::add_method as native::Method as usize)
        || runtime.is_id_add_method(entry))
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
    arguments.iter().copied().map(Operand::object).collect()
}

/// Lookup converts its owned Method answer to Id's native entry result.
pub(crate) fn returns_entry(entry: NativeEntry) -> bool {
    entry.method.is_some_and(|method| method as usize == invoke_lookup as native::Method as usize)
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
    let converted: Option<Vec<_>> = args.iter().map(|arg| arg.as_object()).collect();
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
unsafe extern "C" fn invoke_lookup(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    match unsafe { invoke(context, receiver, args, count) } {
        Ok(result) => crate::runtime::with_active(|runtime| {
            if let Some(entry) = runtime.owned_method_entry(result) {
                Outcome::loaded(Operand::entry(entry))
            } else if result == runtime.canonical().nil {
                Outcome::loaded(Operand::unit())
            } else {
                // Lookup returns an owned Method or nil. No address recovery,
                // context guessing, or special allowance for native C results.
                Outcome::failed(native::INVALID_REQUEST, 0)
            }
        }),
        Err(error) => error,
    }
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
        runtime.native_language_outcome(Outcome::loaded(entry.map(Operand::entry).unwrap_or_else(Operand::unit)))
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{dispatch::build_bootstrap, runtime::with_active};

    unsafe extern "C" fn identity(receiver: Oop, _: *const Oop, _: usize) -> Oop { receiver }
    unsafe extern "C" fn replacement(_: Oop, _: *const Oop, _: usize) -> Oop {
        with_active(|runtime| runtime.retain_method(encode(identity, false)))
    }
    unsafe extern "C" fn select_then_replace(receiver: Oop, args: *const Oop, _: usize) -> Oop {
        with_active(|runtime| {
            let selector = unsafe { args.read() };
            let selected = unsafe { lookup_method(receiver, args, 1) };
            runtime.export(b"selectedLookupTestMethod", selected).unwrap();
            runtime.add_method(receiver, selector, identity);
            let nested = unsafe { invoke_lookup(MethodAddress::from_method(replacement).as_oop(), receiver, core::ptr::null(), 0) };
            assert_eq!(nested.value.as_entry(), Some(encode(identity, false)));
            runtime.gc_collect();
            selected
        })
    }
    #[test]
    fn owned_lookup_keeps_exact_context_across_nesting_replacement_and_collection() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.install_method_objects();
        let behavior = runtime.create_vtable(runtime.globals().object_vt);
        runtime.export(b"selectedLookupTestBehavior", behavior).unwrap();
        let selector = runtime.intern(b"ownedAllocatorProbe").unwrap();
        let allocator = NativeEntry::new(crate::runtime::id_protocol::allocate, core::ptr::from_ref(runtime).cast_mut().cast());
        runtime.add_native_method(behavior, selector, allocator);
        runtime.enter(|| {
            let selected = unsafe { invoke_lookup(MethodAddress::from_method(select_then_replace).as_oop(),
                behavior, [Operand::object(selector)].as_ptr(), 1) };
            assert_eq!(selected.status, native::LOADED);
            let entry = selected.value.as_entry().unwrap();
            assert_eq!(entry, allocator);
            let allocated = unsafe { entry.method.unwrap()(entry.context, behavior, [Operand::word(24)].as_ptr(), 1) };
            assert_eq!(allocated.status, native::LOADED);
            assert_eq!(runtime.checked_vtable_of(allocated.value.as_object().unwrap()), behavior);
            assert_eq!(unsafe { runtime.lookup_native_method_active(behavior, selector) }, Some(encode(identity, false)));
        });
    }
    #[test]
    fn owned_lookup_distinguishes_identical_functions_with_different_contexts() {
        let first = build_bootstrap();
        let second = build_bootstrap();
        let runtime = &first.runtime;
        runtime.install_method_objects();
        runtime.enter(|| {
            for owner in [&first.runtime, &second.runtime] {
                let entry = NativeEntry::new(crate::runtime::id_protocol::allocate,
                    core::ptr::from_ref(owner).cast_mut().cast());
                let object = runtime.retain_method(entry);
                let selected = unsafe { invoke_lookup(MethodAddress::from_method(identity).as_oop(),
                    object, core::ptr::null(), 0) };
                assert_eq!(selected.status, native::LOADED);
                assert_eq!(selected.value.as_entry(), Some(entry));
            }
        });
    }
    unsafe extern "C" fn raw_answer(_: Oop, _: *const Oop, _: usize) -> Oop {
        MethodAddress::from_method(identity).as_oop()
    }
    #[test]
    fn lookup_rejects_raw_function_addresses() {
        let bootstrap = build_bootstrap();
        let runtime = &bootstrap.runtime;
        runtime.enter(|| {
            let outcome = unsafe { invoke_lookup(MethodAddress::from_method(raw_answer).as_oop(),
                runtime.globals().object_vt, core::ptr::null(), 0) };
            assert_eq!(outcome.status, native::INVALID_REQUEST);
        });
    }
}
