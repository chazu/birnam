//! Base identities and Boolean/identity operations, without String or Block.
use birnam_abi::{Oop, base_values::ValuesV1, root_registry, lifetime,
    id_protocol::{self as native, Operand, Outcome, NativeEntry, Protocol}};
use core::{ffi::c_void, ptr};
use std::sync::atomic::{AtomicPtr, Ordering};
struct State { values: ValuesV1, id: Protocol }
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
fn construct(id: Protocol, roots: root_registry::OwnedV1, records: lifetime::OwnedV2) -> Result<*const ValuesV1, Outcome> {
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
        nil_behavior, boolean_behavior, nil, true_object, false_object }, id }));
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
    send(id, boolean_behavior, add, &[Operand::object(intern(id, b"installPrinting:")?), Operand::entry(NativeEntry::new(install_printing, state.cast()))])?;
    BASE.store(state, Ordering::Release);
    Ok(values)
}
unsafe extern "C" fn initialize(id: *const Protocol, args: *const Operand, count: usize) -> Outcome {
    if id.is_null() { return invalid(); }
    let Some([roots, records]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid(); };
    let Some((roots, records)) = roots.as_word().zip(records.as_word()) else { return invalid(); };
    let roots = roots as *const root_registry::OwnedV1;
    let records = records as *const lifetime::OwnedV2;
    if !unsafe { root_registry::valid_owned(roots) && lifetime::valid(records) } { return invalid(); }
    let id = unsafe { *id };
    let existing = BASE.load(Ordering::Acquire);
    if !existing.is_null() {
        if unsafe { (*existing).id.context } != id.context { return invalid(); }
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

struct Printing {
    id: Protocol,
    string: Oop,
    from_bytes: Oop,
    values: ValuesV1,
}
static PRINTING: AtomicPtr<Printing> = AtomicPtr::new(ptr::null_mut());
pub(super) fn printing_ready() -> bool {
    !PRINTING.load(Ordering::Acquire).is_null()
}
unsafe extern "C" fn printing_roots(mark: birnam_abi::MarkFn) {
    let printing = PRINTING.load(Ordering::Acquire);
    if printing.is_null() {
        return;
    }
    let printing = unsafe { &*printing };
    let objects = [
        printing.id.receiver,
        printing.id.intern,
        printing.id.load,
        printing.string,
        printing.from_bytes,
        printing.values.nil,
        printing.values.true_object,
        printing.values.false_object,
    ];
    for object in objects {
        unsafe { mark(object) };
    }
}
unsafe extern "C" fn print(
    context: *mut c_void,
    receiver: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    if count != 0 {
        return invalid();
    }
    let printing = unsafe { &*context.cast::<Printing>() };
    let text = if receiver == printing.values.true_object {
        b"true".as_slice()
    } else if receiver == printing.values.false_object {
        b"false".as_slice()
    } else {
        return Outcome::loaded(Operand::object(printing.values.nil));
    };
    let bytes = Operand::bytes(text);
    unsafe {
        printing.id.message.unwrap()(
            printing.id.context,
            printing.string,
            printing.from_bytes,
            &bytes,
            1,
        )
    }
}
unsafe extern "C" fn install_printing(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let state = unsafe { &*context.cast::<State>() };
    let values = state.values;
    if receiver != values.true_object || printing_ready() {
        return invalid();
    }
    let Some([roots, string]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else {
        return invalid();
    };
    let (Some(roots), Some(string)) = (roots.as_word(), string.as_word()) else {
        return invalid();
    };
    let roots = roots as *const birnam_abi::root_registry::OwnedV1;
    let string = string as *const birnam_abi::string::StringApiV1;
    if !unsafe {
        birnam_abi::root_registry::valid_owned(roots) && birnam_abi::string::valid(string)
    } {
        return invalid();
    }
    let id = state.id;
    let result = (|| {
        let from_bytes = intern(id, b"fromBytes:")?;
        let selector = intern(id, b"printString")?;
        let printing = Box::into_raw(Box::new(Printing {
            id,
            values,
            string: unsafe { (*string).prototype },
            from_bytes,
        }));
        PRINTING.store(printing, Ordering::Release);
        unsafe { birnam_abi::root_registry::register((*roots).api, printing_roots) };
        if send(
            id,
            values.boolean_behavior,
            intern(id, b"lookup")?,
            &[Operand::object(selector)],
        )?
        .kind()
            == native::UNIT
        {
            send(
                id,
                values.boolean_behavior,
                intern(id, b"addMethod")?,
                &[
                    Operand::object(selector),
                    Operand::entry(NativeEntry::new(print, printing.cast())),
                ],
            )?;
        }
        Ok(Operand::unit())
    })();
    match result {
        Ok(value) => Outcome::loaded(value),
        Err(error) => error,
    }
}
