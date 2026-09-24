//! Memory management above Id. All foreign objects are opaque and all service
//! operations use live Id messages. See README.md for the native protocol.
mod accounting;
mod collection;
mod construction;
mod native_stack;
use birnam_abi::{
    Oop, borrowed_slice,
    id_protocol::{self as native, Method, NativeEntry, Operand, Outcome, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
    ptr,
};
use std::collections::{HashMap, HashSet};

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
struct Memory {
    id: Protocol,
    receiver: Oop,
    roots: Oop,
    thread: std::thread::ThreadId,
    selectors: HashMap<&'static [u8], Oop>,
    phase: Cell<Phase>,
    tracers: RefCell<HashMap<usize, Owner>>,
    finalizers: RefCell<HashMap<usize, Owner>>,
    temporary: RefCell<Vec<Oop>>,
    budget: Cell<usize>,
    charged: Cell<usize>,
    constructing: Cell<bool>,
    walk: RefCell<Option<collection::Walk>>,
    retirement: RefCell<Vec<Oop>>,
}
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn object(value: Operand) -> Result<Oop, Outcome> {
    value
        .as_object()
        .filter(|p| !p.is_null())
        .ok_or_else(invalid)
}
fn done(value: Result<Operand, Outcome>) -> Outcome {
    match value {
        Ok(v) => Outcome::loaded(v),
        Err(e) => e,
    }
}
impl Memory {
    unsafe fn send(
        &self,
        receiver: Oop,
        name: &'static [u8],
        args: &[Operand],
    ) -> Result<Operand, Outcome> {
        unsafe { self.id.send(receiver, self.selectors[name], args) }
    }
    fn check(&self) -> Result<(), Outcome> {
        if self.thread == std::thread::current().id() && self.phase.get() != Phase::Failed {
            Ok(())
        } else {
            Err(invalid())
        }
    }
    unsafe fn identity(&self, value: Oop) -> Result<usize, Outcome> {
        unsafe { self.send(self.id.receiver, b"identityOf:", &[Operand::object(value)]) }?
            .as_word()
            .filter(|n| *n != 0)
            .ok_or_else(invalid)
    }
    unsafe fn trace_owner(&self, mut behavior: Oop) -> Result<Option<Oop>, Outcome> {
        let mut visited = HashSet::new();
        while !behavior.is_null() && visited.insert(behavior.addr()) {
            let owner = self.tracers.borrow().get(&behavior.addr()).copied();
            if let Some(owner) = owner {
                if unsafe { self.identity(behavior) }? != owner.identity {
                    return Err(invalid());
                }
                return Ok(Some(owner.object));
            }
            let parent =
                unsafe { self.send(self.id.receiver, b"parentOf:", &[Operand::object(behavior)]) }?;
            behavior = parent.as_object().unwrap_or(ptr::null_mut());
        }
        Ok(None)
    }
}
unsafe fn memory<'a>(context: *mut c_void) -> Result<&'a Memory, Outcome> {
    let memory = unsafe { &*context.cast::<Memory>() };
    memory.check()?;
    Ok(memory)
}
unsafe extern "C" fn roots(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let memory = memory(context)?;
        let Some([visitor]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let visitor = visitor
            .as_entry()
            .filter(|v| v.method.is_some())
            .ok_or_else(invalid)?;
        let mut roots = vec![receiver, memory.roots];
        roots.extend(memory.selectors.values().copied());
        roots.extend(memory.tracers.borrow().values().map(|v| v.object));
        roots.extend(memory.finalizers.borrow().values().map(|v| v.object));
        roots.extend(memory.temporary.borrow().iter().copied());
        roots.extend(memory.retirement.borrow().iter().copied());
        for root in roots {
            (visitor.method.unwrap())(visitor.context, receiver, &Operand::object(root), 1)
                .into_result()?;
        }
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn root_registry(
    context: *mut c_void,
    _: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    done((|| {
        if count != 0 {
            return Err(invalid());
        }
        Ok(Operand::object(unsafe { memory(context)? }.roots))
    })())
}
unsafe extern "C" fn manager(
    context: *mut c_void,
    _: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    done((|| {
        if count != 0 {
            return Err(invalid());
        }
        Ok(Operand::object(unsafe { memory(context)? }.receiver))
    })())
}
unsafe extern "C" fn trace_with(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done(unsafe { attach(context, args, count, true) })
}
unsafe extern "C" fn finalize_with(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done(unsafe { attach(context, args, count, false) })
}
unsafe fn attach(
    context: *mut c_void,
    args: *const Operand,
    count: usize,
    trace: bool,
) -> Result<Operand, Outcome> {
    unsafe {
        let memory = memory(context)?;
        let Some([target, owner]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let target = object(*target)?;
        let identity = memory.identity(target)?;
        let owner = if owner.kind() == native::UNIT {
            None
        } else {
            Some(object(*owner)?)
        };
        if trace
            && memory
                .send(
                    memory.id.receiver,
                    b"isBehavior:",
                    &[Operand::object(target)],
                )?
                .as_word()
                != Some(1)
        {
            return Err(invalid());
        }
        if let Some(owner) = owner {
            memory.identity(owner)?;
            if memory.phase.get() == Phase::Marking && memory.walk.borrow().is_some() {
                collection::emit(memory, owner)?;
            } else if memory.phase.get() == Phase::Finalizing
                && !memory
                    .walk
                    .borrow()
                    .as_ref()
                    .is_some_and(|walk| walk.marked(owner))
            {
                // A replacement callback cannot resurrect a provider in the dead snapshot.
                return Err(invalid());
            }
        }
        // Selection remains live: an earlier trace/finalizer may change a later
        // object's owner. Do not hold either registry across those callbacks.
        let map = if trace {
            &memory.tracers
        } else {
            &memory.finalizers
        };
        if let Some(object) = owner {
            map.borrow_mut()
                .insert(target.addr(), Owner { identity, object });
        } else {
            map.borrow_mut().remove(&target.addr());
        }
        Ok(Operand::unit())
    }
}
unsafe extern "C" fn budget(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let memory = memory(context)?;
        if memory.phase.get() != Phase::Idle {
            return Err(invalid());
        }
        let Some([bytes]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        memory
            .budget
            .set(bytes.as_word().filter(|n| *n != 0).ok_or_else(invalid)?);
        Ok(Operand::unit())
    })())
}
// Metadata owners retire their weak records before Id detaches any allocation.
unsafe extern "C" fn register_retirement(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let memory = memory(context)?;
        if !matches!(memory.phase.get(), Phase::Inactive | Phase::Idle) {
            return Err(invalid());
        }
        let Some([owner]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let owner = object(*owner)?;
        memory.identity(owner)?;
        let mut owners = memory.retirement.borrow_mut();
        if !owners.contains(&owner) {
            owners.push(owner);
        }
        Ok(Operand::unit())
    })())
}
const NAMES: &[&[u8]] = &[
    b"identityOf:",
    b"behaviorOf:",
    b"isBehavior:",
    b"parentOf:",
    b"allocationSizeOf:",
    b"allocationChargeFor:",
    b"rootsDo:",
    b"allocationsDo:",
    b"referencesOf:do:",
    b"retireIndexesFor:",
    b"detachAllocationAt:",
    b"finalize:",
    b"retire:",
    b"allocate:bytes:",
    b"newBehaviorWithParent:",
    b"internBytes:",
    b"symbolNamed:",
    b"allocate",
    b"delegated",
    b"intern:",
    b"addMethod",
];
unsafe fn construct(id: Protocol, roots_object: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let mut selectors = HashMap::new();
        for name in NAMES {
            selectors.insert(*name, id.intern_name(name)?);
        }
        let prototype = id.send_named(id.receiver, b"objectPrototype", &[])?;
        let parent = object(id.send_named(id.receiver, b"behaviorOf:", &[prototype])?)?;
        let behavior = object(id.send_named(parent, b"delegated", &[])?)?;
        let receiver = object(id.send_named(behavior, b"allocate", &[Operand::word(0)])?)?;
        let pointer = Box::into_raw(Box::new(Memory {
            id,
            receiver,
            roots: roots_object,
            thread: std::thread::current().id(),
            selectors,
            phase: Cell::new(Phase::Inactive),
            tracers: RefCell::new(HashMap::new()),
            finalizers: RefCell::new(HashMap::new()),
            temporary: RefCell::new(Vec::new()),
            budget: Cell::new(1 << 20),
            charged: Cell::new(0),
            constructing: Cell::new(false),
            walk: RefCell::new(None),
            retirement: RefCell::new(Vec::new()),
        }));
        for (name, method) in [
            (b"rootsDo:".as_slice(), roots as Method),
            (b"roots", root_registry),
            (b"activate", construction::activate),
            (b"prepareAllocation", construction::prepare),
            (b"registerRetirement:", register_retirement),
            (b"collect", collection::collect),
            (b"collectPrecisely", collection::collect_precisely),
            (b"trace:with:", trace_with),
            (b"referenceOwnerOf:", reference_owner),
            (b"finalize:with:", finalize_with),
            (b"budget:", budget),
            (b"allocationBytes", accounting::read),
            (b"canAllocate:", accounting::preview),
        ] {
            id.send_named(
                behavior,
                b"addMethod",
                &[
                    Operand::object(id.intern_name(name)?),
                    Operand::entry(NativeEntry::new(method, pointer.cast())),
                ],
            )?;
        }
        id.send_named(roots_object, b"register:", &[Operand::object(receiver)])?;
        id.send_named(
            id.receiver,
            b"addMethod",
            &[
                Operand::object(id.intern_name(b"memoryManager")?),
                Operand::entry(NativeEntry::new(manager, pointer.cast())),
            ],
        )?;
        Ok(Operand::object(receiver))
    }
}
unsafe extern "C" fn initialize(
    id: *const Protocol,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        if !native::valid_protocol(id) {
            return Err(invalid());
        }
        let Some([roots]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let roots = object(*roots)?;
        let id = *id;
        let selector = id.intern_name(b"memoryManager")?;
        let existing = id.send_named(id.receiver, b"lookup", &[Operand::object(selector)])?;
        if existing.kind() != native::UNIT {
            let memory = object(id.send(id.receiver, selector, &[])?)?;
            if id.send_named(memory, b"roots", &[])?.as_object() != Some(roots) {
                return Err(invalid());
            }
            return Ok(Operand::object(memory));
        }
        construct(id, roots)
    })())
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

unsafe extern "C" fn reference_owner(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let m = memory(context)?;
        let Some([behavior]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let behavior = object(*behavior)?;
        if m.send(m.id.receiver, b"isBehavior:", &[Operand::object(behavior)])?
            .as_word()
            != Some(1)
        {
            return Err(invalid());
        }
        Ok(m.trace_owner(behavior)?
            .map_or_else(Operand::unit, Operand::object))
    })())
}
