//! Nil predicates on already completed base identities; no String dependency.
use birnam_abi::{Oop, base_values::ValuesV1, id_protocol::{self as native, Operand, Outcome, Protocol, NativeEntry}};
use core::{ffi::c_void, ptr};
use std::sync::atomic::{AtomicPtr, Ordering};
static BASE: AtomicPtr<ValuesV1> = AtomicPtr::new(ptr::null_mut());
fn invalid() -> Outcome { Outcome::failed(native::INVALID_REQUEST, 0) }
unsafe fn predicate(context: *mut c_void, count: usize, truth: bool) -> Outcome {
    if count != 0 { return invalid(); }
    let base = unsafe { *context.cast::<ValuesV1>() };
    Outcome::loaded(Operand::object(if truth { base.true_object } else { base.false_object }))
}
unsafe extern "C" fn true_predicate(context: *mut c_void, _: Oop, _: *const Operand, count: usize) -> Outcome { unsafe { predicate(context, count, true) } }
unsafe extern "C" fn false_predicate(context: *mut c_void, _: Oop, _: *const Operand, count: usize) -> Outcome { unsafe { predicate(context, count, false) } }
unsafe extern "C" fn initialize(id: *const Protocol, args: *const Operand, count: usize) -> Outcome {
    if id.is_null() { return invalid(); }
    let Some([values]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid(); };
    let Some(values) = values.as_word() else { return invalid(); };
    let values = values as *const ValuesV1;
    if !unsafe { birnam_abi::base_values::valid(values) } { return invalid(); }
    let previous = BASE.load(Ordering::Acquire);
    if !previous.is_null() { return if previous.cast_const() == values { Outcome::loaded(Operand::unit()) } else { invalid() }; }
    let id = unsafe { *id };
    let send = |receiver, selector, args: &[Operand]| {
        let answer = unsafe { id.message.unwrap()(id.context, receiver, selector, args.as_ptr(), args.len()) };
        if answer.status == native::LOADED { Ok(answer.value) } else { Err(answer) }
    };
    let install = || -> Result<(), Outcome> {
        let intern = |name: &[u8]| send(id.receiver, id.intern, &[Operand::bytes(name)])?.as_object().ok_or_else(invalid);
        let prototype = send(id.receiver, intern(b"objectPrototype")?, &[])?;
        let behavior = send(id.receiver, intern(b"behaviorOf:")?, &[prototype])?.as_object().ok_or_else(invalid)?;
        let add = intern(b"addMethod")?;
        for (receiver, name, method) in [
            (behavior, b"isNil".as_slice(), false_predicate as native::Method),
            (behavior, b"notNil".as_slice(), true_predicate as native::Method),
            (unsafe { (*values).nil_behavior }, b"isNil".as_slice(), true_predicate as native::Method),
            (unsafe { (*values).nil_behavior }, b"notNil".as_slice(), false_predicate as native::Method),
        ] {
            send(receiver, add, &[Operand::object(intern(name)?), Operand::entry(NativeEntry::new(method, values.cast_mut().cast()))])?;
        }
        Ok(())
    };
    match install() { Ok(()) => { BASE.store(values.cast_mut(), Ordering::Release); Outcome::loaded(Operand::unit()) }, Err(error) => error }
}
pub(super) fn matches(api: &birnam_abi::IdApi) -> bool {
    let base = BASE.load(Ordering::Acquire);
    if base.is_null() { return false; }
    let base = unsafe { *base };
    (base.nil, base.true_object, base.false_object) == (api.nilObj, api.trueObj, api.falseObj)
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module { version: 1, byte_size: size_of::<native::Module>() as u32, initialize: Some(initialize) };
