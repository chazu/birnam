//! Oop-only production floor for the vertical migration slice.
use super::{
    id_dispatch,
    object_memory::{Allocation, ObjectMemory},
    symbol::Symbols,
    vtable::Vtables,
};
use crate::{
    abi::{
        self, Oop,
        id_protocol::{self as native, Method, Protocol},
    },
    raw::AddressMap,
};
std::thread_local! {static THREAD_ID:std::thread::ThreadId=std::thread::current().id();}
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ffi::c_void,
};
pub(crate) struct ModuleRecord {
    _handle: *mut c_void,
    pub(super) declarations: &'static [native::MethodSpec],
    pub(super) literals: Vec<Oop>,
    pub(super) symbols: Vec<Oop>,
    pub(super) methods: Vec<(Oop, Oop)>,
    pub(super) result: Oop,
}
pub(crate) struct State {
    pub protocol: Protocol,
    pub heap: RefCell<ObjectMemory>,
    pub tables: RefCell<Vtables>,
    pub symbols: RefCell<Symbols>,
    pub root: Oop,
    pub root_behavior: Oop,
    pub native_apply: Cell<Oop>,
    pub reference_owners: RefCell<Vec<(Oop, Oop)>>,
    pub meta: Oop,
    pub object: Oop,
    pub symbol: Oop,
    pub name: Oop,
    pub size: Oop,
    pub none: Oop,
    pub method: Oop,
    pub fault: Oop,
    pub sequence: Oop,
    pub module: Oop,
    pub lookup: Oop,
    pub dnu: Oop,
    pub modules: RefCell<AddressMap<ModuleRecord>>,
    pub names: RefCell<HashMap<&'static [u8], Oop>>,
    pub detached: RefCell<AddressMap<Allocation>>,
    pub active: RefCell<Vec<CallFrame>>,
    pub permanent: RefCell<Vec<Oop>>,
    pub default_lookup: Cell<Oop>,
    pub invalid_fault: Cell<Oop>,
    pub unbound_fault: Cell<Oop>,
    pub owner: std::thread::ThreadId,
}
// These records live in Id-owned storage. They borrow the immutable argument
// area only for the synchronous native call; suspended language execution must
// continue to own its arguments in the stackless runtime's frames.
pub(crate) struct CallFrame {
    receiver: Oop,
    selector: Oop,
    arguments: *const Oop,
    count: usize,
    method: Oop,
    context: Oop,
    owner: Oop,
}
pub(crate) struct Scope<'a> {
    s: &'a State,
    start: usize,
    _arguments: core::marker::PhantomData<&'a [Oop]>,
}
impl Scope<'_> {
    pub(super) fn entered(&self, method: Oop, context: Oop, owner: Oop) {
        let mut frames = self.s.active.borrow_mut();
        let frame = &mut frames[self.start];
        frame.method = method;
        frame.context = context;
        frame.owner = owner;
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        let mut frames = self.s.active.borrow_mut();
        debug_assert_eq!(frames.len(), self.start + 1);
        frames.truncate(self.start);
    }
}
#[derive(Clone, Copy)]
pub(crate) enum Action {
    Lookup,
    ApplyNative,
    ReferenceOwners,
    NoReferences,
    TableReferences,
    MethodReferences,
    FaultReferences,
    SequenceReferences,
    ModuleReferences,
    Add,
    Remove,
    Allocate,
    Delegated,
    Behavior,
    Reassociate,
    Parent,
    ParentPut,
    Identity,
    Extent,
    Count,
    AllocationEpoch,
    Roots,
    Allocations,
    References,
    Retire,
    Detach,
    Release,
    CoreObject,
    CoreName,
    NewName,
    Intern,
    Load,
    At,
    Put,
    NewSequence,
    SequenceSize,
    MethodAt,
    LiteralAt,
    Invoke,
    WithContext,
    FaultKind,
    FaultDetail,
    MakeFault,
    SelfValue,
    Bindings,
    Epoch,
    BindingEpoch,
}
// Each core primitive uses the same Id object as its context. Context
// identity is checked before reading Id-owned state; no per-method cookie
// object or opcode registry lookup is needed.
macro_rules! core_primitives {
    ($(($name:literal,$action:ident)),*$(,)?) => {
        #[allow(non_snake_case)]
        mod primitive {
            use super::*;
            $(pub(super) unsafe extern "C" fn $action(root:Oop,context:Oop,r:Oop,a:*const Oop,n:usize)->Oop {
                let s=unsafe{&*root.cast::<*const State>().read()};
                if context!=root{return s.invalid()}
                let Some(a)=(unsafe{abi::borrowed_slice(a,n)})else{return s.invalid()};
                unsafe{s.perform(Action::$action,r,a)}.unwrap_or_else(|e|e)
            })*
        }
        pub(super) const OPERATIONS:&[(&[u8],Action,Method)]=&[$(($name,Action::$action,primitive::$action)),*];
    };
}
core_primitives![
    (b"lookup", Lookup),
    (b"addMethod", Add),
    (b"apply", ApplyNative),
    (b"referenceOwnersDo:", ReferenceOwners),
    (b"removeMethod:", Remove),
    (b"allocate", Allocate),
    (b"delegated", Delegated),
    (b"behaviorOf:", Behavior),
    (b"reassociateBehaviorOf:with:", Reassociate),
    (b"parentOf:", Parent),
    (b"parentOf:put:", ParentPut),
    (b"identityOf:", Identity),
    (b"allocationSizeOf:", Extent),
    (b"allocationEpoch", AllocationEpoch),
    (b"allocationCount", Count),
    (b"rootsDo:", Roots),
    (b"allocationsDo:", Allocations),
    (b"referencesOf:do:", References),
    (b"removeDeadObjects", Retire),
    (b"detachAllocation:", Detach),
    (b"releaseDetachedAllocation:", Release),
    (b"objectBehavior", CoreObject),
    (b"nameBehavior", CoreName),
    (b"nameWithBytes", NewName),
    (b"intern", Intern),
    (b"loadNative:", Load),
    (b"at:", At),
    (b"at:put:", Put),
    (b"sequence", NewSequence),
    (b"size", SequenceSize),
    (b"methodAt:", MethodAt),
    (b"invokeOn:arguments:", Invoke),
    (b"literalAt:", LiteralAt),
    (b"withContext:", WithContext),
    (b"faultKind", FaultKind),
    (b"detail", FaultDetail),
    (b"fault:detail:", MakeFault),
    (b"yourself", SelfValue),
    (b"bindingsDo:", Bindings),
    (b"behaviorEpoch", Epoch),
    (b"bindingEpoch", BindingEpoch),
    (b"noReferences", NoReferences),
    (b"tableReferences", TableReferences),
    (b"methodReferences", MethodReferences),
    (b"faultReferences", FaultReferences),
    (b"sequenceReferences", SequenceReferences),
    (b"moduleReferences", ModuleReferences),
];
impl State {
    pub fn check(&self) -> Result<(), Oop> {
        if self.owner == THREAD_ID.with(|id| *id) {
            Ok(())
        } else {
            Err(self.invalid())
        }
    }
    pub fn invalid(&self) -> Oop {
        self.invalid_fault.get()
    }
    pub fn unbound(&self) -> Oop {
        self.unbound_fault.get()
    }
    pub fn selector(&self, b: &[u8]) -> Oop {
        *self.names.borrow().get(b).expect("owned core selector")
    }
    pub fn seed_symbol(&self, b: &[u8]) -> Oop {
        self.symbols
            .borrow_mut()
            .intern(&mut self.heap.borrow_mut(), self.symbol, b)
            .unwrap()
    }
    pub(super) fn allocate(&self, behavior: Oop, size: usize) -> Result<Oop, Oop> {
        if !ObjectMemory::valid_payload_size(size) || !self.tables.borrow().is_registered(behavior)
        {
            return Err(self.invalid());
        }
        let mut h = self.heap.borrow_mut();
        let o = h.allocate(size);
        assert!(h.set_vtable(o, behavior));
        Ok(o)
    }
    // Allocation facts only. Payload interpretation belongs to the selected
    // owner implementation, never to the allocator or generic dispatcher.
    pub(super) fn payload(&self, o: Oop) -> Result<(*mut u8, usize), Oop> {
        let check = |a: &Allocation| {
            if a.object() != o {
                return Err(self.invalid());
            }
            Ok((a.payload().as_ptr(), a.requested_size()))
        };
        if let Some(a) = self.heap.borrow().get(o.addr()) {
            return check(a);
        }
        self.detached
            .borrow()
            .values()
            .find(|a| a.object() == o)
            .map(check)
            .unwrap_or(Err(self.invalid()))
    }
    pub fn behavior(&self, o: Oop) -> Result<Oop, Oop> {
        if o.is_null() {
            return Ok(self.none);
        }
        if abi::as_size(o).is_some() {
            return Ok(self.size);
        }
        self.heap
            .borrow()
            .get(o.addr())
            .filter(|a| a.object() == o)
            .map(|_| unsafe { ObjectMemory::vtable_of_valid_object(o) })
            .or_else(|| {
                self.detached
                    .borrow()
                    .values()
                    .find(|a| a.object() == o)
                    .map(|_| unsafe { ObjectMemory::vtable_of_valid_object(o) })
            })
            .ok_or(self.invalid())
    }
    pub fn scope<'a>(&'a self, r: Oop, selector: Oop, args: &'a [Oop]) -> Scope<'a> {
        let mut frames = self.active.borrow_mut();
        let start = frames.len();
        frames.push(CallFrame {
            receiver: r,
            selector,
            arguments: args.as_ptr(),
            count: args.len(),
            method: core::ptr::null_mut(),
            context: core::ptr::null_mut(),
            owner: core::ptr::null_mut(),
        });
        Scope {
            s: self,
            start,
            _arguments: core::marker::PhantomData,
        }
    }
    fn call_roots(&self) -> Vec<Oop> {
        let frames = self.active.borrow();
        let mut roots = Vec::new();
        for frame in frames.iter() {
            roots.extend([frame.receiver, frame.selector]);
            // Scope ties this borrowed argument area to the active call. No
            // frame or argument borrow survives the later visitor messages.
            roots.extend_from_slice(unsafe {
                core::slice::from_raw_parts(frame.arguments, frame.count)
            });
            if !frame.method.is_null() {
                roots.extend([frame.method, frame.context, frame.owner]);
            }
        }
        roots
    }
    pub unsafe fn invoke(&self, m: Oop, r: Oop, a: &[Oop], scope: &Scope<'_>) -> Result<Oop, Oop> {
        scope.entered(m, core::ptr::null_mut(), core::ptr::null_mut());
        let apply = self.selector(b"apply");
        let Some(applier) = (unsafe { id_dispatch::bind(self, m, apply) }?) else {
            return Err(self.unbound());
        };
        if applier == self.native_apply.get() {
            return unsafe { self.apply_native(m, r, a, scope) };
        }
        let mut forwarded = Vec::with_capacity(a.len() + 1);
        forwarded.push(r);
        forwarded.extend_from_slice(a);
        let call = self.scope(m, apply, &forwarded);
        unsafe { self.invoke(applier, m, &forwarded, &call) }
    }
    pub(super) unsafe fn send(&self, r: Oop, name: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe { id_dispatch::send(self, r, self.selector(name), a) }
    }
    pub fn initialize(&self) {
        for &(name, _, _) in OPERATIONS {
            let symbol = self.seed_symbol(name);
            self.names.borrow_mut().insert(name, symbol);
            self.permanent.borrow_mut().push(symbol);
        }
        for name in [
            b"visit:".as_slice(),
            b"allocation:bytes:identity:",
            b"binding:value:weak:",
            b"invalidRequest",
            b"unbound",
            b"mappingFailed",
            b"incompatibleModule",
            b"initializerFailed",
            b"trace:with:",
        ] {
            let symbol = self.seed_symbol(name);
            self.names.borrow_mut().insert(name, symbol);
            self.permanent.borrow_mut().push(symbol);
        }
        let invalid = self.make_fault(self.selector(b"invalidRequest"), core::ptr::null_mut());
        self.invalid_fault.set(invalid);
        let unbound = self.make_fault(self.selector(b"unbound"), core::ptr::null_mut());
        self.unbound_fault.set(unbound);
        self.permanent.borrow_mut().extend([invalid, unbound]);
        for &(name, action, entry) in OPERATIONS {
            let m = self.new_method(entry, self.root, self.root).unwrap();
            let sel = self.selector(name);
            let vt = match action {
                Action::Lookup
                | Action::Add
                | Action::Remove
                | Action::Allocate
                | Action::Delegated
                | Action::Bindings => self.meta,
                Action::Intern | Action::Load => self.name,
                Action::At | Action::Put | Action::SequenceSize => self.sequence,
                Action::MethodAt | Action::LiteralAt => self.module,
                Action::WithContext | Action::Invoke | Action::ApplyNative => self.method,
                Action::FaultKind | Action::FaultDetail => self.fault,
                _ => self.object,
            };
            if matches!(action, Action::Lookup | Action::Add | Action::ApplyNative) {
                self.tables.borrow_mut().add_method(vt, sel, m);
            } else {
                unsafe { self.send(vt, b"addMethod", &[sel, m]) }.unwrap();
            }
            if matches!(action, Action::ApplyNative) {
                self.native_apply.set(m);
                self.permanent.borrow_mut().push(m);
            }
            if matches!(action, Action::Lookup) {
                self.default_lookup.set(m);
                self.permanent.borrow_mut().push(m);
            }
        }
        self.install_reference_owners();
    }
    #[inline(always)]
    unsafe fn perform(&self, action: Action, r: Oop, a: &[Oop]) -> Result<Oop, Oop> {
        let nil = core::ptr::null_mut();
        let invalid = self.invalid();
        let one = || if a.len() == 1 { Ok(a[0]) } else { Err(invalid) };
        let pair = || {
            if let [x, y] = a {
                Ok((*x, *y))
            } else {
                Err(invalid)
            }
        };
        match action {
            Action::ApplyNative => {
                let Some((&receiver, args)) = a.split_first() else {
                    return Err(invalid);
                };
                let call = self.scope(receiver, nil, args);
                unsafe { self.apply_native(r, receiver, args, &call) }
            }
            Action::ReferenceOwners => {
                let visitor = one()?;
                let owners = self.reference_owners.borrow().clone();
                for (behavior, owner) in owners {
                    unsafe { self.send(visitor, b"trace:with:", &[behavior, owner]) }?;
                }
                Ok(nil)
            }
            Action::NoReferences
            | Action::TableReferences
            | Action::MethodReferences
            | Action::FaultReferences
            | Action::SequenceReferences
            | Action::ModuleReferences => {
                let (object, visitor) = pair()?;
                let edges = self.owned_references(action, object)?;
                for edge in edges {
                    unsafe { self.send(visitor, b"visit:", &[edge]) }?;
                }
                Ok(nil)
            }
            Action::Lookup => {
                let sel = one()?;
                if !self.tables.borrow().is_registered(r) {
                    return Err(invalid);
                }
                Ok(self.tables.borrow().lookup(r, sel).unwrap_or(nil))
            }
            Action::Add => {
                let (sel, m) = pair()?;
                if !self.tables.borrow().is_registered(r) || m.is_null() {
                    return Err(invalid);
                }
                self.tables.borrow_mut().add_method(r, sel, m);
                Ok(m)
            }
            Action::Remove => {
                let sel = one()?;
                if !self.tables.borrow().is_registered(r) {
                    return Err(invalid);
                }
                Ok(self
                    .tables
                    .borrow_mut()
                    .remove_method(r, sel)
                    .unwrap_or(nil))
            }
            Action::Allocate => self.allocate(r, abi::as_size(one()?).ok_or(invalid)?),
            Action::Delegated => {
                if !a.is_empty() || !self.tables.borrow().is_registered(r) {
                    return Err(invalid);
                }
                let meta = self.behavior(r)?;
                Ok(self
                    .tables
                    .borrow_mut()
                    .create(&mut self.heap.borrow_mut(), r, meta))
            }
            Action::Behavior => self.behavior(one()?),
            Action::Reassociate => {
                let (o, behavior) = pair()?;
                if o.is_null()
                    || abi::as_size(o).is_some()
                    || !self.tables.borrow().is_registered(behavior)
                    || !self.heap.borrow_mut().set_vtable(o, behavior)
                {
                    return Err(invalid);
                }
                self.tables.borrow_mut().note_mutation();
                Ok(o)
            }
            Action::Parent => {
                let v = one()?;
                if !self.tables.borrow().is_registered(v) {
                    return Err(invalid);
                }
                Ok(self.tables.borrow().parent_of(v))
            }
            Action::ParentPut => {
                let (v, p) = pair()?;
                if !self.tables.borrow().is_registered(v)
                    || (!p.is_null() && !self.tables.borrow().is_registered(p))
                {
                    return Err(invalid);
                }
                self.tables.borrow_mut().set_parent(v, p);
                Ok(v)
            }
            Action::Identity => {
                let v = one()?;
                let h = self.heap.borrow();
                let identity = h
                    .get(v.addr())
                    .map(|a| a.identity())
                    .or_else(|| {
                        self.detached
                            .borrow()
                            .values()
                            .find(|a| a.payload().as_ptr().addr() == abi::heap_address(v))
                            .map(|a| a.identity())
                    })
                    .ok_or(invalid)?;
                abi::size(identity as usize).ok_or(invalid)
            }
            Action::Extent => {
                let v = one()?;
                let h = self.heap.borrow();
                let size = h
                    .get(v.addr())
                    .map(|a| a.requested_size())
                    .or_else(|| {
                        self.detached
                            .borrow()
                            .values()
                            .find(|a| a.payload().as_ptr().addr() == abi::heap_address(v))
                            .map(|a| a.requested_size())
                    })
                    .ok_or(invalid)?;
                abi::size(size).ok_or(invalid)
            }
            Action::Count => {
                if !a.is_empty() {
                    return Err(invalid);
                }
                abi::size(self.heap.borrow().len()).ok_or(invalid)
            }
            Action::AllocationEpoch => {
                abi::size(self.heap.borrow().allocation_epoch() as usize).ok_or(invalid)
            }
            Action::Epoch => {
                abi::size(self.tables.borrow().mutation_epoch() as usize).ok_or(invalid)
            }
            Action::BindingEpoch => {
                abi::size(self.tables.borrow().binding_epoch() as usize).ok_or(invalid)
            }
            Action::CoreObject => Ok(self.object),
            Action::CoreName => Ok(self.name),
            Action::NewName => self.name_from_values(a),
            Action::Intern => {
                if !a.is_empty() {
                    return Err(invalid);
                }
                let (p, n) = self.name_data(r)?;
                let bytes = unsafe { core::slice::from_raw_parts(p, n) };
                self.symbols
                    .borrow_mut()
                    .intern(&mut self.heap.borrow_mut(), self.symbol, bytes)
                    .map_err(|_| invalid)
            }
            Action::NewSequence => self.new_sequence(a),
            Action::SequenceSize => {
                if !a.is_empty() {
                    return Err(invalid);
                }
                abi::size(self.sequence_storage(r)?.1).ok_or(invalid)
            }
            Action::At => self.reference_at(r, abi::as_size(one()?).ok_or(invalid)?),
            Action::Put => {
                let (index, value) = pair()?;
                self.reference_put(r, abi::as_size(index).ok_or(invalid)?, value)
            }
            Action::Invoke => {
                let (receiver, seq) = pair()?;
                // Snapshot before a callback can mutate the source's fields.
                let args = self.sequence_values(seq)?;
                let scope = self.scope(receiver, nil, &args);
                unsafe { self.invoke(r, receiver, &args, &scope) }
            }
            Action::WithContext => {
                let ctx = one()?;
                let old = self.method_data(r)?;
                self.new_method(old.entry, ctx, old.owner)
            }
            Action::LiteralAt => {
                let at = abi::as_size(one()?).ok_or(invalid)?;
                self.modules
                    .borrow()
                    .get(&r.addr())
                    .and_then(|m| m.literals.get(at))
                    .copied()
                    .ok_or(invalid)
            }
            Action::MethodAt => {
                let sel = one()?;
                self.modules
                    .borrow()
                    .get(&r.addr())
                    .and_then(|m| m.methods.iter().find(|(s, _)| *s == sel))
                    .map(|(_, v)| *v)
                    .ok_or(invalid)
            }
            Action::MakeFault => {
                let (k, d) = pair()?;
                Ok(self.make_fault(k, d))
            }
            Action::FaultKind => Ok(self.fault_data(r)?.kind),
            Action::FaultDetail => Ok(self.fault_data(r)?.detail),
            Action::SelfValue => {
                if a.is_empty() {
                    Ok(r)
                } else {
                    Err(invalid)
                }
            }
            Action::Roots => {
                let visitor = one()?;
                let mut roots = self.permanent.borrow().clone();
                roots.extend(self.call_roots());
                for module in self.modules.borrow().values() {
                    roots.extend(&module.literals);
                    roots.extend(&module.symbols);
                    for (k, v) in &module.methods {
                        roots.extend([*k, *v]);
                    }
                    roots.push(module.result);
                }
                for v in roots {
                    unsafe { self.send(visitor, b"visit:", &[v]) }?;
                }
                Ok(nil)
            }
            Action::Allocations => {
                let visitor = one()?;
                let rows: Vec<_> = self
                    .heap
                    .borrow()
                    .iter()
                    .map(|(_, a)| (a.object(), a.requested_size(), a.identity()))
                    .collect();
                for (o, n, id) in rows {
                    unsafe {
                        self.send(
                            visitor,
                            b"allocation:bytes:identity:",
                            &[o, abi::size(n).unwrap(), abi::size(id as usize).unwrap()],
                        )
                    }?;
                }
                Ok(nil)
            }
            Action::References => {
                let (object, visitor) = pair()?;
                // The floor reports only the vtable. Payload references are
                // supplied by the registered owner's current methods.
                let behavior = self.behavior(object)?;
                unsafe { self.send(visitor, b"visit:", &[behavior]) }?;
                Ok(nil)
            }
            Action::Bindings => {
                let visitor = one()?;
                if !self.tables.borrow().is_registered(r) {
                    return Err(invalid);
                }
                let rows = {
                    let t = self.tables.borrow();
                    let rec = t.record(r);
                    rec.keys
                        .iter()
                        .copied()
                        .zip(rec.values.iter().copied())
                        .zip(rec.weak_keys.iter().copied())
                        .collect::<Vec<_>>()
                };
                for ((k, v), weak) in rows {
                    unsafe {
                        self.send(
                            visitor,
                            b"binding:value:weak:",
                            &[k, v, abi::size(weak as usize).unwrap()],
                        )
                    }?;
                }
                Ok(nil)
            }
            Action::Retire => {
                self.retire(a)?;
                Ok(nil)
            }
            Action::Detach => {
                let o = one()?;
                let a = self.heap.borrow_mut().detach(o.addr()).ok_or(invalid)?;
                let id = a.identity() as usize;
                self.detached.borrow_mut().insert(id, a);
                abi::size(id).ok_or(invalid)
            }
            Action::Release => {
                let id = abi::as_size(one()?).ok_or(invalid)?;
                self.detached.borrow_mut().remove(&id).ok_or(invalid)?;
                Ok(nil)
            }
            Action::Load => {
                let args = one()?;
                let args = self.sequence_values(args)?;
                unsafe { self.load_module(r, &args) }
            }
        }
    }
    fn retire(&self, objects: &[Oop]) -> Result<(), Oop> {
        let mut dead = AddressMap::default();
        for &o in objects {
            if o.is_null() || abi::as_size(o).is_some() {
                return Err(self.invalid());
            }
            self.heap.borrow().get(o.addr()).ok_or(self.invalid())?;
            dead.insert(abi::heap_address(o), ());
        }
        let behaviors: Vec<_> = self
            .tables
            .borrow()
            .ordered
            .iter()
            .copied()
            .filter(|o| dead.contains_key(&o.addr()))
            .collect();
        {
            let mut t = self.tables.borrow_mut();
            t.remove_weak_keys(&dead);
            t.remove_objects(&behaviors);
        }
        self.symbols.borrow_mut().remove_objects(&dead);
        // Object contents survive until storage release; only actual indexes
        // need clearing here. No per-kind state tables remain.
        Ok(())
    }
    unsafe fn load_module(&self, path: Oop, args: &[Oop]) -> Result<Oop, Oop> {
        let (p, n) = self.name_data(path)?;
        let path_bytes = unsafe { core::slice::from_raw_parts(p, n) };
        let Some(handle) = (unsafe { abi::native_module::open(path_bytes) }) else {
            return Err(self.make_fault(self.selector(b"mappingFailed"), path));
        };
        let descriptor = unsafe { abi::native_module::symbol(handle, c"birnam_native_module_v2") }
            .cast::<native::Module>();
        if descriptor.is_null() {
            unsafe { abi::native_module::close(handle) };
            return Err(self.make_fault(self.selector(b"incompatibleModule"), path));
        }
        if unsafe { (*descriptor).version } != native::VERSION
            || unsafe { (*descriptor).byte_size } as usize != size_of::<native::Module>()
        {
            unsafe { abi::native_module::close(handle) };
            return Err(self.make_fault(self.selector(b"incompatibleModule"), path));
        }
        let d = unsafe { &*descriptor };
        let Some(initialize) = d.initialize else {
            unsafe { abi::native_module::close(handle) };
            return Err(self.make_fault(self.selector(b"incompatibleModule"), path));
        };
        if d.literal_count > 65536 || d.method_count > 65536 {
            unsafe { abi::native_module::close(handle) };
            return Err(self.invalid());
        }
        let literals =
            unsafe { abi::borrowed_slice(d.literals, d.literal_count) }.ok_or(self.invalid())?;
        let methods =
            unsafe { abi::borrowed_slice(d.methods, d.method_count) }.ok_or(self.invalid())?;
        let image = self.allocate(self.module, 0)?;
        self.modules.borrow_mut().insert(
            image.addr(),
            ModuleRecord {
                _handle: handle,
                declarations: methods,
                literals: Vec::new(),
                symbols: Vec::new(),
                methods: Vec::new(),
                result: core::ptr::null_mut(),
            },
        );
        // Native images, including partly initialized ones, stay resident. Publication
        // cannot be rolled back. The bounded slice has no native image-unload policy.
        self.permanent.borrow_mut().push(image);
        for literal in literals {
            if literal.length > 16 * 1024 * 1024 {
                return Err(self.invalid());
            }
            let bytes = unsafe { abi::borrowed_slice(literal.bytes, literal.length) }
                .ok_or(self.invalid())?;
            let name = self.make_name(bytes)?;
            self.modules
                .borrow_mut()
                .get_mut(&image.addr())
                .unwrap()
                .literals
                .push(name);
        }
        let names = self
            .modules
            .borrow()
            .get(&image.addr())
            .unwrap()
            .literals
            .clone();
        for name in names {
            let symbol = unsafe { self.send(name, b"intern", &[]) }?;
            self.modules
                .borrow_mut()
                .get_mut(&image.addr())
                .unwrap()
                .symbols
                .push(symbol);
        }
        for method in methods {
            let selector = *self
                .modules
                .borrow()
                .get(&image.addr())
                .unwrap()
                .symbols
                .get(method.name)
                .ok_or(self.invalid())?;
            let method = self.new_method(
                method.entry.ok_or(self.invalid())?,
                core::ptr::null_mut(),
                image,
            )?;
            self.modules
                .borrow_mut()
                .get_mut(&image.addr())
                .unwrap()
                .methods
                .push((selector, method));
        }
        let names = self
            .modules
            .borrow()
            .get(&image.addr())
            .unwrap()
            .symbols
            .clone();
        let result = unsafe {
            initialize(
                &self.protocol,
                image,
                names.as_ptr(),
                names.len(),
                args.as_ptr(),
                args.len(),
            )
        };
        self.modules
            .borrow_mut()
            .get_mut(&image.addr())
            .unwrap()
            .result = result;
        self.behavior(result)?;
        if abi::is_fault(result) {
            Err(result)
        } else {
            self.behavior(result)?;
            Ok(result)
        }
    }
}
pub(crate) unsafe extern "C" fn message(
    root: Oop,
    r: Oop,
    selector: Oop,
    args: *const Oop,
    n: usize,
) -> Oop {
    let s = unsafe { &*root.cast::<*const State>().read() };
    let Some(args) = (unsafe { abi::borrowed_slice(args, n) }) else {
        return s.invalid();
    };
    unsafe { id_dispatch::send(s, r, selector, args) }.unwrap_or_else(|e| e)
}
// One runtime on the calling thread. Siphon performs this single native handoff.
static PHASE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
static INSTANCE: std::sync::atomic::AtomicPtr<State> =
    std::sync::atomic::AtomicPtr::new(core::ptr::null_mut());
