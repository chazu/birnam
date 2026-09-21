//! Nil predicates on already completed base identities; no String dependency.
use birnam_abi::{Oop, base_values::ValuesV1, id_protocol::{self as native, Operand, Outcome, Protocol, NativeEntry}};
use core::{ffi::c_void, ptr};
use std::sync::atomic::{AtomicPtr, Ordering};
struct State { values: *const ValuesV1, id: Protocol }
static BASE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
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
    if !previous.is_null() { return if unsafe { (*previous).values } == values { Outcome::loaded(Operand::unit()) } else { invalid() }; }
    let id = unsafe { *id };
    let send = |receiver, selector, args: &[Operand]| {
        let answer = unsafe { id.message.unwrap()(id.context, receiver, selector, args.as_ptr(), args.len()) };
        if answer.status == native::LOADED { Ok(answer.value) } else { Err(answer) }
    };
    let state = Box::into_raw(Box::new(State { values, id }));
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
        send(unsafe { (*values).nil_behavior }, add, &[Operand::object(intern(b"installPrinting:")?), Operand::entry(NativeEntry::new(install_printing, state.cast()))])?;
        Ok(())
    };
    match install() { Ok(()) => { BASE.store(state, Ordering::Release); Outcome::loaded(Operand::unit()) }, Err(error) => error }
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module { version: 1, byte_size: size_of::<native::Module>() as u32, initialize: Some(initialize) };

fn send(id: Protocol, receiver: Oop, selector: Oop, args: &[Operand]) -> Result<Operand, Outcome> {
    unsafe { id.message.unwrap()(id.context, receiver, selector, args.as_ptr(), args.len()) }.into_result()
}
fn intern(id: Protocol, name: &[u8]) -> Result<Oop, Outcome> {
    send(id, id.receiver, id.intern, &[Operand::bytes(name)])?.as_object().ok_or_else(invalid)
}

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
    let _ = receiver;
    let text = b"nil";
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
    let values = unsafe { *state.values };
    if receiver != values.nil || printing_ready() {
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
            values.nil_behavior,
            intern(id, b"lookup")?,
            &[Operand::object(selector)],
        )?
        .kind()
            == native::UNIT
        {
            send(
                id,
                values.nil_behavior,
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
