//! Base identities and Boolean/identity operations, without String or Block.
use birnam_abi::{Oop, base_values::ValuesV1, root_registry, lifetime,
    id_protocol::{self as native, Operand, Outcome, NativeEntry, Protocol}};
use core::{ffi::c_void, ptr};
use std::sync::atomic::{AtomicPtr, Ordering};
struct State { values: ValuesV1, id_context: *mut c_void }
static BASE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
fn invalid() -> Outcome { Outcome::failed(native::INVALID_REQUEST, 0) }
fn object(value: Oop) -> Outcome { Outcome::loaded(Operand::object(value)) }
unsafe fn values(context: *mut c_void) -> ValuesV1 { unsafe { *context.cast::<ValuesV1>() } }
unsafe extern "C" fn identical(context: *mut c_void, receiver: Oop, args: *const Operand, count: usize) -> Outcome {
    let Some([argument]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid(); };
    let Some(other) = argument.as_object() else { return invalid(); };
    let values = unsafe { values(context) };
    object(if receiver == other { values.true_object } else { values.false_object })
}
unsafe extern "C" fn not(context: *mut c_void, receiver: Oop, _: *const Operand, count: usize) -> Outcome {
    if count != 0 { return invalid(); }
    let values = unsafe { values(context) };
    object(if receiver == values.true_object { values.false_object } else if receiver == values.false_object { values.true_object } else { values.nil })
}
unsafe fn boolean(context: *mut c_void, receiver: Oop, args: *const Operand, count: usize, and: bool) -> Outcome {
    let Some([argument]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid(); };
    let Some(argument) = argument.as_object() else { return invalid(); };
    let values = unsafe { values(context) };
    object(if receiver == values.true_object { if and { argument } else { receiver } }
        else if receiver == values.false_object { if and { receiver } else { argument } }
        else { values.nil })
}
unsafe extern "C" fn and(context: *mut c_void, receiver: Oop, args: *const Operand, count: usize) -> Outcome { unsafe { boolean(context, receiver, args, count, true) } }
unsafe extern "C" fn or(context: *mut c_void, receiver: Oop, args: *const Operand, count: usize) -> Outcome { unsafe { boolean(context, receiver, args, count, false) } }
fn send(id: Protocol, receiver: Oop, selector: Oop, args: &[Operand]) -> Result<Operand, Outcome> {
    let answer = unsafe { id.message.unwrap()(id.context, receiver, selector, args.as_ptr(), args.len()) };
    if answer.status == native::LOADED { Ok(answer.value) } else { Err(answer) }
}
fn intern(id: Protocol, name: &[u8]) -> Result<Oop, Outcome> {
    send(id, id.receiver, id.intern, &[Operand::bytes(name)])?.as_object().ok_or_else(invalid)
}
fn construct(id: Protocol, roots: root_registry::OwnedV1, records: lifetime::OwnedV1) -> Result<*const ValuesV1, Outcome> {
    let prototype = send(id, id.receiver, intern(id, b"objectPrototype")?, &[])?;
    let behavior = send(id, id.receiver, intern(id, b"behaviorOf:")?, &[prototype])?.as_object().ok_or_else(invalid)?;
    let delegated = intern(id, b"delegated")?;
    let allocate = intern(id, b"allocate")?;
    let retain = |value: Operand| -> Result<Oop, Outcome> {
        let value = value.as_object().filter(|value| !value.is_null()).ok_or_else(invalid)?;
        // Establish ownership before publishing this reference in module state.
        assert_ne!(unsafe { roots.retain.unwrap()(roots.context, value) }, 0);
        Ok(value)
    };
    let nil_behavior = retain(send(id, behavior, delegated, &[])?)?;
    let boolean_behavior = retain(send(id, behavior, delegated, &[])?)?;
    for vtable in [nil_behavior, boolean_behavior] {
        unsafe { birnam_abi::payload_layout::set(records.layouts, vtable, ptr::null(), 0, birnam_abi::SLOT_RAW) };
    }
    let nil = retain(send(id, nil_behavior, allocate, &[Operand::word(0)])?)?;
    let true_object = retain(send(id, boolean_behavior, allocate, &[Operand::word(0)])?)?;
    let false_object = retain(send(id, boolean_behavior, allocate, &[Operand::word(0)])?)?;
    let state = Box::into_raw(Box::new(State { values: ValuesV1 { version: 1, byte_size: size_of::<ValuesV1>(),
        nil_behavior, boolean_behavior, nil, true_object, false_object }, id_context: id.context }));
    let values = unsafe { &raw mut (*state).values };
    let add = intern(id, b"addMethod")?;
    for (vtable, name, method) in [
        (behavior, b"==".as_slice(), identical as native::Method),
        (boolean_behavior, b"not".as_slice(), not as native::Method),
        (boolean_behavior, b"&".as_slice(), and as native::Method),
        (boolean_behavior, b"|".as_slice(), or as native::Method),
    ] {
        send(id, vtable, add, &[Operand::object(intern(id, name)?), Operand::entry(NativeEntry::new(method, values.cast()))])?;
    }
    BASE.store(state, Ordering::Release);
    Ok(values)
}
unsafe extern "C" fn initialize(id: *const Protocol, args: *const Operand, count: usize) -> Outcome {
    if id.is_null() { return invalid(); }
    let Some([roots, records]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid(); };
    let Some((roots, records)) = roots.as_word().zip(records.as_word()) else { return invalid(); };
    let roots = roots as *const root_registry::OwnedV1;
    let records = records as *const lifetime::OwnedV1;
    if !unsafe { root_registry::valid_owned(roots) && lifetime::valid(records) } { return invalid(); }
    let id = unsafe { *id };
    let existing = BASE.load(Ordering::Acquire);
    if !existing.is_null() {
        if unsafe { (*existing).id_context } != id.context { return invalid(); }
        return Outcome::loaded(Operand::word(unsafe { &raw const (*existing).values } as usize));
    }
    match construct(id, unsafe { *roots }, unsafe { *records }) {
        Ok(values) => Outcome::loaded(Operand::word(values as usize)), Err(error) => error,
    }
}
pub(super) fn matches(api: &birnam_abi::IdApi) -> bool {
    let base = BASE.load(Ordering::Acquire);
    if base.is_null() { return false; }
    let values = unsafe { (*base).values };
    (values.nil, values.true_object, values.false_object) == (api.nilObj, api.trueObj, api.falseObj)
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module { version: 1, byte_size: size_of::<native::Module>() as u32, initialize: Some(initialize) };
