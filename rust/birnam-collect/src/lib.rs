//! Oop-only collector vertical slice: explicit collection and owner callbacks.
//! Retains the production inventory/mark/retire/detach/finalize ordering.
mod collection;
mod construction;
mod native_stack;
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
    Inactive,
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
    image: Oop,
    roots: Oop,
    temporary: RefCell<Vec<Oop>>,
    budget: Cell<usize>,
    automatic: Cell<bool>,
    precise_payloads: Cell<bool>,
    charged: Cell<usize>,
    constructing: Cell<bool>,
    core_visitor: Oop,
    names: HashMap<&'static [u8], Oop>,
    phase: Cell<Phase>,
    ready: Cell<bool>,
    tracers: RefCell<HashMap<usize, Owner>>,
    finalizers: RefCell<HashMap<usize, Owner>>,
    retirement: RefCell<Vec<Oop>>,
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
    if c == s.receiver && s.phase.get() != Phase::Failed {
        Ok(s)
    } else {
        Err(s.invalid())
    }
}
unsafe extern "C" fn roots(_id: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    let s = match unsafe { memory(c) } {
        Ok(s) => s,
        Err(e) => return abi::fault_reference(e),
    };
    let Some([visitor]) = (unsafe { abi::borrowed_slice(a, n) }) else {
        return s.invalid();
    };
    let mut values: Vec<_> = s.names.values().copied().collect();
    values.extend([s.receiver, s.roots, s.core_visitor, s.image]);
    values.extend(s.temporary.borrow().iter().copied());
    values.extend(s.tracers.borrow().values().map(|v| v.object));
    values.extend(s.finalizers.borrow().values().map(|v| v.object));
    values.extend(s.retirement.borrow().iter().copied());
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
        Err(e) => return abi::fault_reference(e),
    };
    let Some([o, _]) = (unsafe { abi::borrowed_slice(a, n) }) else {
        return s.invalid();
    };
    if *o != s.receiver && *o != s.core_visitor {
        return s.invalid();
    }
    core::ptr::null_mut()
}
unsafe fn attach(c: Oop, a: *const Oop, n: usize, trace: bool) -> Result<Oop, Oop> {
    let s = unsafe { memory(c) }?;
    let Some([target, owner]) = (unsafe { abi::borrowed_slice(a,n) }) else { return Err(s.invalid()); };
    let target=abi::object_reference(*target);
    let owner=abi::object_reference(*owner);
    let identity = unsafe { s.identity(target) }?;
    if trace && unsafe { s.send(s.id.root,b"isBehavior:",&[target]) }? != abi::size(1).unwrap() {
        return Err(s.invalid());
    }
    if !owner.is_null() {
        unsafe { s.identity(owner) }?;
        if s.phase.get() == Phase::Marking && s.walk.borrow().is_some() {
            collection::emit(s,owner)?;
        } else if s.phase.get() == Phase::Finalizing && !s.walk.borrow().as_ref().is_some_and(|w|w.marked(owner)) {
            // A replacement cannot resurrect an owner in the dead-object list.
            return Err(s.invalid());
        }
    }
    let map = if trace { &s.tracers } else { &s.finalizers };
    if owner.is_null() { map.borrow_mut().remove(&target.addr()); }
    else { map.borrow_mut().insert(target.addr(),Owner{identity,object:owner}); }
    Ok(core::ptr::null_mut())
}
unsafe extern "C" fn register_trace(_id: Oop,c: Oop,_: Oop,a: *const Oop,n: usize) -> Oop {
    unsafe { attach(c,a,n,true) }.unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn register_finalizer(_id: Oop,c: Oop,_: Oop,a: *const Oop,n: usize) -> Oop {
    unsafe { attach(c,a,n,false) }.unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn reference_owner(_id: Oop,c: Oop,_: Oop,a: *const Oop,n: usize) -> Oop {
    (|| unsafe {
        let s=memory(c)?;
        let Some([behavior])=abi::borrowed_slice(a,n) else { return Err(s.invalid()); };
        if s.send(s.id.root,b"isBehavior:",&[*behavior])? != abi::size(1).unwrap() { return Err(s.invalid()); }
        Ok(s.trace_owner(*behavior)?.unwrap_or(core::ptr::null_mut()))
    })().unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn register_retirement(_id: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = memory(c)?;
        if !matches!(s.phase.get(), Phase::Inactive | Phase::Idle) { return Err(s.invalid()); }
        let Some([owner]) = abi::borrowed_slice(a, n) else { return Err(s.invalid()); };
        let owner=abi::object_reference(*owner);
        s.identity(owner)?;
        let mut owners = s.retirement.borrow_mut();
        if !owners.contains(&owner) { owners.push(owner); }
        Ok(core::ptr::null_mut())
    })().unwrap_or_else(abi::fault_reference)
}
unsafe extern "C" fn manager(_id:Oop,c:Oop,_r:Oop,_a:*const Oop,n:usize)->Oop {
    match unsafe{memory(c)} { Ok(s)=>if n==0{s.receiver}else{s.invalid()}, Err(e) => abi::fault_reference(e) }
}
unsafe extern "C" fn root_registry(_id:Oop,c:Oop,_r:Oop,_a:*const Oop,n:usize)->Oop {
    match unsafe{memory(c)} { Ok(s)=>if n==0{s.roots}else{s.invalid()}, Err(e) => abi::fault_reference(e) }
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
    b"allocationEpoch",
    b"referenceOwnersDo:",
    b"referenceOwnerOf:",
    b"isBehavior:",
    b"registerRetirement:",
    b"retire:",
    b"allocate:bytes:", b"newBehaviorWithParent:", b"allocationChargeFor:", b"allocationSizeOf:",
    b"vtableBehavior", b"nameBehavior", b"methodBehavior", b"moduleBehavior",
    b"intern", b"existingSymbol", b"rawIntern", b"nameWithBytes", b"newMethodAt:",
    b"rawNameWithBytes", b"rawSequence", b"rawWithContext:", b"rawNewMethodAt:",
    b"managedAllocate", b"managedDelegated", b"managedIntern", b"managedName", b"managedSequence",
    b"managedContext", b"managedNativeMethod", b"activate", b"prepareAllocation", b"budget:",
    b"automaticCollection:", b"precisePayloads:", b"precisePayloads", b"allocationBytes", b"chargeExternalAllocation:", b"collect", b"memoryManager", b"roots"
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
    MethodSpec{name:index(b"memoryManager"),entry:Some(manager)},
    MethodSpec{name:index(b"roots"),entry:Some(root_registry)},
    MethodSpec{name:index(b"managedAllocate"),entry:Some(construction::allocate)},
    MethodSpec{name:index(b"managedDelegated"),entry:Some(construction::delegated)},
    MethodSpec{name:index(b"managedIntern"),entry:Some(construction::intern)},
    MethodSpec{name:index(b"managedName"),entry:Some(construction::new_name)},
    MethodSpec{name:index(b"managedSequence"),entry:Some(construction::sequence)},
    MethodSpec{name:index(b"managedContext"),entry:Some(construction::method_context)},
    MethodSpec{name:index(b"managedNativeMethod"),entry:Some(construction::native_method)},
    MethodSpec{name:index(b"activate"),entry:Some(construction::activate)},
    MethodSpec{name:index(b"prepareAllocation"),entry:Some(construction::prepare)},
    MethodSpec{name:index(b"budget:"),entry:Some(construction::budget)},
    MethodSpec{name:index(b"automaticCollection:"),entry:Some(construction::automatic)},
    MethodSpec{name:index(b"precisePayloads:"),entry:Some(construction::precise_set)},
    MethodSpec{name:index(b"precisePayloads"),entry:Some(construction::precise)},
    MethodSpec{name:index(b"allocationBytes"),entry:Some(construction::charged)},
    MethodSpec{name:index(b"chargeExternalAllocation:"),entry:Some(construction::external)},
    MethodSpec{name:index(b"collect"),entry:Some(construction::collect)},
    MethodSpec { name:index(b"registerRetirement:"), entry:Some(register_retirement) },
    MethodSpec { name:index(b"referenceOwnerOf:"), entry:Some(reference_owner) },
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
        return if s.ready.get() && s.phase.get() != Phase::Failed && s.roots == *root_registry {
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
                image,
                temporary:RefCell::new(Vec::new()),
                budget:Cell::new(16<<20),
                automatic:Cell::new(true),
                precise_payloads:Cell::new(false),
                charged:Cell::new(0),
                constructing:Cell::new(false),
                roots: *root_registry,
                core_visitor,
                names,
                phase: Cell::new(Phase::Inactive),
                ready: Cell::new(false),
                tracers: RefCell::new(HashMap::new()),
                finalizers: RefCell::new(HashMap::new()),
                retirement: RefCell::new(Vec::new()),
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
                } else if spec.name == index(b"memoryManager") {
                    s.send(id.root,b"behaviorOf:",&[id.root])?
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
            s.send(id.root, b"referenceOwnersDo:", &[receiver])?;
            s.send(*root_registry, b"register:", &[receiver])?;
            s.ready.set(true);
            Ok(receiver)
        })()
    })
    .unwrap_or_else(abi::fault_reference)
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
