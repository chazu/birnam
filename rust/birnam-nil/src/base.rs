//! Takes Namespace, installs predicates on Object and nil, returns Unit.
//! `installPrinting:` takes a completed String Object and installs `printString`
//! without overwriting an existing method. Printing sends `fromBytes:` live.
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
unsafe fn predicate(context: *mut c_void, count: usize, truth: bool) -> Outcome {
    if count != 0 {
        return invalid();
    }
    let state = unsafe { &*context.cast::<State>() };
    Outcome::loaded(Operand::object(if truth {
        state.truth
    } else {
        state.falsity
    }))
}
unsafe extern "C" fn yes(context: *mut c_void, _: Oop, _: *const Operand, count: usize) -> Outcome {
    unsafe { predicate(context, count, true) }
}
unsafe extern "C" fn no(context: *mut c_void, _: Oop, _: *const Operand, count: usize) -> Outcome {
    unsafe { predicate(context, count, false) }
}
struct Printing {
    id: Protocol,
    string: Oop,
}
unsafe extern "C" fn print(
    context: *mut c_void,
    _: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    if count != 0 {
        return invalid();
    }
    let printing = unsafe { &*context.cast::<Printing>() };
    result(unsafe {
        printing
            .id
            .send_named(printing.string, b"fromBytes:", &[Operand::bytes(b"nil")])
    })
}
unsafe extern "C" fn install_printing(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let state = unsafe { &*context.cast::<State>() };
    if receiver != state.nil || state.printing.get() {
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
        let get = |name: &[u8]| {
            object(id.send_named(namespace, b"at:", &[Operand::object(id.intern_name(name)?)])?)
        };
        let roots = get(b"Roots")?;
        let nil = get(b"nil")?;
        let truth = get(b"true")?;
        let falsity = get(b"false")?;
        // Installed native contexts keep these identities even after rebinding.
        for value in [nil, truth, falsity] {
            id.send_named(roots, b"retain:", &[Operand::object(value)])?;
        }
        let prototype = id.send_named(id.receiver, b"objectPrototype", &[])?;
        let parent = object(id.send_named(id.receiver, b"behaviorOf:", &[prototype])?)?;
        let behavior =
            object(id.send_named(id.receiver, b"behaviorOf:", &[Operand::object(nil)])?)?;
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
            (parent, b"isNil".as_slice(), no as Method),
            (parent, b"notNil", yes),
            (behavior, b"isNil", yes),
            (behavior, b"notNil", no),
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
        INSTANCE.store(state, Ordering::Release);
        Ok(Operand::unit())
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
            Outcome::loaded(Operand::unit())
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
