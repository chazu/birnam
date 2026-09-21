//! The real Id carrier: only Id-owned state and the native loading protocol.
pub(crate) mod objects;
mod storage;
mod registries;

use crate::abi::{
    Oop,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use crate::{
    id_dispatch, initial_objects::InitialObjects, object_memory::ObjectMemory, symbol::Symbols,
    vtable::Vtables,
};
use core::ffi::c_void;
use std::sync::atomic::{AtomicPtr, AtomicU8, Ordering};

pub(crate) struct State {
    pub(crate) protocol: Protocol,
    memory: *mut ObjectMemory,
    _memory: crate::owned::Owned<ObjectMemory>,
    dictionaries: *mut Vtables,
    symbols: *mut Symbols,
    _symbols: crate::owned::Owned<Symbols>,
    _dictionaries: crate::owned::Owned<Vtables>,
    symbol_vtable: Oop,
    object_prototype: Oop,
    lookup: Oop,
    does_not_understand: Oop,
    owner: std::thread::ThreadId,
}
impl State {
    pub(crate) unsafe fn new(
        memory_owner: crate::owned::Owned<ObjectMemory>,
        dictionary_owner: crate::owned::Owned<Vtables>,
        symbol_owner: crate::owned::Owned<Symbols>,
        metavtable: Oop,
        symbol_vtable: Oop,
        object_prototype: Oop,
        lookup: Oop,
        does_not_understand: Oop,
    ) -> Box<Self> {
        let memory = memory_owner.pointer();
        let dictionaries = dictionary_owner.pointer();
        let symbols = symbol_owner.pointer();
        let intern =
            unsafe { (&mut *symbols).intern(&mut *memory, symbol_vtable, b"intern:") }.unwrap();
        let load =
            unsafe { (&mut *symbols).intern(&mut *memory, symbol_vtable, b"loadNative:") }.unwrap();
        let add = unsafe { (&*symbols).existing(b"addMethod") }.unwrap();
        let mut state = Box::new(Self {
            protocol: Protocol {
                version: native::VERSION,
                byte_size: size_of::<Protocol>() as u32,
                message: Some(message),
                context: core::ptr::null_mut(),
                receiver: metavtable,
                intern,
                load,
            },
            memory,
            _memory: memory_owner,
            dictionaries,
            symbols,
            _symbols: symbol_owner,
            _dictionaries: dictionary_owner,
            symbol_vtable,
            object_prototype,
            lookup,
            does_not_understand,
            owner: std::thread::current().id(),
        });
        let context = core::ptr::from_mut(state.as_mut()).cast();
        state.protocol.context = context;
        unsafe {
            id_dispatch::enter(
                dictionaries,
                metavtable,
                lookup,
                does_not_understand,
                || {
                    for (selector, method) in [
                        (intern, intern_bytes as native::Method),
                        (load, load_module as native::Method),
                    ] {
                        assert_eq!(
                            id_dispatch::send(
                                metavtable,
                                add,
                                &[
                                    Operand::object(selector),
                                    Operand::entry(NativeEntry::new(method, context))
                                ]
                            )
                            .status,
                            native::LOADED
                        );
                    }
                    objects::install(&state, context, add);
                    storage::install(&state, context, add);
                    registries::install(&state, context, add);
                },
            )
        };
        state
    }
    pub(crate) unsafe fn enter<R>(&self, operation: impl FnOnce() -> R) -> R {
        unsafe { id_dispatch::enter(self.dictionaries, self.protocol.receiver, self.lookup, self.does_not_understand, operation) }
    }
    #[cfg(test)]
    pub(crate) unsafe fn exhaust_epoch(context: *mut c_void) {
        unsafe { (&mut *(*context.cast::<Self>()).dictionaries).exhaust_mutation_epoch_for_test() };
    }
    pub(crate) fn roots(&self) -> [Oop; 7] {
        [
            self.protocol.receiver,
            self.protocol.intern,
            self.protocol.load,
            self.object_prototype,
            self.symbol_vtable,
            self.lookup,
            self.does_not_understand,
        ]
    }
}

/// # Safety
/// The caller retains the carrier's owner and live arguments for the call.
unsafe extern "C" fn message(
    context: *mut c_void,
    receiver: Oop,
    selector: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let state = unsafe { &*context.cast::<State>() };
    if state.owner != std::thread::current().id() {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    }
    let Some(args) = (unsafe { crate::abi::borrowed_slice(args, count) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    // Copy Id facts out before invoking arbitrary code; no registry reference
    // is held across a nested message or module initializer.
    let (dictionaries, meta, lookup, dnu) = (
        state.dictionaries,
        state.protocol.receiver,
        state.lookup,
        state.does_not_understand,
    );
    unsafe {
        id_dispatch::enter(dictionaries, meta, lookup, dnu, || {
            id_dispatch::send(receiver, selector, args)
        })
    }
}
unsafe extern "C" fn intern_bytes(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some([argument]) = (unsafe { crate::abi::borrowed_slice(args, count) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some(bytes) = argument.as_bytes() else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some(bytes) = (unsafe { crate::abi::borrowed_bytes(bytes.pointer, bytes.length) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let state = unsafe { &*context.cast::<State>() };
    match unsafe { (&mut *state.symbols).intern(&mut *state.memory, state.symbol_vtable, bytes) } {
        Ok(symbol) => Outcome::loaded(Operand::object(symbol)),
        Err(_) => Outcome::failed(native::INVALID_REQUEST, 0),
    }
}
pub(crate) unsafe extern "C" fn load_module(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let Some(arguments) = (unsafe { crate::abi::borrowed_slice(args, count) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some((path, arguments)) = arguments.split_first() else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some(path) = path.as_bytes() else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some(path) = (unsafe { crate::abi::borrowed_bytes(path.pointer, path.length) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let protocol = unsafe { (*context.cast::<State>()).protocol };
    unsafe { crate::abi::native_module::load(&protocol, path, arguments) }
}

// Production initializes Id once. Upper Runtime retains that Id owner; its
// clients never take ownership of the registries. Carrier/context addresses
// stay fixed. Tests construct InitialObjects without this process slot.
static PHASE: AtomicU8 = AtomicU8::new(0);
static PENDING: AtomicPtr<InitialObjects> = AtomicPtr::new(core::ptr::null_mut());
static PROTOCOL: AtomicPtr<Protocol> = AtomicPtr::new(core::ptr::null_mut());
unsafe extern "C" fn initialize(_: *const Protocol, _: *const Operand, count: usize) -> Outcome {
    if count != 0 {
        return Outcome::failed(native::INVALID_REQUEST, count);
    }
    match PHASE.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => {
            let mut initial = Box::new(InitialObjects::new());
            let protocol = core::ptr::from_mut(&mut initial.protocol.protocol);
            PENDING.store(Box::into_raw(initial), Ordering::Release);
            PROTOCOL.store(protocol, Ordering::Release);
            PHASE.store(2, Ordering::Release);
        }
        Err(2) => {}
        Err(_) => return Outcome::failed(native::INITIALIZER_FAILED, 1),
    }
    let protocol = PROTOCOL.load(Ordering::Acquire);
    let state = unsafe { &*(*protocol).context.cast::<State>() };
    if state.owner != std::thread::current().id() {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    }
    Outcome::loaded(Operand::protocol(protocol))
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: native::VERSION,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};
pub(crate) fn take_initial_objects() -> InitialObjects {
    let outcome = unsafe { initialize(core::ptr::null(), core::ptr::null(), 0) };
    assert_eq!(outcome.status, native::LOADED, "Id construction failed");
    let pending = PENDING.swap(core::ptr::null_mut(), Ordering::AcqRel);
    crate::boundary::require_invariant(!pending.is_null(), "Rust id_init called more than once");
    unsafe { *Box::from_raw(pending) }
}
