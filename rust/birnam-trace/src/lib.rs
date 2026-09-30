//! Declared-graph diagnostics over Storage messages. Never a reclamation decision.
mod graph;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
    ptr,
};
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
struct State {
    id: Protocol,
    ns: Oop,
    storage: Oop,
    receiver: Oop,
    sink_behavior: Oop,
    thread: std::thread::ThreadId,
    active: Cell<bool>,
    sink: Cell<Oop>,
    caller: Cell<Oop>,
    accept: Cell<usize>,
    completed: Cell<bool>,
    graph: RefCell<graph::Graph>,
    failure: Cell<Option<Outcome>>,
}
static INSTANCE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
static INITIALIZING: AtomicBool = AtomicBool::new(false);
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 1)
}
fn object(v: Operand) -> Result<Oop, Outcome> {
    v.as_object().filter(|o| !o.is_null()).ok_or_else(invalid)
}
unsafe fn state<'a>(c: *mut c_void) -> Result<&'a State, Outcome> {
    let s = unsafe { &*c.cast::<State>() };
    if s.thread != std::thread::current().id() {
        return Err(invalid());
    }
    Ok(s)
}
impl State {
    unsafe fn send(&self, r: Oop, n: &[u8], a: &[Operand]) -> Result<Operand, Outcome> {
        unsafe { self.id.send_named(r, n, a) }
    }
}
struct Observation<'a>(&'a State);
impl Drop for Observation<'_> {
    fn drop(&mut self) {
        self.0.sink.set(ptr::null_mut());
        self.0.caller.set(ptr::null_mut());
        *self.0.graph.borrow_mut() = graph::Graph::default();
        self.0.failure.set(None);
        self.0.completed.set(false);
        self.0.active.set(false);
    }
}
unsafe extern "C" fn roots(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let s = state(c)?;
        let Some([visitor]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let visitor = object(*visitor)?;
        for value in [
            s.ns,
            s.storage,
            s.receiver,
            s.sink_behavior,
            s.sink.get(),
            s.caller.get(),
        ]
        .into_iter()
        .filter(|o| !o.is_null())
        {
            s.send(visitor, b"visit:", &[Operand::object(value)])?;
        }
        Ok(Operand::unit())
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
unsafe fn receive(
    c: *mut c_void,
    r: Oop,
    a: *const Operand,
    n: usize,
    op: usize,
) -> Result<Operand, Outcome> {
    unsafe {
        let s = state(c)?;
        let answer = (|| {
            if !s.active.get() || r != s.sink.get() || s.completed.get() {
                return Err(invalid());
            }
            let args = borrowed_slice(a, n).ok_or_else(invalid)?;
            match op {
                0 => {
                    let [value, bytes, identity] = args else {
                        return Err(invalid());
                    };
                    bytes.as_word().ok_or_else(invalid)?;
                    s.graph.borrow_mut().allocations.push((
                        object(*value)?.expose_provenance(),
                        identity.as_word().ok_or_else(invalid)?,
                    ));
                }
                1 => {
                    let [root] = args else {
                        return Err(invalid());
                    };
                    s.graph
                        .borrow_mut()
                        .roots
                        .push(object(*root)?.expose_provenance());
                }
                2 => {
                    let [from, to] = args else {
                        return Err(invalid());
                    };
                    s.graph.borrow_mut().edges.push((
                        object(*from)?.expose_provenance(),
                        object(*to)?.expose_provenance(),
                    ));
                }
                3 => {
                    let [gaps] = args else {
                        return Err(invalid());
                    };
                    let gaps = gaps.as_word().ok_or_else(invalid)?;
                    if let Some(error) = s.failure.get() {
                        return Err(error);
                    }
                    let partition = s.graph.borrow().partition().ok_or_else(invalid)?;
                    if gaps & !s.accept.get() != 0 {
                        return Err(Outcome::failed(native::INVALID_REQUEST, 3));
                    }
                    s.completed.set(true);
                    for (address, identity, reached) in partition {
                        s.send(
                            s.caller.get(),
                            if reached {
                                b"reached:identity:"
                            } else {
                                b"notReached:identity:"
                            },
                            &[
                                Operand::object(ptr::with_exposed_provenance_mut(address)),
                                Operand::word(identity),
                            ],
                        )?;
                    }
                    return s.send(s.caller.get(), b"complete:", &[Operand::word(gaps)]);
                }
                _ => return Err(invalid()),
            }
            Ok(Operand::unit())
        })();
        if let Err(error) = answer
            && r == s.sink.get()
        {
            s.failure.set(Some(error));
        }
        answer
    }
}
macro_rules! callback {
    ($name:ident,$op:expr) => {
        unsafe extern "C" fn $name(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
            unsafe { receive(c, r, a, n, $op) }.map_or_else(|e| e, Outcome::loaded)
        }
    };
}
callback!(allocation, 0);
callback!(root, 1);
callback!(edge, 2);
callback!(complete, 3);
unsafe extern "C" fn report(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let s = state(c)?;
        let Some([caller, accept]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let caller = object(*caller)?;
        let accept = accept.as_word().ok_or_else(invalid)?;
        if s.active.replace(true) {
            return Err(Outcome::failed(native::INVALID_REQUEST, 2));
        }
        let _guard = Observation(s);
        s.caller.set(caller);
        s.accept.set(accept);
        let sink = object(s.send(s.sink_behavior, b"allocate", &[Operand::word(0)])?)?;
        s.sink.set(sink);
        let result = s.send(s.storage, b"observe:", &[Operand::object(sink)])?;
        if let Some(error) = s.failure.get() {
            return Err(error);
        }
        if !s.completed.get() {
            return Err(invalid());
        }
        Ok(result)
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
unsafe fn construct(id: Protocol, ns: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named = |name: &[u8]| {
            object(id.send_named(ns, b"at:", &[Operand::object(id.intern_name(name)?)])?)
        };
        let roots = named(b"Roots")?;
        let storage = named(b"Storage")?;
        let keep = |o: Oop| -> Result<Oop, Outcome> {
            id.send_named(roots, b"retain:", &[Operand::object(o)])?;
            Ok(o)
        };
        keep(ns)?;
        let parent = object(id.send_named(
            id.receiver,
            b"behaviorOf:",
            &[id.send_named(id.receiver, b"objectPrototype", &[])?],
        )?)?;
        let behavior = keep(object(id.send_named(parent, b"delegated", &[])?)?)?;
        let receiver = keep(object(id.send_named(
            behavior,
            b"allocate",
            &[Operand::word(0)],
        )?)?)?;
        let sink_behavior = keep(object(id.send_named(parent, b"delegated", &[])?)?)?;
        for name in [
            b"observe:".as_slice(),
            b"reached:identity:",
            b"notReached:identity:",
            b"complete:",
        ] {
            keep(id.intern_name(name)?)?;
        }
        let s = Box::into_raw(Box::new(State {
            id,
            ns,
            storage,
            receiver,
            sink_behavior,
            thread: std::thread::current().id(),
            active: Cell::new(false),
            sink: Cell::new(ptr::null_mut()),
            caller: Cell::new(ptr::null_mut()),
            accept: Cell::new(0),
            completed: Cell::new(false),
            graph: RefCell::new(graph::Graph::default()),
            failure: Cell::new(None),
        }));
        for (vt, name, method) in [
            (
                behavior,
                b"report:acceptingGaps:".as_slice(),
                report as native::Method,
            ),
            (behavior, b"rootsDo:", self::roots),
            (sink_behavior, b"allocation:bytes:identity:", allocation),
            (sink_behavior, b"root:", root),
            (sink_behavior, b"edgeFrom:to:", edge),
            (sink_behavior, b"complete:", complete),
        ] {
            id.send_named(
                vt,
                b"addMethod",
                &[
                    Operand::object(id.intern_name(name)?),
                    Operand::entry(NativeEntry::new(method, s.cast())),
                ],
            )?;
        }
        id.send_named(roots, b"register:", &[Operand::object(receiver)])?;
        id.send_named(
            ns,
            b"at:put:",
            &[
                Operand::object(id.intern_name(b"Trace")?),
                Operand::object(receiver),
            ],
        )?;
        INSTANCE.store(s, Ordering::Release);
        Ok(Operand::object(receiver))
    }
}
unsafe extern "C" fn initialize(id: *const Protocol, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        if !native::valid_protocol(id) {
            return Err(invalid());
        }
        let Some([ns]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let ns = object(*ns)?;
        let old = INSTANCE.load(Ordering::Acquire);
        if !old.is_null() {
            let s = state(old.cast())?;
            return if s.id.same_contract(&*id) && s.ns == ns {
                Ok(Operand::object(s.receiver))
            } else {
                Err(invalid())
            };
        }
        if INITIALIZING.swap(true, Ordering::AcqRel) {
            return Err(invalid());
        }
        let result = construct(*id, ns);
        INITIALIZING.store(false, Ordering::Release);
        result
    })()
    .map_or_else(|e| e, Outcome::loaded)
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: native::VERSION,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
