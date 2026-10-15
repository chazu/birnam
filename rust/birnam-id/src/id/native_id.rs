//! The real Id carrier: only Id-owned state and the native loading protocol.
pub(crate) mod objects;
mod storage;
mod registries;
#[cfg(feature="dispatch-metrics")]
mod metrics;

use crate::abi::{
    Oop,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use super::{
    id_dispatch, initial_objects::InitialObjects, object_memory::ObjectMemory, symbol::Symbols,
    vtable::Vtables,
};
use core::{cell::{Cell, RefCell}, ffi::c_void};
use std::sync::atomic::{AtomicPtr, AtomicU8, Ordering};

// Rust's ThreadId is immutable for a thread's lifetime. Keep that value in
// thread-local storage instead of cloning/dropping the Thread handle for every
// Id message. Ownership is still checked at message entry and Id initialization.
std::thread_local! {
    static CURRENT_THREAD_ID: std::thread::ThreadId = std::thread::current().id();
}
#[inline]
fn current_thread_id() -> std::thread::ThreadId {
    CURRENT_THREAD_ID.with(|id| *id)
}

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
    method_change_observer: Cell<Oop>,
    method_changed: Oop,
    detached: RefCell<std::collections::HashMap<usize, super::object_memory::Allocation>>,
    visits: [Oop; 3],
    owner: std::thread::ThreadId,
    #[cfg(feature="dispatch-metrics")]
    metrics: RefCell<metrics::Metrics>,
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
        let method_changed = unsafe { (&mut *symbols).intern(&mut *memory, symbol_vtable, b"methodChanged:on:") }.unwrap();
        let visits = [b"visit:".as_slice(), b"allocation:bytes:identity:", b"binding:value:weak:"]
            .map(|name| unsafe { (&mut *symbols).intern(&mut *memory, symbol_vtable, name) }.unwrap());
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
            method_change_observer: Cell::new(core::ptr::null_mut()),
            method_changed,
            detached: RefCell::new(std::collections::HashMap::new()),
            visits,
            owner: current_thread_id(),
            #[cfg(feature="dispatch-metrics")]
            metrics: RefCell::new(metrics::Metrics::default()),
        });
        let context = core::ptr::from_mut(state.as_mut()).cast();
        state.protocol.context = context;
        unsafe {
            for (selector, method) in [
                (intern, intern_bytes as native::Method),
                (load, load_module as native::Method),
            ] {
                assert_eq!(
                    id_dispatch::send(
                        state.dispatch_context(), metavtable, add,
                        &[Operand::object(selector), Operand::entry(NativeEntry::new(method, context))],
                    ).status,
                    native::LOADED,
                );
            }
            objects::install(&state, context, add);
            storage::install(&state, context, add);
            registries::install(&state, context, add);
            #[cfg(feature="dispatch-metrics")]
            metrics::install(&state, context, add);
        }
        state
    }
    pub(crate) fn dispatch_context(&self) -> id_dispatch::Context {
        id_dispatch::Context {
            dictionaries: self.dictionaries,
            metavtable: self.protocol.receiver,
            lookup: self.lookup,
            does_not_understand: self.does_not_understand,
        }
    }
    pub(crate) fn roots(&self) -> [Oop; 12] {
        [
            self.protocol.receiver,
            self.protocol.intern,
            self.protocol.load,
            self.object_prototype,
            self.symbol_vtable,
            self.lookup,
            self.does_not_understand,
            self.method_change_observer.get(),
            self.method_changed,
            self.visits[0],
            self.visits[1],
            self.visits[2],
        ]
    }
    // Id reports dictionary mutation without knowing Store, execution policy or
    // native callback addresses. Drop dictionary borrows before this live send.
    unsafe fn method_changed(&self, selector: Oop, behavior: Oop) -> Result<(), Outcome> {
        let observer = self.method_change_observer.get();
        if !observer.is_null() {
            unsafe { self.protocol.send(observer, self.method_changed, &[Operand::object(selector), Operand::object(behavior)]) }?;
        }
        Ok(())
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
    if state.owner != current_thread_id() {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    }
    let Some(args) = (unsafe { crate::abi::borrowed_slice(args, count) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    #[cfg(feature="dispatch-metrics")]
    let tracked=metrics::enter(state,receiver,selector,args);
    // No registry reference spans a callback or nested message.
    let result=unsafe { id_dispatch::send(state.dispatch_context(), receiver, selector, args) };
    #[cfg(feature="dispatch-metrics")]
    metrics::leave(state,receiver,selector,args,&result,tracked);
    result
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

// Production initializes and retains Id for process life, like its native image.
// Upper constructors borrow the protocol; no private Rust owner crosses the boundary.
// Tests may separately own an InitialObjects instance.
static PHASE: AtomicU8 = AtomicU8::new(0);
static PROTOCOL: AtomicPtr<Protocol> = AtomicPtr::new(core::ptr::null_mut());
unsafe extern "C" fn initialize(_: *const Protocol, _: *const Operand, count: usize) -> Outcome {
    if count != 0 {
        return Outcome::failed(native::INVALID_REQUEST, count);
    }
    match PHASE.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => {
            let initial = InitialObjects::new();
            let state = Box::leak(initial.protocol);
            let protocol = core::ptr::from_mut(&mut state.protocol);
            PROTOCOL.store(protocol, Ordering::Release);
            PHASE.store(2, Ordering::Release);
        }
        Err(2) => {}
        Err(_) => return Outcome::failed(native::INITIALIZER_FAILED, 1),
    }
    let protocol = PROTOCOL.load(Ordering::Acquire);
    let state = unsafe { &*(*protocol).context.cast::<State>() };
    if state.owner != current_thread_id() {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    }
    Outcome::loaded(Operand::protocol(protocol))
}
pub(crate) const DESCRIPTOR: native::Module = native::Module {
    version: native::VERSION,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};

#[cfg(test)]
mod thread_identity_tests {
    use super::current_thread_id;
    #[test]
    fn cached_thread_identity_agrees_and_never_crosses_thread_lifetimes() {
        let parent=current_thread_id();
        assert_eq!(parent,std::thread::current().id());
        let mut seen=std::collections::HashSet::new();
        seen.insert(parent);
        for _ in 0..32 {
            let child=std::thread::spawn(|| {
                let id=current_thread_id();
                for _ in 0..100 {
                    assert_eq!(current_thread_id(),std::thread::current().id());
                }
                id
            }).join().unwrap();
            assert!(seen.insert(child),"a new thread must not inherit an old thread identity");
            assert_eq!(current_thread_id(),parent);
        }
    }
}
