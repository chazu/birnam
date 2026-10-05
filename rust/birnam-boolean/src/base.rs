//! Initialization takes Namespace and returns true (the Boolean prototype).
//! Publishes Boolean, False, true, false and nil. No dependency on Block:
//! conditional methods send `value` only to the chosen argument.
//! `installPrinting:` later takes a completed String prototype. Printing always
//! sends its current `fromBytes:` method; no foreign storage or entry is cached.
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{self as native, Method, NativeEntry, Operand, Outcome, Protocol},
};
use core::{cell::Cell, ffi::c_void, ptr};
use std::sync::atomic::{AtomicPtr, Ordering};
struct State {
    id: Protocol,
    namespace: Oop,
    roots: Oop,
    nil: Oop,
    truth: Oop,
    falsity: Oop,
    printing: Cell<bool>,
}
static INSTANCE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn object(value: Operand) -> Result<Oop, Outcome> {
    value
        .as_object()
        .filter(|p| !p.is_null())
        .ok_or_else(invalid)
}
fn result(value: Result<Operand, Outcome>) -> Outcome {
    match value {
        Ok(v) => Outcome::loaded(v),
        Err(e) => e,
    }
}
unsafe fn state<'a>(context: *mut c_void) -> &'a State {
    unsafe { &*context.cast::<State>() }
}
unsafe extern "C" fn identical(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([argument]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Ok(other) = object(*argument) else {
        return invalid();
    };
    let state = unsafe { state(context) };
    Outcome::loaded(Operand::object(if receiver == other {
        state.truth
    } else {
        state.falsity
    }))
}
unsafe extern "C" fn not(
    context: *mut c_void,
    receiver: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    if count != 0 {
        return invalid();
    }
    let state = unsafe { state(context) };
    Outcome::loaded(Operand::object(if receiver == state.truth {
        state.falsity
    } else if receiver == state.falsity {
        state.truth
    } else {
        state.nil
    }))
}
unsafe fn boolean(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
    and: bool,
) -> Outcome {
    let Some([argument]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Ok(argument) = object(*argument) else {
        return invalid();
    };
    let state = unsafe { state(context) };
    Outcome::loaded(Operand::object(if receiver == state.truth {
        if and { argument } else { receiver }
    } else if receiver == state.falsity {
        if and { receiver } else { argument }
    } else {
        state.nil
    }))
}
unsafe extern "C" fn and(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    unsafe { boolean(context, receiver, args, count, true) }
}
unsafe extern "C" fn or(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    unsafe { boolean(context, receiver, args, count, false) }
}
unsafe fn branch(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
    expected: usize,
    choose_true: bool,
    short_circuit: bool,
) -> Outcome {
    let Some(arguments) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    if count != expected || arguments.iter().any(|v| object(*v).is_err()) {
        return invalid();
    }
    let state = unsafe { state(context) };
    let truth = if receiver == state.truth {
        true
    } else if receiver == state.falsity {
        false
    } else {
        return Outcome::loaded(Operand::object(state.nil));
    };
    if expected == 1 && truth != choose_true {
        return Outcome::loaded(Operand::object(if short_circuit {
            receiver
        } else {
            state.nil
        }));
    }
    let block = arguments[if expected == 2 && !truth { 1 } else { 0 }]
        .as_object()
        .unwrap();
    result(unsafe { state.id.send_named(block, b"value", &[]) })
}
macro_rules! conditional {
    ($name:ident, $count:expr, $truth:expr, $short:expr) => {
        unsafe extern "C" fn $name(
            context: *mut c_void,
            receiver: Oop,
            args: *const Operand,
            count: usize,
        ) -> Outcome {
            unsafe { branch(context, receiver, args, count, $count, $truth, $short) }
        }
    };
}
conditional!(if_true_false, 2, true, false);
conditional!(if_true, 1, true, false);
conditional!(if_false, 1, false, false);
conditional!(and_block, 1, true, true);
conditional!(or_block, 1, false, true);
struct Printing {
    id: Protocol,
    string: Oop,
    nil: Oop,
    truth: Oop,
    falsity: Oop,
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
    let bytes = if receiver == printing.truth {
        b"true".as_slice()
    } else if receiver == printing.falsity {
        b"false".as_slice()
    } else {
        return Outcome::loaded(Operand::object(printing.nil));
    };
    result(unsafe {
        printing
            .id
            .send_named(printing.string, b"fromBytes:", &[Operand::bytes(bytes)])
    })
}
unsafe extern "C" fn install_printing(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let state = unsafe { state(context) };
    if receiver != state.truth || state.printing.get() {
        return invalid();
    }
    let Some([string]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Ok(string) = object(*string) else {
        return invalid();
    };
    result((|| unsafe {
        let behavior = object(state.id.send_named(
            state.id.receiver,
            b"behaviorOf:",
            &[Operand::object(receiver)],
        )?)?;
        let selector = state.id.intern_name(b"printString")?;
        if state
            .id
            .send_named(behavior, b"lookup", &[Operand::object(selector)])?
            .kind()
            == native::UNIT
        {
            state
                .id
                .send_named(state.roots, b"retain:", &[Operand::object(string)])?;
            let printing = Box::into_raw(Box::new(Printing {
                id: state.id,
                string,
                nil: state.nil,
                truth: state.truth,
                falsity: state.falsity,
            }));
            state.id.send_named(
                behavior,
                b"addMethod",
                &[
                    Operand::object(selector),
                    Operand::entry(NativeEntry::new(print, printing.cast())),
                ],
            )?;
        }
        state.printing.set(true);
        Ok(Operand::unit())
    })())
}
unsafe fn construct(id: Protocol, namespace: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let roots = object(id.send_named(
            namespace,
            b"at:",
            &[Operand::object(id.intern_name(b"Roots")?)],
        )?)?;
        let prototype = id.send_named(id.receiver, b"objectPrototype", &[])?;
        let parent = object(id.send_named(id.receiver, b"behaviorOf:", &[prototype])?)?;
        let retain = |value: Operand| -> Result<Oop, Outcome> {
            let value = object(value)?;
            id.send_named(roots, b"retain:", &[Operand::object(value)])?;
            Ok(value)
        };
        let nil_behavior = retain(id.send_named(parent, b"delegated", &[])?)?;
        let behavior = retain(id.send_named(parent, b"delegated", &[])?)?;
        let nil = retain(id.send_named(nil_behavior, b"allocate", &[Operand::word(0)])?)?;
        let truth = retain(id.send_named(behavior, b"allocate", &[Operand::word(0)])?)?;
        let falsity = retain(id.send_named(behavior, b"allocate", &[Operand::word(0)])?)?;
        let state = Box::into_raw(Box::new(State {
            id,
            namespace,
            roots,
            nil,
            truth,
            falsity,
            printing: Cell::new(false),
        }));
        for (target, name, method) in [
            (parent, b"==".as_slice(), identical as Method),
            (behavior, b"not", not),
            (behavior, b"&", and),
            (behavior, b"|", or),
            (behavior, b"ifTrue:ifFalse:", if_true_false),
            (behavior, b"ifTrue:", if_true),
            (behavior, b"ifFalse:", if_false),
            (behavior, b"and:", and_block),
            (behavior, b"or:", or_block),
            (behavior, b"installPrinting:", install_printing),
        ] {
            id.send_named(
                target,
                b"addMethod",
                &[
                    Operand::object(id.intern_name(name)?),
                    Operand::entry(NativeEntry::new(method, state.cast())),
                ],
            )?;
        }
        for (name, value) in [
            (b"Boolean".as_slice(), truth),
            (b"False", falsity),
            (b"true", truth),
            (b"false", falsity),
            (b"nil", nil),
        ] {
            id.send_named(
                namespace,
                b"at:put:",
                &[
                    Operand::object(id.intern_name(name)?),
                    Operand::object(value),
                ],
            )?;
        }
        INSTANCE.store(state, Ordering::Release);
        Ok(Operand::object(truth))
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
    let Some([namespace]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Ok(namespace) = object(*namespace) else {
        return invalid();
    };
    let id = unsafe { *id };
    let previous = INSTANCE.load(Ordering::Acquire);
    if !previous.is_null() {
        let previous = unsafe { &*previous };
        return if previous.id.same_contract(&id) && previous.namespace == namespace {
            Outcome::loaded(Operand::object(previous.truth))
        } else {
            invalid()
        };
    }
    result(unsafe { construct(id, namespace) })
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: native::VERSION,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};
