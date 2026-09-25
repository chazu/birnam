//! Sequential native loading above Id. Initialization takes the native-library
//! directory as Bytes and returns Namespace. Siphon knows only Id; this module
//! owns the order of the completed language objects built above it.
//!
//! Bootstrap `loadNative:` accepts a path (Bytes), followed by initializer
//! operands. It retains Object inputs across initialization, retains an Object
//! result before releasing the inputs, and returns Object or Unit. `release:`
//! releases one retained occurrence of a result and returns Word 1/0.
use birnam_abi::{
    Oop, borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, Method, NativeEntry, Operand, Outcome, Protocol},
};
use core::{cell::RefCell, ffi::c_void, ptr};
use std::{
    os::unix::ffi::OsStrExt,
    path::Path,
    sync::atomic::{AtomicPtr, Ordering},
};
struct State {
    id: Protocol,
    roots: Oop,
    namespace: Oop,
    directory: Vec<u8>,
    results: RefCell<Vec<(Oop, usize)>>,
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
fn outcome(result: Result<Operand, Outcome>) -> Outcome {
    match result {
        Ok(value) => Outcome::loaded(value),
        Err(error) => error,
    }
}
unsafe fn retain(id: Protocol, roots: Oop, value: Oop) -> Result<usize, Outcome> {
    unsafe { id.send_named(roots, b"retain:", &[Operand::object(value)]) }?
        .as_word()
        .filter(|t| *t != 0)
        .ok_or_else(invalid)
}
unsafe fn release(id: Protocol, roots: Oop, token: usize) -> Result<(), Outcome> {
    match unsafe { id.send_named(roots, b"release:", &[Operand::word(token)]) }?.as_word() {
        Some(1) => Ok(()),
        _ => Err(invalid()),
    }
}
unsafe fn load(state: &State, request: &[Operand]) -> Result<Operand, Outcome> {
    let Some((path, arguments)) = request.split_first() else {
        return Err(invalid());
    };
    let path = path.as_bytes().ok_or_else(invalid)?;
    // Own the path before calling a method that could invalidate borrowed bytes.
    let path = unsafe { borrowed_bytes(path.pointer, path.length) }
        .ok_or_else(invalid)?
        .to_vec();
    let mut input = Vec::new();
    let answer = (|| unsafe {
        for argument in arguments {
            if argument.kind() == native::OBJECT {
                input.push(retain(state.id, state.roots, object(*argument)?)?);
            }
        }
        let mut request = vec![Operand::bytes(&path)];
        request.extend_from_slice(arguments);
        let value = state.id.send(state.id.receiver, state.id.load, &request)?;
        match value.kind() {
            native::UNIT => {}
            native::OBJECT => {
                let object = object(value)?;
                let token = retain(state.id, state.roots, object)?;
                // No Bootstrap storage borrow crosses a dependency send.
                state.results.borrow_mut().push((object, token));
            }
            _ => return Err(invalid()),
        }
        Ok(value)
    })();
    let mut cleanup = Ok(());
    for token in input {
        if let Err(error) = unsafe { release(state.id, state.roots, token) } {
            cleanup = Err(error);
        }
    }
    cleanup?;
    answer
}
unsafe extern "C" fn load_native(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some(arguments) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    outcome(unsafe { load(&*context.cast::<State>(), arguments) })
}
unsafe extern "C" fn release_result(
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
    let state = unsafe { &*context.cast::<State>() };
    let token = {
        let mut results = state.results.borrow_mut();
        let Some(index) = results.iter().position(|(value, _)| *value == object) else {
            return Outcome::loaded(Operand::word(0));
        };
        results.swap_remove(index).1
    };
    match unsafe { release(state.id, state.roots, token) } {
        Ok(()) => Outcome::loaded(Operand::word(1)),
        Err(error) => {
            state.results.borrow_mut().push((object, token));
            error
        }
    }
}
fn module_path(directory: &[u8], name: &str) -> Vec<u8> {
    Path::new(std::ffi::OsStr::from_bytes(directory))
        .join(format!("libbirnam_{name}.dylib"))
        .as_os_str()
        .as_bytes()
        .to_vec()
}
unsafe fn construct(id: Protocol, directory: &[u8]) -> Result<Operand, Outcome> {
    unsafe {
        // Roots is complete before any retaining loader or collector exists.
        // This removes the old early/managed root table adoption stage entirely.
        let roots = object(id.send(
            id.receiver,
            id.load,
            &[Operand::bytes(&module_path(directory, "root_registry"))],
        )?)?;
        let namespace = object(id.send(
            id.receiver,
            id.load,
            &[
                Operand::bytes(&module_path(directory, "namespace")),
                Operand::object(roots),
            ],
        )?)?;
        let prototype = id.send_named(id.receiver, b"objectPrototype", &[])?;
        let parent = object(id.send_named(id.receiver, b"behaviorOf:", &[prototype])?)?;
        let behavior = object(id.send_named(parent, b"delegated", &[])?)?;
        retain(id, roots, behavior)?;
        let receiver = object(id.send_named(behavior, b"allocate", &[Operand::word(0)])?)?;
        for value in [roots, namespace, receiver] {
            retain(id, roots, value)?;
        }
        let state = Box::into_raw(Box::new(State {
            id,
            roots,
            namespace,
            directory: directory.to_vec(),
            results: RefCell::new(Vec::new()),
        }));
        for (name, method) in [
            (b"loadNative:".as_slice(), load_native as Method),
            (b"release:", release_result),
        ] {
            id.send_named(
                behavior,
                b"addMethod",
                &[
                    Operand::object(id.intern_name(name)?),
                    Operand::entry(NativeEntry::new(method, state.cast())),
                ],
            )?;
        }
        // Publish already constructed identities, not descriptors or placeholders.
        for (name, value) in [
            (b"Bootstrap".as_slice(), receiver),
            (b"Object", object(prototype)?),
            (b"ObjectVt", parent),
            (b"Vtable", id.receiver),
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
        for name in ["boolean", "nil"] {
            id.send_named(
                receiver,
                b"loadNative:",
                &[
                    Operand::bytes(&module_path(directory, name)),
                    Operand::object(namespace),
                ],
            )?;
        }
        let memory = object(id.send_named(
            receiver,
            b"loadNative:",
            &[
                Operand::bytes(&module_path(directory, "collect")),
                Operand::object(roots),
            ],
        )?)?;
        id.send_named(
            namespace,
            b"at:put:",
            &[
                Operand::object(id.intern_name(b"GC")?),
                Operand::object(memory),
            ],
        )?;
        id.send_named(memory, b"activate", &[])?;
        id.send_named(
            receiver,
            b"loadNative:",
            &[
                Operand::bytes(&module_path(directory, "object_model")),
                Operand::object(namespace),
            ],
        )?;
        for name in ["string", "error", "integer", "float"] {
            id.send_named(
                receiver,
                b"loadNative:",
                &[
                    Operand::bytes(&module_path(directory, name)),
                    Operand::object(namespace),
                ],
            )?;
        }
        let string = id.send_named(
            namespace,
            b"at:",
            &[Operand::object(id.intern_name(b"String")?)],
        )?;
        for name in [b"true".as_slice(), b"nil"] {
            let value = object(id.send_named(
                namespace,
                b"at:",
                &[Operand::object(id.intern_name(name)?)],
            )?)?;
            id.send_named(value, b"installPrinting:", &[string])?;
        }
        let named = |name: &[u8]| {
            id.send_named(namespace, b"at:", &[Operand::object(id.intern_name(name)?)])
        };
        let integer = named(b"Integer")?;
        let float = named(b"Float")?;
        id.send_named(object(integer)?, b"installFloat:", &[float])?;
        id.send_named(object(string)?, b"installNumbers:", &[integer, float])?;
        // Collection is only a shared parent identity; no installer library is needed.
        let collection = object(id.send_named(parent, b"delegated", &[])?)?;
        let collection_root = retain(id, roots, collection)?;
        for name in [b"Collection".as_slice(), b"CollectionVt"] {
            id.send_named(
                namespace,
                b"at:put:",
                &[
                    Operand::object(id.intern_name(name)?),
                    Operand::object(collection),
                ],
            )?;
        }
        release(id, roots, collection_root)?;
        for name in ["word", "vector", "map", "range", "store"] {
            id.send_named(
                receiver,
                b"loadNative:",
                &[
                    Operand::bytes(&module_path(directory, name)),
                    Operand::object(namespace),
                ],
            )?;
        }
        id.send_named(
            object(string)?,
            b"installCollections:",
            &[named(b"Vector")?],
        )?;
        id.send_named(
            receiver,
            b"loadNative:",
            &[
                Operand::bytes(&module_path(directory, "execution")),
                Operand::object(namespace),
            ],
        )?;
        for name in ["peg", "lang", "tcc", "codegen_runtime"] {
            id.send_named(
                receiver,
                b"loadNative:",
                &[
                    Operand::bytes(&module_path(directory, name)),
                    Operand::object(namespace),
                ],
            )?;
        }
        INSTANCE.store(state, Ordering::Release);
        Ok(Operand::object(namespace))
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
    let Some([directory]) = (unsafe { borrowed_slice(args, count) }) else {
        return invalid();
    };
    let Some(directory) = directory.as_bytes() else {
        return invalid();
    };
    let Some(directory) = (unsafe { borrowed_bytes(directory.pointer, directory.length) }) else {
        return invalid();
    };
    let directory = directory.to_vec();
    let id = unsafe { *id };
    let previous = INSTANCE.load(Ordering::Acquire);
    if !previous.is_null() {
        let previous = unsafe { &*previous };
        return if previous.id.same_contract(&id) && previous.directory == directory {
            Outcome::loaded(Operand::object(previous.namespace))
        } else {
            invalid()
        };
    }
    outcome(unsafe { construct(id, &directory) })
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: native::VERSION,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};