pub(crate) unsafe fn bootstrap(seeds: *const native::Seed, n: usize) -> *const Protocol {
    use std::sync::atomic::Ordering;
    let Some(seeds) = (unsafe { abi::borrowed_slice(seeds, n) }) else {
        return core::ptr::null();
    };
    let mut inputs = Vec::new();
    for seed in seeds {
        if seed.length > 16 * 1024 * 1024 {
            return core::ptr::null();
        }
        let Some(bytes) = (unsafe { abi::borrowed_slice(seed.bytes, seed.length) }) else {
            return core::ptr::null();
        };
        if bytes.contains(&0) {
            return core::ptr::null();
        }
        inputs.push(bytes);
    }
    if PHASE.load(Ordering::Acquire) == 2 {
        let s = unsafe { &*INSTANCE.load(Ordering::Acquire) };
        if s.check().is_err() {
            return core::ptr::null();
        }
        if !inputs.is_empty() {
            let existing = s.sequence_values(s.protocol.seeds).unwrap();
            if existing.len() != inputs.len() {
                return core::ptr::null();
            }
            for (name, bytes) in existing.into_iter().zip(inputs) {
                let Ok((p, n)) = s.name_data(name) else {
                    return core::ptr::null();
                };
                if unsafe { core::slice::from_raw_parts(p, n) } != bytes {
                    return core::ptr::null();
                }
            }
        }
        return &s.protocol;
    }
    if PHASE
        .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return core::ptr::null();
    }
    let mut s = super::initial_objects::construct();
    let names: Vec<_> = inputs
        .into_iter()
        .map(|b| s.make_name(b).unwrap())
        .collect();
    let seq = s.new_sequence(&names).unwrap();
    s.permanent.borrow_mut().push(seq);
    s.protocol.seeds = seq;
    let raw = Box::into_raw(s);
    INSTANCE.store(raw, Ordering::Release);
    PHASE.store(2, Ordering::Release);
    unsafe { &(*raw).protocol }
}

