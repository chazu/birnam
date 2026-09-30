//! Scoped storage diagnostics over Id/Roots/reference-owner messages.
//! Observation roots protect the inventory without becoming declared graph roots.
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{self as native, NativeEntry, Operand, Outcome, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
    ptr,
};
use std::{
    collections::{HashMap, HashSet},
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};

// Native stack roots are not inspected. Missing owner coverage is never guessed.
const NATIVE_ROOTS_UNOBSERVED: usize = 1;
const PAYLOAD_OWNER_MISSING: usize = 2;
#[derive(Clone, Copy)]
struct Allocation {
    object: Oop,
    bytes: usize,
    identity: usize,
}
#[derive(Default)]
struct Snapshot {
    allocations: Vec<Allocation>,
    indices: HashMap<usize, usize>,
    roots: Vec<Oop>,
    edges: Vec<(Oop, Oop)>,
    failure: Option<Outcome>,
}
struct State {
    id: Protocol,
    ns: Oop,
    roots: Oop,
    gc: Oop,
    receiver: Oop,
    sink_behavior: Oop,
    thread: std::thread::ThreadId,
    active: Cell<bool>,
    sink: Cell<Oop>,
    phase: Cell<usize>,
    source: Cell<Oop>,
    caller: Cell<Oop>,
    snapshot: RefCell<Snapshot>,
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
    unsafe fn send(&self, r: Oop, name: &[u8], args: &[Operand]) -> Result<Operand, Outcome> {
        unsafe { self.id.send_named(r, name, args) }
    }
    fn checked(&self) -> Result<(), Outcome> {
        self.snapshot.borrow().failure.map_or(Ok(()), Err)
    }
}
struct Observation<'a>(&'a State);
impl Drop for Observation<'_> {
    fn drop(&mut self) {
        self.0.phase.set(0);
        self.0.sink.set(ptr::null_mut());
        self.0.caller.set(ptr::null_mut());
        *self.0.snapshot.borrow_mut() = Snapshot::default();
        self.0.active.set(false);
    }
}
unsafe extern "C" fn roots(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let s = state(c)?;
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let v = object(*v)?;
        let mut values = vec![
            s.ns,
            s.receiver,
            s.sink_behavior,
            s.caller.get(),
            s.sink.get(),
        ];
        // Only our diagnostic root sink omits the temporary inventory roots.
        // A collection (including one invoked by a provider) sees every object.
        if v != s.sink.get() {
            values.extend(s.snapshot.borrow().allocations.iter().map(|a| a.object));
        }
        for value in values.into_iter().filter(|o| !o.is_null()) {
            s.send(v, b"visit:", &[Operand::object(value)])?;
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
    allocation: bool,
) -> Result<Operand, Outcome> {
    unsafe {
        let s = state(c)?;
        let result = (|| {
            if r != s.sink.get() || !s.active.get() {
                return Err(invalid());
            }
            let args = borrowed_slice(a, n).ok_or_else(invalid)?;
            let mut data = s.snapshot.borrow_mut();
            if allocation {
                if s.phase.get() != 1 {
                    return Err(invalid());
                }
                let [value, bytes, identity] = args else {
                    return Err(invalid());
                };
                let allocation = Allocation {
                    object: object(*value)?,
                    bytes: bytes.as_word().ok_or_else(invalid)?,
                    identity: identity.as_word().filter(|n| *n != 0).ok_or_else(invalid)?,
                };
                let index = data.allocations.len();
                if data
                    .indices
                    .insert(allocation.object.addr(), index)
                    .is_some()
                {
                    return Err(invalid());
                }
                data.allocations.push(allocation);
            } else {
                let [value] = args else {
                    return Err(invalid());
                };
                let value = value.as_object().ok_or_else(invalid)?;
                if value.is_null() {
                    return Ok(Operand::unit());
                }
                if !data.indices.contains_key(&value.addr()) {
                    return Err(invalid());
                }
                match s.phase.get() {
                    2 => data.roots.push(value),
                    3 => data.edges.push((s.source.get(), value)),
                    _ => return Err(invalid()),
                }
            }
            Ok(Operand::unit())
        })();
        // A provider cannot hide a bad callback by ignoring its failure.
        if let Err(e) = result
            && r == s.sink.get()
        {
            s.snapshot.borrow_mut().failure.get_or_insert(e);
        }
        result
    }
}
unsafe extern "C" fn allocation(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe { receive(c, r, a, n, true) }.map_or_else(|e| e, Outcome::loaded)
}
unsafe extern "C" fn visit(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    unsafe { receive(c, r, a, n, false) }.map_or_else(|e| e, Outcome::loaded)
}
unsafe extern "C" fn observe(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    (|| unsafe {
        let s = state(c)?;
        let Some([caller]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let caller = object(*caller)?;
        if s.active.replace(true) {
            return Err(Outcome::failed(native::INVALID_REQUEST, 2));
        }
        let _guard = Observation(s);
        s.caller.set(caller);
        // A fresh receiver makes retained callback objects fail closed forever.
        let sink = object(s.send(s.sink_behavior, b"allocate", &[Operand::word(0)])?)?;
        s.sink.set(sink);
        let visitor = Operand::object(sink);
        s.phase.set(1);
        s.send(s.id.receiver, b"allocationsDo:", &[visitor])?;
        s.checked()?;
        let mut allocations = s.snapshot.borrow().allocations.clone();
        allocations.sort_unstable_by_key(|a| a.identity);
        let mut identities = HashSet::new();
        if allocations.iter().any(|a| !identities.insert(a.identity)) {
            return Err(invalid());
        }
        s.phase.set(2);
        s.send(s.id.receiver, b"rootsDo:", &[visitor])?;
        s.send(s.roots, b"rootsDo:", &[visitor])?;
        s.checked()?;
        let mut gaps = NATIVE_ROOTS_UNOBSERVED;
        s.phase.set(3);
        for allocation in &allocations {
            let value = allocation.object;
            s.source.set(value);
            let kind = s
                .send(
                    s.id.receiver,
                    b"referencesOf:do:",
                    &[Operand::object(value), visitor],
                )?
                .as_word()
                .filter(|k| *k <= 2)
                .ok_or_else(invalid)?;
            if kind != 2 {
                let behavior = s.send(s.id.receiver, b"behaviorOf:", &[Operand::object(value)])?;
                let owner = s.send(s.gc, b"referenceOwnerOf:", &[behavior])?;
                if let Some(owner) = owner.as_object() {
                    // Owner is a strong edge in the collector's declared graph.
                    receive(c, sink, &Operand::object(owner), 1, false)?;
                    s.send(
                        owner,
                        b"referencesOf:do:",
                        &[Operand::object(value), visitor],
                    )?;
                } else if owner.kind() != native::UNIT {
                    return Err(invalid());
                } else if kind == 0 && allocation.bytes != 0 {
                    gaps |= PAYLOAD_OWNER_MISSING;
                }
            }
            s.checked()?;
        }
        s.phase.set(0);
        let (roots, edges) = {
            let data = s.snapshot.borrow();
            (data.roots.clone(), data.edges.clone())
        };
        for a in allocations {
            s.send(
                caller,
                b"allocation:bytes:identity:",
                &[
                    Operand::object(a.object),
                    Operand::word(a.bytes),
                    Operand::word(a.identity),
                ],
            )?;
        }
        for root in roots {
            s.send(caller, b"root:", &[Operand::object(root)])?;
        }
        for (from, to) in edges {
            s.send(
                caller,
                b"edgeFrom:to:",
                &[Operand::object(from), Operand::object(to)],
            )?;
        }
        let result = s.send(caller, b"complete:", &[Operand::word(gaps)])?;
        s.checked()?;
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
        let gc = named(b"GC")?;
        let keep = |value: Oop| -> Result<Oop, Outcome> {
            id.send_named(roots, b"retain:", &[Operand::object(value)])?;
            Ok(value)
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
        // Pre-intern query selectors before an inventory is protected.
        for name in [
            b"allocationsDo:".as_slice(),
            b"referencesOf:do:",
            b"referenceOwnerOf:",
            b"root:",
            b"edgeFrom:to:",
            b"complete:",
        ] {
            keep(id.intern_name(name)?)?;
        }
        let s = Box::into_raw(Box::new(State {
            id,
            ns,
            roots,
            gc,
            receiver,
            sink_behavior,
            thread: std::thread::current().id(),
            active: Cell::new(false),
            sink: Cell::new(ptr::null_mut()),
            phase: Cell::new(0),
            source: Cell::new(ptr::null_mut()),
            caller: Cell::new(ptr::null_mut()),
            snapshot: RefCell::new(Snapshot::default()),
        }));
        for (vt, name, method) in [
            (behavior, b"observe:".as_slice(), observe as native::Method),
            (behavior, b"rootsDo:", self::roots),
            (sink_behavior, b"allocation:bytes:identity:", allocation),
            (sink_behavior, b"visit:", visit),
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
                Operand::object(id.intern_name(b"Storage")?),
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
