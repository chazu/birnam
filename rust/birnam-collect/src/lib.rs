//! Oop-only collector vertical slice: explicit collection and owner callbacks.
//! Retains the production inventory/mark/retire/detach/finalize ordering.
mod collection;
use birnam_abi::{
    self as abi, Oop,
    id_protocol::{Literal, MethodSpec, Module, Protocol},
};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Marking,
    Finalizing,
    Failed,
}
#[derive(Clone, Copy)]
struct Owner {
    identity: usize,
    object: Oop,
}
#[derive(Clone, Copy)]
struct Allocation {
    object: Oop,
    size: usize,
    identity: usize,
}
static API: std::sync::atomic::AtomicPtr<Protocol> =
    std::sync::atomic::AtomicPtr::new(core::ptr::null_mut());
static INSTANCE: std::sync::atomic::AtomicPtr<Memory> =
    std::sync::atomic::AtomicPtr::new(core::ptr::null_mut());
struct Memory {
    id: Protocol,
    receiver: Oop,
    roots: Oop,
    core_visitor: Oop,
    names: HashMap<&'static [u8], Oop>,
    phase: Cell<Phase>,
    tracers: RefCell<HashMap<usize, Owner>>,
    finalizers: RefCell<HashMap<usize, Owner>>,
    walk: RefCell<Option<collection::Walk>>,
    inventory: RefCell<Option<collection::Inventory>>,
}
impl Memory {
    fn invalid(&self) -> Oop {
        self.id.invalid
    }
    unsafe fn send(&self, r: Oop, n: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.id.send(r, self.names[n], a) }
    }
    unsafe fn identity(&self, o: Oop) -> Result<usize, Oop> {
        abi::as_size(unsafe { self.send(self.id.root, b"identityOf:", &[o]) }?)
            .ok_or(self.invalid())
    }
    unsafe fn trace_owner(&self, mut behavior: Oop) -> Result<Option<Oop>, Oop> {
        let mut visited: Option<HashSet<usize>> = None;
        while !behavior.is_null() {
            if visited
                .as_ref()
                .is_some_and(|seen| seen.contains(&behavior.addr()))
            {
                break;
            }
            let owner = self.tracers.borrow().get(&behavior.addr()).copied();
            if let Some(owner) = owner {
                if unsafe { self.identity(behavior) }? != owner.identity {
                    return Err(self.invalid());
                }
                return Ok(Some(owner.object));
            }
            visited
                .get_or_insert_with(HashSet::new)
                .insert(behavior.addr());
            behavior = unsafe { self.send(self.id.root, b"parentOf:", &[behavior]) }?;
        }
        Ok(None)
    }
}
unsafe fn memory<'a>(c: Oop) -> Result<&'a Memory, Oop> {
    let raw = INSTANCE.load(std::sync::atomic::Ordering::Acquire);
    if raw.is_null() {
        let api = API.load(std::sync::atomic::Ordering::Acquire);
        if api.is_null() {
            std::process::abort()
        }
        return Err(unsafe { (*api).invalid });
    }
    let s = unsafe { &*raw };
    if c == s.receiver {
        Ok(s)
    } else {
        Err(s.invalid())
    }
}
unsafe extern "C" fn roots(_id: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    let s = match unsafe { memory(c) } {
        Ok(s) => s,
        Err(e) => return e,
    };
    let Some([visitor]) = (unsafe { abi::borrowed_slice(a, n) }) else {
        return s.invalid();
    };
    let mut values: Vec<_> = s.names.values().copied().collect();
    values.extend([s.receiver, s.roots, s.core_visitor]);
    values.extend(s.tracers.borrow().values().map(|v| v.object));
    values.extend(s.finalizers.borrow().values().map(|v| v.object));
    for o in values {
        if let Err(e) = unsafe { s.send(*visitor, b"visit:", &[o]) } {
            return e;
        }
    }
    core::ptr::null_mut()
}
unsafe extern "C" fn references(_id: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    let s = match unsafe { memory(c) } {
        Ok(s) => s,
        Err(e) => return e,
    };
    let Some([o, _]) = (unsafe { abi::borrowed_slice(a, n) }) else {
        return s.invalid();
    };
    if *o != s.receiver && *o != s.core_visitor {
        return s.invalid();
    }
    core::ptr::null_mut()
}
unsafe extern "C" fn register_trace(_id: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    let s = match unsafe { memory(c) } {
        Ok(s) => s,
        Err(e) => return e,
    };
    let Some([behavior, owner]) = (unsafe { abi::borrowed_slice(a, n) }) else {
        return s.invalid();
    };
    match unsafe { s.identity(*behavior) } {
        Ok(identity) => {
            s.tracers.borrow_mut().insert(
                behavior.addr(),
                Owner {
                    identity,
                    object: *owner,
                },
            );
            *owner
        }
        Err(e) => e,
    }
}
unsafe extern "C" fn register_finalizer(_id: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    let s = match unsafe { memory(c) } {
        Ok(s) => s,
        Err(e) => return e,
    };
    let Some([o, owner]) = (unsafe { abi::borrowed_slice(a, n) }) else {
        return s.invalid();
    };
    match unsafe { s.identity(*o) } {
        Ok(identity) => {
            s.finalizers.borrow_mut().insert(
                o.addr(),
                Owner {
                    identity,
                    object: *owner,
                },
            );
            *owner
        }
        Err(e) => e,
    }
}
macro_rules! literals {($($n:literal),*$(,)?)=>{const NAMES:&[&[u8]]=&[$($n),*];static LITERALS:&[Literal]=&[$(Literal::new($n)),*];};}
literals![
    b"objectBehavior",
    b"delegated",
    b"allocate",
    b"methodAt:",
    b"withContext:",
    b"addMethod",
    b"behaviorOf:",
    b"identityOf:",
    b"parentOf:",
    b"rootsDo:",
    b"referencesOf:do:",
    b"visit:",
    b"allocationsDo:",
    b"allocation:bytes:identity:",
    b"coreEdge:",
    b"collectPrecisely",
    b"trace:with:",
    b"finalize:with:",
    b"finalize:",
    b"sequence",
    b"removeDeadObjects",
    b"detachAllocation:",
    b"releaseDetachedAllocation:",
    b"register:",
    b"allocationCount",
    b"allocationEpoch"
];
const fn index(name: &[u8]) -> usize {
    let mut i = 0;
    while i < NAMES.len() {
        let a = NAMES[i];
        if a.len() == name.len() {
            let mut j = 0;
            while j < a.len() && a[j] == name[j] {
                j += 1
            }
            if j == a.len() {
                return i;
            }
        }
        i += 1
    }
    panic!("missing collector name")
}
static METHODS: &[MethodSpec] = &[
    MethodSpec {
        name: index(b"rootsDo:"),
        entry: Some(roots),
    },
    MethodSpec {
        name: index(b"referencesOf:do:"),
        entry: Some(references),
    },
    MethodSpec {
        name: index(b"visit:"),
        entry: Some(collection::visit),
    },
    MethodSpec {
        name: index(b"allocation:bytes:identity:"),
        entry: Some(collection::allocation),
    },
    MethodSpec {
        name: index(b"coreEdge:"),
        entry: Some(collection::core_edge),
    },
    MethodSpec {
        name: index(b"collectPrecisely"),
        entry: Some(collection::collect),
    },
    MethodSpec {
        name: index(b"trace:with:"),
        entry: Some(register_trace),
    },
    MethodSpec {
        name: index(b"finalize:with:"),
        entry: Some(register_finalizer),
    },
];
unsafe extern "C" fn initialize(
    p: *const Protocol,
    image: Oop,
    l: *const Oop,
    n: usize,
    a: *const Oop,
    argc: usize,
) -> Oop {
    API.store(p.cast_mut(), std::sync::atomic::Ordering::Release);
    let id = unsafe { *p };
    let Some([root_registry]) = (unsafe { abi::borrowed_slice(a, argc) }) else {
        return id.invalid;
    };
    let existing = INSTANCE.load(std::sync::atomic::Ordering::Acquire);
    if !existing.is_null() {
        let s = unsafe { &*existing };
        return if s.roots == *root_registry {
            s.receiver
        } else {
            id.invalid
        };
    }
    let Some(symbols) = (unsafe { abi::borrowed_slice(l, n) }) else {
        return id.invalid;
    };
    if symbols.len() != NAMES.len() {
        return id.invalid;
    }
    (unsafe {
        (|| {
            let names: HashMap<_, _> = NAMES.iter().copied().zip(symbols.iter().copied()).collect();
            let send = |r, n: &[u8], a: &[Oop]| id.send(r, names[n], a);
            let base = send(id.root, b"objectBehavior", &[])?;
            let vt = send(base, b"delegated", &[])?;
            let receiver = send(vt, b"allocate", &[abi::size(size_of::<usize>()).unwrap()])?;
            let core_vt = send(base, b"delegated", &[])?;
            let core_visitor = send(
                core_vt,
                b"allocate",
                &[abi::size(size_of::<usize>()).unwrap()],
            )?;
            let s = Box::new(Memory {
                id,
                receiver,
                roots: *root_registry,
                core_visitor,
                names,
                phase: Cell::new(Phase::Idle),
                tracers: RefCell::new(HashMap::new()),
                finalizers: RefCell::new(HashMap::new()),
                walk: RefCell::new(None),
                inventory: RefCell::new(None),
            });
            let raw = Box::into_raw(s);
            INSTANCE.store(raw, std::sync::atomic::Ordering::Release);
            receiver.cast::<*const Memory>().write(raw);
            core_visitor.cast::<*const Memory>().write(raw);
            let s = &*raw;
            for spec in METHODS {
                let m = s.send(image, b"methodAt:", &[symbols[spec.name]])?;
                let m = s.send(m, b"withContext:", &[receiver])?;
                let target = if spec.name == index(b"coreEdge:") {
                    core_vt
                } else {
                    vt
                };
                let selector = if spec.name == index(b"coreEdge:") {
                    s.names[b"visit:".as_slice()]
                } else {
                    symbols[spec.name]
                };
                s.send(target, b"addMethod", &[selector, m])?;
            }
            // Native payloads are interpreted only by their own registered owners.
            for (behavior, owner) in [
                (vt, receiver),
                (core_vt, receiver),
                (
                    s.send(id.root, b"behaviorOf:", &[*root_registry])?,
                    *root_registry,
                ),
            ] {
                let identity = s.identity(behavior)?;
                s.tracers.borrow_mut().insert(
                    behavior.addr(),
                    Owner {
                        identity,
                        object: owner,
                    },
                );
            }
            s.send(*root_registry, b"register:", &[receiver])?;
            Ok(receiver)
        })()
    })
    .unwrap_or_else(|e| e)
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v2: Module = Module {
    version: 2,
    byte_size: size_of::<Module>() as u32,
    literals: LITERALS.as_ptr(),
    literal_count: LITERALS.len(),
    methods: METHODS.as_ptr(),
    method_count: METHODS.len(),
    initialize: Some(initialize),
};