#[cfg(test)]
mod lifetime_tests {
    use super::*;
    #[test]
    fn contents_survive_index_cleanup_and_detachment() {
        let s = super::super::initial_objects::construct();
        let name = s.make_name(b"kept contents").unwrap();
        let fields = s.new_sequence(&[name]).unwrap();
        let empty = s.new_sequence(&[]).unwrap();
        let fault = s.make_fault(s.selector(b"invalidRequest"), fields);
        let method = s.new_method(s.method_data(s.tables.borrow().lookup(s.object, s.selector(b"yourself")).unwrap()).unwrap().entry, s.root, s.root).unwrap();
        let objects = [name, fields, fault, method];
        s.retire(&objects).unwrap();
        let check = || unsafe {
            let (p, n) = s.name_data(name).unwrap();
            assert_eq!(core::slice::from_raw_parts(p, n), b"kept contents");
            assert_eq!(
                s.send(fields, b"at:", &[abi::size(0).unwrap()]).unwrap(),
                name
            );
            assert_eq!(s.send(fault, b"detail", &[]).unwrap(), fields);
            assert_eq!(
                s.send(method, b"invokeOn:arguments:", &[name, empty])
                    .unwrap(),
                name
            );
        };
        check();
        for object in objects {
            let allocation = s.heap.borrow_mut().detach(object.addr()).unwrap();
            let identity = allocation.identity() as usize;
            s.detached.borrow_mut().insert(identity, allocation);
        }
        check();
        s.detached.borrow_mut().clear();
        assert!(s.name_data(name).is_err());
        assert!(s.method_data(method).is_err());
        assert!(s.fault_data(fault).is_err());
        assert!(s.sequence_values(fields).is_err());
    }
    #[test]
    fn detached_object_with_released_vtable_is_not_dispatched() {
        let s = super::super::initial_objects::construct();
        let vt = s
            .tables
            .borrow_mut()
            .create(&mut s.heap.borrow_mut(), s.object, s.meta);
        let object = s.allocate(vt, 0).unwrap();
        let storage = s.heap.borrow_mut().detach(object.addr()).unwrap();
        s.detached
            .borrow_mut()
            .insert(storage.identity() as usize, storage);
        s.retire(&[vt]).unwrap();
        drop(s.heap.borrow_mut().detach(vt.addr()).unwrap());
        assert!(unsafe { s.send(object, b"yourself", &[]) }.is_err());
    }
}
