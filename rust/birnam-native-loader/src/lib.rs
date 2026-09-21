//! System's checked language wrapper around Id's common native loader.
#![allow(clippy::missing_safety_doc)]
use birnam_abi::error_value::numeric_error;
use birnam_abi::id_protocol::{self as native, Operand, Outcome, Protocol};
use birnam_abi::root_registry::ConstructionRoots;
use birnam_abi::{IdApi, Oop, borrowed_oop_args};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

// Completed, process-pinned dependencies. The carrier itself is copied because
// its caller may have supplied it from temporary constructor storage.
struct Runtime {
    api: usize,
    roots: usize,
    presence: usize,
    imports: usize,
    integer: usize,
    id: Protocol,
    system: usize,
}
// All operations run on the Id owner thread; publication is one-time.
unsafe impl Send for Runtime {}
unsafe impl Sync for Runtime {}
static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static PHASE: AtomicU8 = AtomicU8::new(0);
fn invalid() -> Outcome { Outcome::failed(native::INVALID_REQUEST, 0) }
unsafe fn error(runtime: &Runtime, code: &'static [u8], message: &'static [u8]) -> Oop {
    unsafe { numeric_error(&*(runtime.api as *const IdApi), runtime.presence as _, runtime.imports as _, code, message) }
}
unsafe fn send(api: &IdApi, receiver: Oop, selector: &'static core::ffi::CStr, arguments: &[Oop]) -> Oop {
    let selector = unsafe { (api.intern)(selector.as_ptr()) };
    unsafe { (api.send)(receiver, selector, arguments.as_ptr(), arguments.len()) }
}
unsafe extern "C" fn load(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { load_impl(receiver, args, count, false) }
}
unsafe extern "C" fn load_arguments(receiver: Oop, args: *const Oop, count: usize) -> Oop {
    unsafe { load_impl(receiver, args, count, true) }
}
unsafe fn load_impl(receiver: Oop, args: *const Oop, count: usize, explicit: bool) -> Oop {
    let runtime = RUNTIME.get().expect("Native Loader is not constructed");
    let api = unsafe { &*(runtime.api as *const IdApi) };
    let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else { return api.nilObj; };
    if count != if explicit { 2 } else { 1 } { return api.nilObj; }
    let mut roots = unsafe { ConstructionRoots::new(runtime.roots as _) };
    unsafe { roots.keep(receiver); }
    for &object in arguments { unsafe { roots.keep(object); } }
    let Some(path) = (unsafe { birnam_abi::text_view::string_bytes(api, arguments[0]) }) else { return api.nilObj; };
    // Sequence messages can collect or mutate the original String.
    let path = path.to_vec();
    let mut operands = vec![Operand::bytes(&path)];
    if explicit {
        let sequence = arguments[1];
        let length = unsafe { roots.keep(send(api, sequence, c"size", &[])) };
        let integer = unsafe { *(runtime.integer as *const birnam_abi::integer::IntegerApiV1) };
        if unsafe { (integer.is_exact)(integer.context, length) } == 0 {
            return unsafe { error(runtime, b"typeError\0", b"module arguments need an Integer size") };
        }
        let length = unsafe { (integer.read)(integer.context, length) };
        if length < 0 {
            return unsafe { error(runtime, b"typeError\0", b"module argument count must be nonnegative") };
        }
        for index in 0..length {
            let integer = unsafe { *(runtime.integer as *const birnam_abi::integer::IntegerApiV1) };
            let index = unsafe { roots.keep((integer.make)(integer.context, index)) };
            let object = unsafe { roots.keep(send(api, sequence, c"at:", &[index])) };
            operands.push(Operand::object(object));
        }
    }
    // Id owns descriptor admission and pins entered native images, including
    // initialization failures. No upper service bag or private symbol discovery.
    let outcome = unsafe { runtime.id.message.unwrap()(runtime.id.context, runtime.id.receiver, runtime.id.load, operands.as_ptr(), operands.len()) };
    match outcome.status {
        native::LOADED => match outcome.value.kind() {
            native::UNIT => receiver,
            native::OBJECT => outcome.value.as_object().unwrap(),
            _ => unsafe { error(runtime, b"typeError\0", b"native module returned a non-language value") },
        },
        native::MAPPING_FAILED => unsafe { error(runtime, b"ioError\0", b"could not load module dylib") },
        native::MISSING_ENTRY | native::INCOMPATIBLE_ABI => unsafe { error(runtime, b"notAModule\0", b"missing or incompatible birnam_native_module_v1") },
        _ => unsafe { error(runtime, b"moduleError\0", b"native module initialization failed") },
    }
}

unsafe extern "C" fn initialize(id: *const Protocol, args: *const Operand, count: usize) -> Outcome {
    if !unsafe { native::valid_protocol(id) } { return invalid(); }
    let Some([api, roots, system, presence, imports, integer, errors, text]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid(); };
    let (Some(api), Some(roots), Some(system), Some(presence), Some(imports), Some(integer), Some(errors), Some(text)) =
        (api.as_word(), roots.as_word(), system.as_object(), presence.as_word(), imports.as_word(), integer.as_word(), errors.as_word(), text.as_word()) else { return invalid(); };
    let api = api as *const IdApi;
    if api.is_null() || system.is_null() || unsafe { (*api).vtable_vt != (*id).receiver }
        || !unsafe { birnam_abi::root_registry::valid_owned(roots as _)
            && birnam_abi::namespace_presence::valid(presence as _)
            && birnam_abi::namespace_import::valid(imports as _)
            && birnam_abi::integer::valid(integer as _) } { return invalid(); }
    if unsafe { birnam_abi::error_value::attach(errors as _) } != 1
        || unsafe { birnam_abi::text_view::attach(text as _) } != 1 { return invalid(); }
    if RUNTIME.get().is_some_and(|r| r.api != api.addr() || r.system != system.addr()
        || r.roots != roots || r.presence != presence || r.imports != imports || r.integer != integer
        || r.id.context != unsafe { (*id).context }) { return invalid(); }
    match PHASE.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => {}, Err(2) => return Outcome::loaded(Operand::object(system)), _ => return invalid(),
    }
    assert!(RUNTIME.set(Runtime { api: api.addr(), roots, presence, imports, integer, id: unsafe { *id }, system: system.addr() }).is_ok());
    let mut retained = unsafe { ConstructionRoots::new(roots as _) };
    let api = unsafe { &*api };
    let behavior = unsafe { retained.keep((api.vtableOf)(system)) };
    for (name, method) in [(c"loadModule:", load as birnam_abi::Method), (c"loadModule:arguments:", load_arguments as birnam_abi::Method)] {
        let selector = unsafe { retained.keep((api.intern)(name.as_ptr())) };
        unsafe { (api.method)(behavior, selector, method); }
    }
    PHASE.store(2, Ordering::Release);
    Outcome::loaded(Operand::object(system))
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module { version: 1, byte_size: size_of::<native::Module>() as u32, initialize: Some(initialize) };
