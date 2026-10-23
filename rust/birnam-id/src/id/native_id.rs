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
#[derive(Clone, Copy)]
pub(crate) struct MethodRecord {
    pub entry: Method,
    pub context: Oop,
    pub owner: Oop,
}
#[derive(Clone, Copy)]
pub(crate) struct FaultRecord {
    kind: Oop,
    detail: Oop,
}
pub(crate) struct ModuleRecord {
    _handle: *mut c_void,
    literals: Vec<Oop>,
    symbols: Vec<Oop>,
    methods: Vec<(Oop, Oop)>,
    result: Oop,
}
pub(crate) struct State {
    pub protocol: Protocol,
    pub heap: RefCell<ObjectMemory>,
    pub tables: RefCell<Vtables>,
    pub symbols: RefCell<Symbols>,
    pub root: Oop,
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
    pub methods: RefCell<AddressMap<MethodRecord>>,
    pub faults: RefCell<AddressMap<FaultRecord>>,
    pub sequences: RefCell<AddressMap<Vec<Oop>>>,
    pub modules: RefCell<AddressMap<ModuleRecord>>,
    pub name_objects: RefCell<AddressMap<usize>>,
    pub names: RefCell<HashMap<&'static [u8], Oop>>,
    pub detached: RefCell<AddressMap<Allocation>>,
    pub active: RefCell<Vec<Oop>>,
    pub permanent: RefCell<Vec<Oop>>,
    pub default_lookup: Cell<Oop>,
    pub invalid_fault: Cell<Oop>,
    pub unbound_fault: Cell<Oop>,
    pub owner: std::thread::ThreadId,
}
pub(crate) struct Scope<'a> {
    s: &'a State,
    start: usize,
}
impl Scope<'_> {
    pub fn keep(&self, o: Oop) {
        self.s.active.borrow_mut().push(o)
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.s.active.borrow_mut().truncate(self.start)
    }
}
#[derive(Clone, Copy)]
pub(crate) enum Action {
    Lookup,
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
        const OPERATIONS:&[(&[u8],Action,Method)]=&[$(($name,Action::$action,primitive::$action)),*];
    };
}
core_primitives![
    (b"lookup", Lookup),
    (b"addMethod", Add),
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
    (b"retireIndexesFor:", Retire),
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
    fn allocate(&self, behavior: Oop, size: usize) -> Result<Oop, Oop> {
        if !ObjectMemory::valid_payload_size(size) || !self.tables.borrow().is_registered(behavior)
        {
            return Err(self.invalid());
        }
        let mut h = self.heap.borrow_mut();
        let o = h.allocate(size);
        assert!(h.set_vtable(o, behavior));
        Ok(o)
    }
    pub fn make_name(&self, b: &[u8]) -> Result<Oop, Oop> {
        if b.contains(&0) {
            return Err(self.invalid());
        }
        let n = size_of::<usize>()
            .checked_add(b.len())
            .ok_or(self.invalid())?;
        let o = self.allocate(self.name, n)?;
        unsafe {
            o.cast::<usize>().write(b.len());
            core::ptr::copy_nonoverlapping(
                b.as_ptr(),
                o.cast::<u8>().add(size_of::<usize>()),
                b.len(),
            )
        };
        self.name_objects.borrow_mut().insert(o.addr(), b.len());
        Ok(o)
    }
    fn name_data(&self, o: Oop) -> Result<(*const u8, usize), Oop> {
        if !self.name_objects.borrow().contains_key(&o.addr()) {
            return Err(self.invalid());
        }
        let h = self.heap.borrow();
        let a = h.get(o.addr()).ok_or(self.invalid())?;
        if a.requested_size() < size_of::<usize>() {
            return Err(self.invalid());
        }
        let n = unsafe { o.cast::<usize>().read() };
        if n != a.requested_size() - size_of::<usize>() {
            return Err(self.invalid());
        }
        Ok((unsafe { o.cast::<u8>().add(size_of::<usize>()) }, n))
    }
    fn new_sequence(&self, values: Vec<Oop>) -> Result<Oop, Oop> {
        let o = self.allocate(self.sequence, 0)?;
        self.sequences.borrow_mut().insert(o.addr(), values);
        Ok(o)
    }
    fn new_method(&self, entry: Method, context: Oop, owner: Oop) -> Result<Oop, Oop> {
        let o = self.allocate(self.method, 0)?;
        self.methods.borrow_mut().insert(
            o.addr(),
            MethodRecord {
                entry,
                context,
                owner,
            },
        );
        Ok(o)
    }
    fn make_fault(&self, kind: Oop, detail: Oop) -> Oop {
        let o = self.allocate(self.fault, 0).expect("core fault allocation");
        self.heap.borrow_mut().mark_fault(o);
        self.faults
            .borrow_mut()
            .insert(o.addr(), FaultRecord { kind, detail });
        abi::fault_reference(o)
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
            .vtable_of(o)
            .or_else(|| {
                self.detached
                    .borrow()
                    .values()
                    .find(|a| a.payload().as_ptr().addr() == abi::heap_address(o))
                    .map(|_| unsafe { ObjectMemory::vtable_of_valid_object(o) })
            })
            .ok_or(self.invalid())
    }
    // The unsafe native boundary already requires live Oop arguments. Protect
    // them before lookup; validate receiver/primitive-specific shapes where
    // consumed. Rechecking every argument's behavior here adds duplicate Id
    // registry lookups without establishing its borrow lifetime.
    pub fn scope(&self, r: Oop, s: Oop, a: &[Oop]) -> Scope<'_> {
        let mut roots = self.active.borrow_mut();
        let start = roots.len();
        roots.push(r);
        roots.push(s);
        roots.extend_from_slice(a);
        Scope { s: self, start }
    }
    pub unsafe fn invoke(&self, m: Oop, r: Oop, a: &[Oop], scope: &Scope<'_>) -> Result<Oop, Oop> {
        let record = *self
            .methods
            .borrow()
            .get(&abi::heap_address(m))
            .ok_or(self.invalid())?;
        scope.keep(m);
        scope.keep(record.context);
        scope.keep(record.owner);
        let value = unsafe { (record.entry)(self.root, record.context, r, a.as_ptr(), a.len()) };
        if abi::is_fault(value) {
            if !self.faults.borrow().contains_key(&abi::heap_address(value)) {
                return Err(self.invalid());
            }
            Err(value)
        } else {
            self.behavior(value)?;
            Ok(value)
        }
    }
    unsafe fn send(&self, r: Oop, name: &[u8], a: &[Oop]) -> Result<Oop, Oop> {
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
                Action::WithContext | Action::Invoke => self.method,
                Action::FaultKind | Action::FaultDetail => self.fault,
                _ => self.object,
            };
            if matches!(action, Action::Lookup | Action::Add) {
                self.tables.borrow_mut().add_method(vt, sel, m);
            } else {
                unsafe { self.send(vt, b"addMethod", &[sel, m]) }.unwrap();
            }
            if matches!(action, Action::Lookup) {
                self.default_lookup.set(m);
            }
        }
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
            Action::Lookup => {
                let sel = one()?;
                if !self.tables.borrow().is_registered(r) {
                    return Err(invalid);
                }
                Ok(self.tables.borrow().lookup(r, sel).unwrap_or(nil))
            }
            Action::Add => {
                let (sel, m) = pair()?;
                if !self.tables.borrow().is_registered(r)
                    || !self.methods.borrow().contains_key(&abi::heap_address(m))
                {
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
            Action::NewName => {
                if a.iter()
                    .any(|&v| abi::as_size(v).is_none_or(|b| b == 0 || b > 255))
                {
                    return Err(invalid);
                }
                let bytes = size_of::<usize>().checked_add(a.len()).ok_or(invalid)?;
                let name = self.allocate(self.name, bytes)?;
                unsafe {
                    name.cast::<usize>().write(a.len());
                    for (i, &v) in a.iter().enumerate() {
                        name.cast::<u8>()
                            .add(size_of::<usize>() + i)
                            .write(abi::as_size(v).unwrap() as u8);
                    }
                }
                self.name_objects.borrow_mut().insert(name.addr(), a.len());
                Ok(name)
            }
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
            Action::NewSequence => self.new_sequence(a.to_vec()),
            Action::SequenceSize => {
                if !a.is_empty() {
                    return Err(invalid);
                }
                abi::size(self.sequences.borrow().get(&r.addr()).ok_or(invalid)?.len())
                    .ok_or(invalid)
            }
            Action::At => {
                let at = abi::as_size(one()?).ok_or(invalid)?;
                self.sequences
                    .borrow()
                    .get(&r.addr())
                    .and_then(|v| v.get(at))
                    .copied()
                    .ok_or(invalid)
            }
            Action::Put => {
                let (at, v) = pair()?;
                let at = abi::as_size(at).ok_or(invalid)?;
                let mut seq = self.sequences.borrow_mut();
                *seq.get_mut(&r.addr())
                    .and_then(|v| v.get_mut(at))
                    .ok_or(invalid)? = v;
                Ok(v)
            }
            Action::Invoke => {
                let (receiver, seq) = pair()?;
                let args = self
                    .sequences
                    .borrow()
                    .get(&seq.addr())
                    .cloned()
                    .ok_or(invalid)?;
                let scope = self.scope(receiver, nil, &args);
                unsafe { self.invoke(r, receiver, &args, &scope) }
            }
            Action::WithContext => {
                let ctx = one()?;
                let old = *self.methods.borrow().get(&r.addr()).ok_or(invalid)?;
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
            Action::FaultKind => Ok(self
                .faults
                .borrow()
                .get(&abi::heap_address(r))
                .ok_or(invalid)?
                .kind),
            Action::FaultDetail => Ok(self
                .faults
                .borrow()
                .get(&abi::heap_address(r))
                .ok_or(invalid)?
                .detail),
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
                roots.extend(self.active.borrow().iter().copied());
                for v in roots {
                    unsafe { self.send(visitor, b"visit:", &[v]) }?;
                }
                Ok(nil)
            }
            Action::Allocations => {
                let visitor = one()?;
                let mut rows: Vec<_> = self
                    .heap
                    .borrow()
                    .iter()
                    .map(|(_, a)| (a.object(), a.requested_size(), a.identity()))
                    .collect();
                rows.sort_by_key(|v| v.2);
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
                let (o, visitor) = pair()?;
                let (edges, kind) = self.edges(o)?;
                for edge in edges {
                    unsafe { self.send(visitor, b"visit:", &[edge]) }?;
                }
                Ok(abi::size(kind).unwrap())
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
                let seq = one()?;
                let dead = self
                    .sequences
                    .borrow()
                    .get(&seq.addr())
                    .cloned()
                    .ok_or(invalid)?;
                self.retire(&dead)?;
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
                let args = self
                    .sequences
                    .borrow()
                    .get(&args.addr())
                    .cloned()
                    .ok_or(invalid)?;
                unsafe { self.load_module(r, &args) }
            }
        }
    }
    fn edges(&self, o: Oop) -> Result<(Vec<Oop>, usize), Oop> {
        let mut edges = vec![self.behavior(o)?];
        let address = abi::heap_address(o);
        if o.is_null() || abi::as_size(o).is_some() {
            return Ok((edges, 2));
        }
        if self.tables.borrow().is_registered(o) {
            let t = self.tables.borrow();
            let rec = t.record(o);
            edges.push(rec.parent);
            for ((k, v), weak) in rec.keys.iter().zip(&rec.values).zip(&rec.weak_keys) {
                if !weak {
                    edges.push(*k);
                    edges.push(*v)
                }
            }
            return Ok((edges, 1));
        }
        if let Some(m) = self.methods.borrow().get(&address) {
            edges.extend([m.context, m.owner]);
            return Ok((edges, 2));
        }
        if let Some(f) = self.faults.borrow().get(&address) {
            edges.extend([f.kind, f.detail]);
            return Ok((edges, 2));
        }
        if let Some(v) = self.sequences.borrow().get(&address) {
            edges.extend(v);
            return Ok((edges, 2));
        }
        if let Some(m) = self.modules.borrow().get(&address) {
            edges.extend(&m.literals);
            edges.extend(&m.symbols);
            for (k, v) in &m.methods {
                edges.extend([*k, *v]);
            }
            edges.push(m.result);
            return Ok((edges, 2));
        }
        if o == self.root
            || self.symbols.borrow().bytes_of(o).is_some()
            || self.name_objects.borrow().contains_key(&address)
        {
            return Ok((edges, 2));
        }
        Ok((edges, 0))
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
        for address in dead.keys() {
            self.methods.borrow_mut().remove(address);
            self.faults.borrow_mut().remove(address);
            self.sequences.borrow_mut().remove(address);
            self.name_objects.borrow_mut().remove(address);
        }
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
            let existing = s
                .sequences
                .borrow()
                .get(&s.protocol.seeds.addr())
                .unwrap()
                .clone();
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
    let names = inputs
        .into_iter()
        .map(|b| s.make_name(b).unwrap())
        .collect();
    let seq = s.new_sequence(names).unwrap();
    s.permanent.borrow_mut().push(seq);
    s.protocol.seeds = seq;
    let raw = Box::into_raw(s);
    INSTANCE.store(raw, Ordering::Release);
    PHASE.store(2, Ordering::Release);
    unsafe { &(*raw).protocol }
}

#[cfg(test)]
mod oop_tests {
    use super::*;
    #[test]
    fn fault_identity_survives_index_retirement_and_detachment() {
        let s = super::super::initial_objects::construct();
        let fault = s.make_fault(s.selector(b"invalidRequest"), abi::size(7).unwrap());
        assert!(abi::is_fault(fault));
        let id = s.heap.borrow().get(fault.addr()).unwrap().identity();
        s.retire(&[fault]).unwrap();
        assert_eq!(s.heap.borrow().get(fault.addr()).unwrap().object(), fault);
        let detached = s.heap.borrow_mut().detach(fault.addr()).unwrap();
        assert_eq!(detached.object(), fault);
        assert_eq!(detached.identity(), id);
    }
}
