//! Upper Runtime adaptation of the existing all-Oop native Method ABI.
//! Id itself stores and invokes typed entries. Remove this adapter when upper
//! native consumers have migrated; it is not part of independent Id.
use crate::abi::{
    Method, Oop,
    id_protocol::{self as native, NativeEntry, Operand, Outcome},
};
use crate::raw::MethodAddress;
use crate::{boundary, id_dispatch};
use core::ffi::c_void;

pub(crate) fn encode(method: Method, lookup: bool) -> NativeEntry {
    let address = method as usize;
    for (old, new) in [
        (
            lookup_method as Method,
            id_dispatch::vtable_lookup_method as native::Method,
        ),
        (
            add_method as Method,
            id_dispatch::vtable_add_method as native::Method,
        ),
        (
            remove_method as Method,
            id_dispatch::vtable_remove_method as native::Method,
        ),
    ] {
        if address == old as usize {
            return id_dispatch::entry(new);
        }
    }
    let invoke = if lookup {
        invoke_lookup as native::Method
    } else if address == crate::dispatch::vtable_add_method as *const () as usize
        || address == crate::dispatch::vtable_remove_method as *const () as usize
    {
        invoke_entry as native::Method
    } else {
        invoke_object as native::Method
    };
    NativeEntry::new(invoke, MethodAddress::from_method(method).as_oop())
}
pub(crate) fn decode(entry: NativeEntry) -> MethodAddress {
    try_decode(entry).unwrap_or_else(|| boundary::fatal_invariant("typed native entry has no legacy Method adapter"))
}
pub(crate) fn try_decode(entry: NativeEntry) -> Option<MethodAddress> {
    let address = entry.method.map(|method| method as usize);
    for method in [invoke_object as native::Method, invoke_lookup, invoke_entry] {
        if address == Some(method as usize) {
            return unsafe { MethodAddress::from_oop(entry.context) };
        }
    }
    for (new, old) in [
        (crate::runtime::id_protocol::add_method as native::Method, crate::dispatch::vtable_add_method as Method),
        (crate::runtime::id_protocol::remove_method as native::Method, crate::dispatch::vtable_remove_method as Method),
    ] {
        if address == Some(new as usize) { return Some(MethodAddress::from_method(old)); }
    }
    for (new, old) in [
        (
            id_dispatch::vtable_lookup_method as native::Method,
            lookup_method as Method,
        ),
        (
            id_dispatch::vtable_add_method as native::Method,
            add_method as Method,
        ),
        (
            id_dispatch::vtable_remove_method as native::Method,
            remove_method as Method,
        ),
    ] {
        if address == Some(new as usize) && entry.context.is_null() {
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
                .or_else(|| arg.as_entry().map(|entry| decode(entry).as_oop()))
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
    match unsafe { invoke(context, receiver, args, count) } {
        Ok(result) => Outcome::loaded(
            unsafe { MethodAddress::from_oop(result) }
                .map(|method| Operand::entry(encode(method.to_method(), lookup)))
                .unwrap_or_else(Operand::unit),
        ),
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
fn old_result(result: Outcome) -> Oop {
    if result.status != native::LOADED {
        boundary::fatal_invariant("Id primitive rejected legacy Method arguments");
    }
    if result.value.kind() == native::UNIT {
        return core::ptr::null_mut();
    }
    decode(
        result
            .value
            .as_entry()
            .expect("Id primitive returned a non-entry"),
    )
    .as_oop()
}
pub(crate) unsafe extern "C" fn lookup_method(
    receiver: Oop,
    args: *const Oop,
    count: usize,
) -> Oop {
    if count == 0 || args.is_null() {
        boundary::fatal_invariant("lookup requires selector");
    }
    old_result(unsafe {
        id_dispatch::vtable_lookup_method(
            core::ptr::null_mut(),
            receiver,
            &[Operand::object(args.read())] as *const Operand,
            1,
        )
    })
}
pub(crate) unsafe extern "C" fn add_method(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    if count < 2 || args.is_null() {
        boundary::fatal_invariant("addMethod requires selector and method");
    }
    let method = unsafe { MethodAddress::from_oop(args.add(1).read()) }.expect("null method");
    let converted = [
        Operand::object(unsafe { args.read() }),
        Operand::entry(encode(method.to_method(), false)),
    ];
    old_result(unsafe {
        id_dispatch::vtable_add_method(core::ptr::null_mut(), receiver, converted.as_ptr(), 2)
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
    old_result(unsafe {
        id_dispatch::vtable_remove_method(
            core::ptr::null_mut(),
            receiver,
            &[Operand::object(args.read())] as *const Operand,
            1,
        )
    })
}
