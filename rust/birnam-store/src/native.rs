//! Native integration. Dependency operations and GC enumeration use Id sends.
//! Only Store reads fact/Cell state; no Store borrow spans a callback.
use super::*;
use birnam_abi::{
    self as abi, borrowed_slice,
    id_protocol::{Literal, Method, MethodSpec, Module, Protocol},
};
use core::cell::{Cell, RefCell};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicPtr, Ordering},
};

struct State {
    id: Protocol,
    namespace: Oop,
    roots: Oop,
    gc: Oop,
    store_vt: Oop,
    cell_vt: Oop,
    receiver: Cell<Oop>,
    method_observer: Cell<Oop>,
    method_authorizer: Cell<Oop>,
    policy: Runtime,
    names: HashMap<&'static [u8], Oop>,
    values: HashMap<&'static [u8], Oop>,
    stores: RefCell<HashMap<Oop, StoreState>>,
    cells: RefCell<HashMap<Oop, [Oop; 3]>>,
    // Invocation-local snapshots must survive callbacks which remove their
    // original Store edges and collect. Install them before the first send.
    active: RefCell<Vec<Vec<Oop>>>,
    ready: Cell<bool>,
    thread: std::thread::ThreadId,
}
static INSTANCE: AtomicPtr<State> = AtomicPtr::new(ptr::null_mut());
static API: AtomicPtr<Protocol> = AtomicPtr::new(ptr::null_mut());
fn invalid() -> Oop {
    let p = API.load(Ordering::Acquire);
    if p.is_null() {
        std::process::abort();
    }
    unsafe { abi::object_reference((*p).invalid) }
}
fn object(v: Oop) -> Result<Oop, Oop> {
    if v.is_null() { Err(invalid()) } else { Ok(v) }
}
fn word(v: usize) -> Result<Oop, Oop> {
    abi::size(v).ok_or_else(invalid)
}
fn done(v: Result<Oop, Oop>) -> Oop {
    v.unwrap_or_else(abi::fault_reference)
}
unsafe fn state(c: Oop) -> Result<&'static State, Oop> {
    unsafe {
        let p = INSTANCE.load(Ordering::Acquire);
        if p.is_null() {
            return Err(invalid());
        }
        let s = &*p;
        if s.namespace != c || s.thread != std::thread::current().id() {
            return Err(invalid());
        }
        Ok(s)
    }
}
struct Scope<'a> {
    s: &'a State,
    index: usize,
}
impl<'a> Scope<'a> {
    fn new(s: &'a State, values: Vec<Oop>) -> Self {
        let mut active = s.active.borrow_mut();
        let index = active.len();
        active.push(values);
        Self { s, index }
    }
    fn keep(&self, value: Oop) -> Oop {
        self.s.active.borrow_mut()[self.index].push(value);
        value
    }
    fn keep_all(&self, values: impl IntoIterator<Item = Oop>) {
        self.s.active.borrow_mut()[self.index].extend(values);
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        let mut active = self.s.active.borrow_mut();
        assert_eq!(active.len(), self.index + 1);
        active.pop();
    }
}
impl State {
    unsafe fn send(&self, r: Oop, n: &'static [u8], a: &[Oop]) -> Result<Oop, Oop> {
        unsafe { self.id.send(r, self.names[n], a) }
    }
    fn nil(&self) -> Oop {
        self.values[b"nil".as_slice()]
    }
    fn boolean(&self, yes: bool) -> Oop {
        self.values[if yes {
            b"true".as_slice()
        } else {
            b"false".as_slice()
        }]
    }
    unsafe fn integer(&self, n: usize) -> Result<Oop, Oop> {
        unsafe {
            self.send(
                self.values[b"Integer".as_slice()],
                b"fromWord:",
                &[word(n)?],
            )
        }
    }
    unsafe fn number(&self, value: Oop) -> Result<i64, Oop> {
        unsafe {
            let q = Scope::new(self, vec![value]);
            let bits =
                q.keep(self.send(self.values[b"Integer".as_slice()], b"wordOf:", &[value])?);
            if let Some(n) = abi::as_size(bits) {
                return Ok(n as i64);
            }
            object(bits)?;
            let low = abi::as_size(self.send(bits, b"low32", &[])?)
                .filter(|n| *n <= u32::MAX as usize)
                .ok_or_else(invalid)?;
            let high = abi::as_size(self.send(bits, b"high32", &[])?)
                .filter(|n| *n <= u32::MAX as usize)
                .ok_or_else(invalid)?;
            Ok(((low as u64) | ((high as u64) << 32)) as i64)
        }
    }
    unsafe fn error(&self, kind: &'static [u8]) -> Result<Oop, Oop> {
        unsafe {
            let factory = self.send(self.namespace, b"at:", &[self.names[b"Error".as_slice()]])?;
            self.send(object(factory)?, b"kind:", &[self.names[kind]])
        }
    }
    unsafe fn allocate(&self, scope: &Scope<'_>, vt: Oop) -> Result<Oop, Oop> {
        let o = scope.keep(object(unsafe { self.send(vt, b"allocate", &[word(0)?]) }?)?);
        unsafe { self.validate_allocation(o, vt) }?;
        Ok(o)
    }
    unsafe fn validate_allocation(&self, o: Oop, vt: Oop) -> Result<(), Oop> {
        unsafe {
            if self.send(self.id.root, b"behaviorOf:", &[o])? != vt
                || abi::as_size(self.send(self.id.root, b"allocationSizeOf:", &[o])?) != Some(0)
                || self.stores.borrow().contains_key(&o)
                || self.cells.borrow().contains_key(&o)
            {
                return Err(invalid());
            }
            Ok(())
        }
    }
    unsafe fn fresh(&self, scope: &Scope<'_>) -> Result<Oop, Oop> {
        let o = unsafe { self.allocate(scope, self.store_vt) }?;
        self.stores.borrow_mut().insert(o, self.seeded());
        Ok(o)
    }
    fn seeded(&self) -> StoreState {
        let mut store = StoreState::new();
        for e in [
            as_oop(self.policy.cardinality),
            self.names[b"name".as_slice()],
            as_oop(self.policy.history),
        ] {
            append_fact(
                &mut store,
                e,
                as_oop(self.policy.cardinality),
                as_oop(self.policy.one),
                Op::Assert,
                false,
            );
        }
        store.raw = false;
        store.transitions_since_compaction = 0;
        store
    }
    fn read<T>(&self, r: Oop, f: impl FnOnce(&StoreState) -> T) -> Result<T, Oop> {
        self.stores.borrow().get(&r).map(f).ok_or_else(invalid)
    }
    fn edit<T>(&self, r: Oop, f: impl FnOnce(&mut StoreState) -> T) -> Result<T, Oop> {
        self.stores
            .borrow_mut()
            .get_mut(&r)
            .map(|state| {
                state.revision = state
                    .revision
                    .checked_add(1)
                    .expect("Store revision exhausted");
                f(state)
            })
            .ok_or_else(invalid)
    }
}

// Raw native revisions belong to this owner, independently of durable fact basis.
unsafe extern "C" fn image_revision(_: Oop, c: Oop, r: Oop, _: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        if n != 0 {
            return Err(invalid());
        }
        word(state(c)?.read(r, |st| st.revision)?)
    })())
}
unsafe fn visit(s: &State, visitor: Oop, o: Oop) -> Result<(), Oop> {
    unsafe { s.send(visitor, b"visit:", &[o]) }?;
    Ok(())
}
unsafe extern "C" fn roots(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        let Some([v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let v = object(*v)?;
        let mut values = vec![
            s.namespace,
            s.roots,
            s.gc,
            s.store_vt,
            s.cell_vt,
            s.receiver.get(),
            s.method_observer.get(),
            s.method_authorizer.get(),
        ];
        values.extend(s.names.values().copied());
        values.extend(s.values.values().copied());
        values.extend(s.active.borrow().iter().flatten().copied());
        for o in values {
            visit(s, v, o)?;
        }
        Ok(ptr::null_mut())
    })())
}
unsafe extern "C" fn references(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        let Some([o, v]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let o = object(*o)?;
        let v = object(*v)?;
        let values = if let Some(store) = s.stores.borrow().get(&o) {
            store
                .facts
                .iter()
                .flat_map(|f| [f.e, f.a, f.v])
                .chain(
                    store
                        .compensation_reservations
                        .iter()
                        .map(|r| as_oop(r.owner)),
                )
                .collect::<Vec<_>>()
        } else if let Some(cell) = s.cells.borrow().get(&o) {
            cell.to_vec()
        } else {
            Vec::new()
        };
        for edge in values {
            visit(s, v, edge)?;
        }
        Ok(ptr::null_mut())
    })())
}
unsafe extern "C" fn retire(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        let values = borrowed_slice(a, n).ok_or_else(invalid)?;
        for o in values {
            s.stores.borrow_mut().remove(o);
            s.cells.borrow_mut().remove(o);
        }
        Ok(ptr::null_mut())
    })())
}
// Snapshot callbacks copy Oops into Store's current invocation scope, without
// managed allocation or foreign payload inspection during enumeration.
unsafe extern "C" fn allocation(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        let [o, _, _] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        s.active
            .borrow_mut()
            .last_mut()
            .ok_or_else(invalid)?
            .push(*o);
        Ok(ptr::null_mut())
    })())
}
unsafe extern "C" fn binding(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        let [selector, method, _] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        s.active
            .borrow_mut()
            .last_mut()
            .ok_or_else(invalid)?
            .extend_from_slice(&[*selector, *method]);
        Ok(ptr::null_mut())
    })())
}
unsafe fn bindings(s: &State, q: &Scope<'_>, behavior: Oop) -> Result<Vec<(Oop, Oop)>, Oop> {
    unsafe {
        q.keep(behavior);
        let start = s.active.borrow()[q.index].len();
        s.send(behavior, b"bindingsDo:", &[s.receiver.get()])?;
        let rows = s.active.borrow()[q.index][start..].to_vec();
        if rows.len() % 2 != 0 {
            return Err(invalid());
        }
        Ok(rows.chunks_exact(2).map(|r| (r[0], r[1])).collect())
    }
}

#[derive(Clone, Copy)]
enum OpCode {
    New,
    Raw,
    Flip,
    Assert,
    Retract,
    Read,
    RawRead,
    ReadElse,
    Has,
    Count,
    AttrCount,
    AttrAt,
    History,
    Basis,
    Oldest,
    Facts,
    Bytes,
    Reserved,
    ReservedBytes,
    ReservedFor,
    ReserveCompensation,
    ReleaseCompensation,
    Limit,
    RecordBytes,
    Policy,
    PolicyArg,
    CurrentOnly,
    Since,
    Last,
    Compact,
    AsOf,
    Triples,
    TriplesAsOf,
    TriplesSince,
    TriplesBetween,
    Cardinality,
    Cell,
    CellValue,
    CellSet,
    CellHistory,
    CellImageField,
    CellImageRestore,
    ImageFacts,
    ImageBegin,
    ImageAdd,
    ImageEnd,
    ImageSwap,
    SetObserver,
    SetAuthorizer,
    AddMethod,
    RemoveMethod,
    Backfill,
    DictionaryChanged,
}
macro_rules! entry {
    ($name:ident,$op:ident,$arity:expr) => {
        unsafe extern "C" fn $name(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
            done((|| unsafe {
                let s = state(c)?;
                let a = borrowed_slice(a, n)
                    .filter(|a| a.len() == $arity)
                    .ok_or_else(invalid)?;
                let args = a
                    .iter()
                    .map(|v| object(*v))
                    .collect::<Result<Vec<_>, _>>()?;
                let scope = Scope::new(s, std::iter::once(r).chain(args.iter().copied()).collect());
                execute(s, &scope, r, &args, OpCode::$op)
            })())
        }
    };
}

// A candidate inverse is a compare-and-restore of one single-valued fact.
// A null Oop denotes absence, distinct from the language nil object. Consume its
// pre-reserved capacity and perform the inverse under one Store borrow, with no
// outgoing send between them. Callback facts on other keys remain untouched.
unsafe extern "C" fn restore_value(_: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    done((|| unsafe {
        let s = state(c)?;
        let Some([entity, attribute, expected, previous, owner]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let [entity, attribute, owner] = [object(*entity)?, object(*attribute)?, object(*owner)?];
        let expected = *expected;
        let previous = *previous;
        let _scope = Scope::new(
            s,
            [r, entity, attribute, owner, expected, previous]
                .into_iter()
                .filter(|o| !o.is_null())
                .collect(),
        );
        s.send(s.gc, b"prepareAllocation", &[])?;
        let result = s.edit(r, |st| {
            if cardinality(&s.policy, st, attribute) != s.policy.one
                || current_value_raw(st, entity, attribute) != expected
            {
                return false;
            }
            if !consume_compensation_in(st, owner.addr(), 1) {
                return false;
            }
            if expected == previous {
                return true;
            }
            let (value, op) = if previous.is_null() {
                (expected, super::Op::Retract)
            } else {
                (previous, super::Op::Assert)
            };
            let historied = st.raw
                || is_schema_fact(&s.policy, attribute)
                || history_enabled(&s.policy, st, attribute);
            // Reserving charged this record before the forward write. Other
            // mutations cannot spend it; native allocation failure is fatal by
            // the same rule as ordinary fact append.
            append_fact(st, entity, attribute, value, op, historied);
            true
        })?;
        if result {
            Ok(r)
        } else {
            s.error(b"compensationConflict")
        }
    })())
}

unsafe fn execute(s: &State, scope: &Scope<'_>, r: Oop, a: &[Oop], op: OpCode) -> Result<Oop, Oop> {
    use OpCode::*;
    unsafe {
        let p = &s.policy;
        // Mutations and native Vec construction obey the same phase boundary as
        // ordinary object construction. Callback inputs are rooted before it.
        if matches!(
            op,
            New | Raw
                | Flip
                | Assert
                | Retract
                | CurrentOnly
                | Since
                | Last
                | Compact
                | Cell
                | CellSet
                | CellImageRestore
                | ImageSwap
                | ImageBegin
                | ImageAdd
                | ImageEnd
                | SetObserver
                | SetAuthorizer
                | AddMethod
                | RemoveMethod
                | Backfill
                | ReserveCompensation
        ) {
            s.send(s.gc, b"prepareAllocation", &[])?;
        }
        match op {
            New => return Ok(s.fresh(scope)?),
            Cell => {
                s.read(r, |_| ())?;
                let cell = s.allocate(scope, s.cell_vt)?;
                s.cells.borrow_mut().insert(cell, [r, a[0], a[1]]);
                return Ok(cell);
            }
            CellImageField => {
                let index = s.number(a[0])? as usize;
                let fields = *s.cells.borrow().get(&r).ok_or_else(invalid)?;
                return Ok(*fields.get(index).ok_or_else(invalid)?);
            }
            CellImageRestore => {
                s.read(a[0], |_| ())?;
                *s.cells.borrow_mut().get_mut(&r).ok_or_else(invalid)? = [a[0], a[1], a[2]];
                return Ok(r);
            }
            CellValue | CellSet | CellHistory => {
                let [store, e, attr] = *s.cells.borrow().get(&r).ok_or_else(invalid)?;
                scope.keep_all([store, e, attr]);
                let mut args = vec![e, attr];
                let selector = match op {
                    CellValue => b"currentValue:attr:".as_slice(),
                    CellHistory => b"history:attr:",
                    _ => {
                        args.push(a[0]);
                        b"assert:attr:value:"
                    }
                };
                let result = s.send(store, selector, &args)?;
                return Ok(if matches!(op, CellSet) && result == store {
                    r
                } else {
                    result
                });
            }
            _ => {
                s.read(r, |_| ())?;
            }
        }
        match op {
            SetObserver | SetAuthorizer => {
                let slot = if matches!(op, SetObserver) {
                    &s.method_observer
                } else {
                    &s.method_authorizer
                };
                if !slot.get().is_null() && slot.get() != a[0] {
                    return s.error(if matches!(op, SetObserver) {
                        b"methodMutationObserverAlreadySet"
                    } else {
                        b"methodMutationAuthorizerAlreadySet"
                    });
                }
                slot.set(a[0]);
            }
            DictionaryChanged => {
                // Explicit invalidation after a dictionary edit.
                // No sends or Store borrows escape this invalidation operation.
                for st in s.stores.borrow_mut().values_mut() {
                    st.revision = st
                        .revision
                        .checked_add(1)
                        .expect("Store revision exhausted");
                    let old = current_value_raw(st, a[1], a[0]);
                    if !old.is_null() {
                        remove_mirror_facts(st, a[1], a[0], old);
                    }
                }
            }
            AddMethod | RemoveMethod => {
                let authorizer = s.method_authorizer.get();
                if !authorizer.is_null() {
                    scope.keep(authorizer);
                    let decision = s.send(authorizer, b"value", &[])?;
                    if decision != as_oop(p.true_value) {
                        return Ok(decision);
                    }
                }
                let selector = a[0];
                let behavior = a[1];
                if matches!(op, RemoveMethod) {
                    if s.send(s.id.root, b"ownerOf:", &[behavior])?.is_null()
                        && s.send(s.namespace, b"nameOf:", &[behavior])?.is_null()
                    {
                        return s.error(b"unstableMethodTarget");
                    }
                    if !bindings(s, scope, behavior)?
                        .iter()
                        .any(|(key, _)| *key == selector)
                    {
                        return s.error(b"missingDirectMethod");
                    }
                }
                s.edit(r, |st| {
                    let old = current_value_raw(st, behavior, selector);
                    if !old.is_null() {
                        remove_mirror_facts(st, behavior, selector, old);
                    }
                })?;
                if matches!(op, AddMethod) {
                    let result = s.send(behavior, b"addMethod:as:", &[a[2], selector])?;
                    if result != a[2] {
                        return Ok(result);
                    }
                    s.edit(r, |st| append_mirror_fact(st, behavior, selector, a[2]))?;
                } else {
                    s.send(behavior, b"removeMethod:", &[selector])?;
                }
                let observer = s.method_observer.get();
                if !observer.is_null() {
                    scope.keep(observer);
                    s.send(observer, b"managedMethodChanged:on:", &[selector, behavior])?;
                }
            }
            Backfill => {
                let start = s.active.borrow()[scope.index].len();
                s.send(s.id.root, b"allocationsDo:", &[s.receiver.get()])?;
                let objects = s.active.borrow()[scope.index][start..].to_vec();
                let mut pairs = Vec::new();
                for behavior in objects {
                    if abi::as_size(s.send(s.id.root, b"isBehavior:", &[behavior])?) != Some(1) {
                        continue;
                    }
                    let mut ancestor = behavior;
                    let mut ordinary = false;
                    while !ancestor.is_null() {
                        if ancestor == s.values[b"ObjectVt".as_slice()] {
                            ordinary = true;
                            break;
                        }
                        ancestor = scope.keep(s.send(s.id.root, b"parentOf:", &[ancestor])?);
                    }
                    if ordinary {
                        pairs.extend(
                            bindings(s, scope, behavior)?
                                .into_iter()
                                .map(|(selector, _)| (behavior, selector)),
                        );
                    }
                }
                s.edit(r, |st| {
                    for (e, a) in pairs {
                        if current_value_raw(st, e, a).is_null() {
                            st.facts.push(Fact {
                                e,
                                a,
                                v: s.names[b"primitive".as_slice()],
                                tx: st.tx.saturating_sub(1),
                                op: super::Op::Assert,
                                historied: false,
                                mirror: true,
                            });
                        }
                    }
                })?;
            }
            ReservedFor => {
                let count = s.read(r, |st| {
                    st.compensation_reservations
                        .iter()
                        .find(|e| e.owner == a[0].addr())
                        .map_or(0, |e| e.facts)
                })?;
                return s.integer(count);
            }
            ReserveCompensation => {
                let count = s.number(a[1])?;
                if count <= 0 {
                    return s.error(b"invalidCompensation");
                }
                if s.edit(r, |st| {
                    reserve_compensation_in(p, st, a[0].addr(), count as usize)
                })?
                .is_err()
                {
                    return s.error(b"historyResourceLimit");
                }
            }
            ReleaseCompensation => {
                s.edit(r, |st| {
                    st.compensation_reservations
                        .retain(|entry| entry.owner != a[0].addr())
                })?;
            }
            Raw | Assert | Retract => {
                let result = s.edit(r, |st| {
                    let [e, attr, v] = [a[0], a[1], a[2]];
                    let retraction = matches!(op, Retract);
                    if retraction && !is_member(st, e, attr, v) {
                        return Ok(());
                    }
                    let many = cardinality(p, st, attr) == p.many;
                    if matches!(op, Assert)
                        && !st.raw
                        && (if many {
                            is_member(st, e, attr, v)
                        } else {
                            current_value_raw(st, e, attr) == v
                        })
                    {
                        return Ok(());
                    }
                    let operation = if retraction {
                        super::Op::Retract
                    } else {
                        super::Op::Assert
                    };
                    if matches!(op, Raw)
                        || (!retraction && st.raw)
                        || is_schema_fact(p, attr)
                        || history_enabled(p, st, attr)
                    {
                        prepare_semantic_mutation(p, st, true)?;
                        append_fact(st, e, attr, v, operation, true);
                        Ok(())
                    } else if many {
                        record_projection_many(p, st, e, attr, v, operation)
                    } else {
                        record_projection_one(p, st, e, attr, v, operation)
                    }
                })?;
                if result.is_err() {
                    return s.error(b"historyResourceLimit");
                }
            }
            Flip => s.edit(r, |st| st.raw = false)?,
            Read | RawRead | ReadElse => {
                let v = s.read(r, |st| current_value_raw(st, a[0], a[1]))?;
                if !v.is_null() {
                    return Ok(v);
                }
                return match op {
                    RawRead => Ok(ptr::null_mut()),
                    ReadElse => s.send(a[2], b"value", &[]),
                    _ => Ok(s.nil()),
                };
            }
            Has | Count => {
                let count = s.read(r, |st| {
                    if cardinality(p, st, a[1]) == p.many {
                        count_values(st, a[0], a[1])
                    } else {
                        usize::from(!current_value_raw(st, a[0], a[1]).is_null())
                    }
                })?;
                return if matches!(op, Has) {
                    Ok(s.boolean(count != 0))
                } else {
                    s.integer(count)
                };
            }
            AttrCount | AttrAt => {
                let index = if matches!(op, AttrAt) {
                    Some(s.number(a[1])? as usize)
                } else {
                    None
                };
                let attrs = s.read(r, |st| live_attributes(st, a[0], p))?;
                return if let Some(i) = index {
                    Ok(attrs.get(i).map_or(ptr::null_mut(), |o| *o))
                } else {
                    s.integer(attrs.len())
                };
            }
            History | Basis | Oldest | Facts | Bytes | Reserved | ReservedBytes | Limit
            | RecordBytes | Policy | PolicyArg => {
                let n = s.read(r, |st| match op {
                    History => st
                        .facts
                        .iter()
                        .filter(|f| f.e == a[0] && f.a == a[1])
                        .count(),
                    Basis => st.tx as usize,
                    Oldest => st.oldest_retained_basis as usize,
                    Facts => st.facts.len(),
                    Bytes => semantic_fact_count(st) * size_of::<Fact>(),
                    Reserved => reserved_compensation_fact_count(st),
                    ReservedBytes => {
                        reserved_compensation_fact_count(st).saturating_mul(size_of::<Fact>())
                    }
                    Limit => p.fact_bytes_limit,
                    RecordBytes => size_of::<Fact>(),
                    Policy => match st.retention {
                        RetentionPolicy::CurrentOnly => 0,
                        RetentionPolicy::KeepSince(_) => 1,
                        RetentionPolicy::KeepLast(_) => 2,
                    },
                    PolicyArg => match st.retention {
                        RetentionPolicy::CurrentOnly => 0,
                        RetentionPolicy::KeepSince(n) | RetentionPolicy::KeepLast(n) => n as usize,
                    },
                    _ => unreachable!(),
                })?;
                return s.integer(n);
            }
            CurrentOnly => s.edit(r, |st| st.retention = RetentionPolicy::CurrentOnly)?,
            Since | Last => {
                let n = s.number(a[0])?;
                if n < 0
                    || matches!(op, Since)
                        && s.read(r, |st| {
                            (n as u64) > st.tx || (n as u64) < st.oldest_retained_basis
                        })?
                {
                    return s.error(b"historyNotRetained");
                }
                s.edit(r, |st| {
                    st.retention = if matches!(op, Since) {
                        RetentionPolicy::KeepSince(n as u64)
                    } else {
                        RetentionPolicy::KeepLast(n as u64)
                    }
                })?;
            }
            Compact => {
                if s.edit(r, |st| {
                    compact_state(
                        st,
                        as_oop(p.cardinality),
                        as_oop(p.history),
                        as_oop(p.true_value),
                        as_oop(p.one),
                        as_oop(p.many),
                    )
                })?
                .is_err()
                {
                    return s.error(b"historyCompactionResourceLimit");
                }
            }
            AsOf => {
                let basis = s.number(a[2])?;
                if basis < 0 {
                    return s.error(b"historyNotRetained");
                }
                let result = s.read(r, |st| {
                    if !relation_history_available(
                        st,
                        a[0],
                        a[1],
                        basis as u64,
                        as_oop(p.history),
                        as_oop(p.true_value),
                    ) {
                        return None;
                    }
                    let value = current_value_raw_as_of(st, a[0], a[1], Some(basis as u64));
                    if !value.is_null() {
                        return Some(value);
                    }
                    if !history_enabled(p, st, a[1])
                        && st
                            .facts
                            .iter()
                            .any(|f| f.e == a[0] && f.a == a[1] && f.tx >= basis as u64)
                    {
                        None
                    } else {
                        Some(s.nil())
                    }
                })?;
                return match result {
                    Some(v) => Ok(v),
                    None => s.error(b"historyNotRetained"),
                };
            }
            Triples | TriplesAsOf | TriplesSince | TriplesBetween => {
                let (lo, hi, block) = match op {
                    Triples => (None, None, a[0]),
                    TriplesAsOf => (None, Some(s.number(a[0])?), a[1]),
                    TriplesSince => (Some(s.number(a[0])?), None, a[1]),
                    _ => (Some(s.number(a[0])?), Some(s.number(a[1])?), a[2]),
                };
                let oldest = s.read(r, |st| st.oldest_retained_basis)?;
                if [lo, hi]
                    .into_iter()
                    .flatten()
                    .any(|v| v < 0 || (v as u64) < oldest)
                {
                    return s.error(b"historyNotRetained");
                }
                let triples = s.read(r, |st| {
                    collect_live_triples(p, st, lo.map(|n| n as u64), hi.map(|n| n as u64))
                })?;
                scope.keep_all(triples.iter().flat_map(|t| [t.e, t.a, t.v]));
                for t in triples {
                    s.send(block, b"value:value:value:", &[t.e, t.a, t.v])?;
                }
            }
            Cardinality => {
                return Ok(as_oop(s.read(r, |st| cardinality(p, st, a[0]))?));
            }
            ImageFacts => {
                let facts = s.read(r, |st| {
                    let transient = st
                        .facts
                        .iter()
                        .filter_map(|f| {
                            (current_value_raw(st, f.a, s.names[b"imageTransient".as_slice()])
                                == as_oop(p.true_value))
                            .then_some(f.a)
                        })
                        .collect::<Vec<_>>();
                    st.facts
                        .iter()
                        .filter(|f| durable_image_fact(f, &transient))
                        .copied()
                        .collect::<Vec<_>>()
                })?;
                scope.keep_all(facts.iter().flat_map(|f| [f.e, f.a, f.v]));
                for f in facts {
                    let tx = scope.keep(object(s.integer(f.tx as usize)?)?);
                    let op = scope.keep(object(s.integer(f.op as usize)?)?);
                    s.send(
                        a[0],
                        b"e:a:v:tx:op:historied:",
                        &[f.e, f.a, f.v, tx, op, s.boolean(f.historied)],
                    )?;
                }
            }
            ImageSwap => {
                if r == a[0] {
                    return Err(invalid());
                }
                let mut stores = s.stores.borrow_mut();
                if !stores.contains_key(&r) || !stores.contains_key(&a[0]) {
                    return Err(invalid());
                }
                let mut staged = stores.remove(&a[0]).ok_or_else(invalid)?;
                let current = stores.get_mut(&r).ok_or_else(invalid)?;
                let left = current.revision.checked_add(1).ok_or_else(invalid)?;
                let right = staged.revision.checked_add(1).ok_or_else(invalid)?;
                std::mem::swap(current, &mut staged);
                current.revision = left;
                staged.revision = right;
                stores.insert(a[0], staged);
            }
            ImageBegin => s.edit(r, |st| {
                st.facts.clear();
                st.compensation_reservations.clear();
                st.oldest_retained_basis = 0;
                st.retention = RetentionPolicy::CurrentOnly;
                st.transitions_since_compaction = 0;
            })?,
            ImageAdd => {
                let tx = s.number(a[3])?;
                let op = s.number(a[4])?;
                if tx < 0
                    || !matches!(op, 0 | 1)
                    || ![s.values[b"true".as_slice()], s.values[b"false".as_slice()]]
                        .contains(&a[5])
                {
                    return Err(invalid());
                }
                s.edit(r, |st| {
                    st.facts.push(Fact {
                        e: a[0],
                        a: a[1],
                        v: a[2],
                        tx: tx as u64,
                        op: if op == 0 {
                            super::Op::Assert
                        } else {
                            super::Op::Retract
                        },
                        historied: a[5] == as_oop(p.true_value),
                        mirror: false,
                    })
                })?;
            }
            ImageEnd => {
                let tx = s.number(a[0])?;
                let oldest = s.number(a[2])?;
                let policy = s.number(a[3])?;
                let argument = s.number(a[4])?;
                if tx < 0
                    || oldest < 0
                    || oldest > tx
                    || argument < 0
                    || ![s.values[b"true".as_slice()], s.values[b"false".as_slice()]]
                        .contains(&a[1])
                {
                    return Err(invalid());
                }
                let retention = match policy {
                    0 if argument == 0 => RetentionPolicy::CurrentOnly,
                    1 if argument <= tx => RetentionPolicy::KeepSince(argument as u64),
                    2 => RetentionPolicy::KeepLast(argument as u64),
                    _ => return Err(invalid()),
                };
                s.edit(r, |st| {
                    st.tx = tx as u64;
                    st.raw = a[1] == as_oop(p.true_value);
                    st.oldest_retained_basis = oldest as u64;
                    st.retention = retention;
                    st.transitions_since_compaction = 0;
                })?;
            }
            _ => unreachable!(),
        }
        Ok(r)
    }
}
entry!(raw_assert, Raw, 3);
entry!(flip, Flip, 0);
entry!(assert_fact, Assert, 3);
entry!(raw_read, RawRead, 2);
entry!(read, Read, 2);
entry!(read_else, ReadElse, 3);
entry!(has, Has, 2);
entry!(count, Count, 2);
entry!(attr_count, AttrCount, 1);
entry!(attr_at, AttrAt, 2);
entry!(history, History, 2);
entry!(retract, Retract, 3);
entry!(basis, Basis, 0);
entry!(oldest, Oldest, 0);
entry!(facts, Facts, 0);
entry!(bytes, Bytes, 0);
entry!(reserved, Reserved, 0);
entry!(reserved_bytes, ReservedBytes, 0);
entry!(reserved_for, ReservedFor, 1);
entry!(reserve_compensation, ReserveCompensation, 2);
entry!(release_compensation, ReleaseCompensation, 1);
entry!(limit, Limit, 0);
entry!(record_bytes, RecordBytes, 0);
entry!(policy, Policy, 0);
entry!(policy_arg, PolicyArg, 0);
entry!(current_only, CurrentOnly, 0);
entry!(since, Since, 1);
entry!(last, Last, 1);
entry!(compact, Compact, 0);
entry!(as_of, AsOf, 3);
entry!(triples, Triples, 1);
entry!(triples_as_of, TriplesAsOf, 2);
entry!(triples_since, TriplesSince, 2);
entry!(triples_between, TriplesBetween, 3);
entry!(get_cardinality, Cardinality, 1);
entry!(image_facts, ImageFacts, 1);
entry!(image_begin, ImageBegin, 0);
entry!(image_swap, ImageSwap, 1);
entry!(image_add, ImageAdd, 6);
entry!(image_end, ImageEnd, 5);
entry!(new_store, New, 0);
entry!(cell, Cell, 2);
entry!(cell_value, CellValue, 0);
entry!(cell_set, CellSet, 1);
entry!(cell_history, CellHistory, 0);
entry!(cell_image_field, CellImageField, 1);
entry!(cell_image_restore, CellImageRestore, 3);
entry!(set_observer, SetObserver, 1);
entry!(set_authorizer, SetAuthorizer, 1);
entry!(tracked_add_method, AddMethod, 3);
entry!(tracked_remove_method, RemoveMethod, 2);
entry!(backfill_mirror, Backfill, 0);
entry!(dictionary_changed, DictionaryChanged, 2);
const STORE_METHODS: &[(&[u8], Method)] = &[
    (b"setMethodMutationObserver:", set_observer),
    (b"setMethodMutationAuthorizer:", set_authorizer),
    (b"addMethod:on:with:", tracked_add_method),
    (b"removeMethod:on:", tracked_remove_method),
    (b"backfillMirror", backfill_mirror),
    (b"methodChanged:on:", dictionary_changed),
    (b"rawAssert:attr:value:", raw_assert),
    (b"flip", flip),
    (b"assert:attr:value:", assert_fact),
    (b"rawCurrentValue:attr:", raw_read),
    (b"currentValue:attr:", read),
    (b"currentValue:attr:ifAbsent:", read_else),
    (b"hasValue:attr:", has),
    (b"count:attr:", count),
    (b"attrCount:", attr_count),
    (b"attrAt:index:", attr_at),
    (b"history:attr:", history),
    (b"retract:attr:value:", retract),
    (b"basis", basis),
    (b"oldestRetainedBasis", oldest),
    (b"retainedFactCount", facts),
    (b"retainedFactBytes", bytes),
    (b"reservedCompensationFactCount", reserved),
    (b"reservedCompensationFactBytes", reserved_bytes),
    (b"reserveCompensationFor:count:", reserve_compensation),
    (b"reservedCompensationFor:", reserved_for),
    (b"releaseCompensationFor:", release_compensation),
    (
        b"restoreValue:attr:expected:previous:reservation:",
        restore_value,
    ),
    (b"factBytesLimit", limit),
    (b"factRecordBytes", record_bytes),
    (b"retentionPolicyCode", policy),
    (b"retentionPolicyArgument", policy_arg),
    (b"retainCurrentOnly", current_only),
    (b"retainSince:", since),
    (b"retainLast:", last),
    (b"compact", compact),
    (b"valueOf:attr:asOf:", as_of),
    (b"liveTriplesDo:", triples),
    (b"liveTriplesAsOf:do:", triples_as_of),
    (b"liveTriplesSince:do:", triples_since),
    (b"liveTriplesBetween:and:do:", triples_between),
    (b"cardinality:", get_cardinality),
    (b"imageFactsInto:", image_facts),
    (b"imageLoadBegin", image_begin),
    (b"swapImageStateWith:", image_swap),
    (b"imageStateRevision", image_revision),
    (b"imageAddFact:attr:value:tx:op:historied:", image_add),
    (b"imageLoadEnd:raw:oldest:policy:argument:", image_end),
    (b"new", new_store),
    (b"cellFor:attr:", cell),
];
const CELL_METHODS: &[(&[u8], Method)] = &[
    (b"imageFieldAt:", cell_image_field),
    (b"restoreImageStore:entity:attr:", cell_image_restore),
    (b"value", cell_value),
    (b"set:", cell_set),
    (b"history", cell_history),
];
const NAMES: &[&[u8]] = &[
    b"imageFieldAt:",
    b"restoreImageStore:entity:attr:",
    b"visit:",
    b"CellVt",
    b"Error",
    b"GC",
    b"Integer",
    b"ObjectVt",
    b"Roots",
    b"Store",
    b"StoreVt",
    b"addMethod",
    b"addMethod:as:",
    b"addMethod:on:with:",
    b"allocate",
    b"allocationSizeOf:",
    b"assert:attr:value:",
    b"at:",
    b"at:put:",
    b"attrAt:index:",
    b"attrCount:",
    b"backfillMirror",
    b"basis",
    b"behaviorOf:",
    b"cardinality",
    b"cardinality:",
    b"cellFor:attr:",
    b"compact",
    b"count:attr:",
    b"currentValue:attr:",
    b"currentValue:attr:ifAbsent:",
    b"db",
    b"delegated",
    b"e:a:v:tx:op:historied:",
    b"factBytesLimit",
    b"factRecordBytes",
    b"false",
    b"flip",
    b"fromWord:",
    b"hasValue:attr:",
    b"history",
    b"history:attr:",
    b"historyCompactionResourceLimit",
    b"historyNotRetained",
    b"historyResourceLimit",
    b"invalidCompensation",
    b"compensationConflict",
    b"imageAddFact:attr:value:tx:op:historied:",
    b"imageFactsInto:",
    b"imageLoadBegin",
    b"swapImageStateWith:",
    b"imageStateRevision",
    b"imageLoadEnd:raw:oldest:policy:argument:",
    b"imageTransient",
    b"kind:",
    b"liveTriplesAsOf:do:",
    b"liveTriplesBetween:and:do:",
    b"liveTriplesDo:",
    b"liveTriplesSince:do:",
    b"managedMethodChanged:on:",
    b"many",
    b"methodChanged:on:",
    b"methodMutationAuthorizerAlreadySet",
    b"methodMutationObserverAlreadySet",
    b"missingDirectMethod",
    b"name",
    b"new",
    b"nil",
    b"oldestRetainedBasis",
    b"one",
    b"nameOf:",
    b"ownerOf:",
    b"ownerOf:put:",
    b"parentOf:",
    b"prepareAllocation",
    b"primitive",
    b"rawAssert:attr:value:",
    b"rawCurrentValue:attr:",
    b"referencesOf:do:",
    b"register:",
    b"registerRetirement:",
    b"release:",
    b"removeMethod:",
    b"removeMethod:on:",
    b"reservedCompensationFactBytes",
    b"reserveCompensationFor:count:",
    b"reservedCompensationFor:",
    b"releaseCompensationFor:",
    b"restoreValue:attr:expected:previous:reservation:",
    b"reservedCompensationFactCount",
    b"retain:",
    b"retainCurrentOnly",
    b"retainLast:",
    b"retainSince:",
    b"retainedFactBytes",
    b"retainedFactCount",
    b"retentionPolicyArgument",
    b"retentionPolicyCode",
    b"retire:",
    b"retract:attr:value:",
    b"rootsDo:",
    b"set:",
    b"setMethodMutationAuthorizer:",
    b"setMethodMutationObserver:",
    b"trace:with:",
    b"true",
    b"unstableMethodTarget",
    b"value",
    b"value:value:value:",
    b"valueOf:attr:asOf:",
    b"wordOf:",
    b"methodAt:",
    b"withContext:",
    b"low32",
    b"high32",
    b"isBehavior:",
    b"allocationsDo:",
    b"bindingsDo:",
    b"allocation:bytes:identity:",
    b"binding:value:weak:",
];

const OWNER_METHODS: &[(&[u8], Method)] = &[
    (b"rootsDo:", roots),
    (b"referencesOf:do:", references),
    (b"retire:", retire),
    (b"allocation:bytes:identity:", allocation),
    (b"binding:value:weak:", binding),
];
const fn index(name: &[u8]) -> usize {
    let mut i = 0;
    while i < NAMES.len() {
        let n = NAMES[i];
        if n.len() == name.len() {
            let mut j = 0;
            while j < n.len() && n[j] == name[j] {
                j += 1;
            }
            if j == n.len() {
                return i;
            }
        }
        i += 1;
    }
    panic!("undeclared Store name")
}
const LITERALS: [Literal; NAMES.len()] = {
    let mut a = [const { Literal::new(b"") }; NAMES.len()];
    let mut i = 0;
    while i < NAMES.len() {
        a[i] = Literal::new(NAMES[i]);
        i += 1;
    }
    a
};
const METHOD_COUNT: usize = STORE_METHODS.len() + CELL_METHODS.len() + OWNER_METHODS.len();
const METHODS: [MethodSpec; METHOD_COUNT] = {
    let mut a = [const {
        MethodSpec {
            name: 0,
            entry: None,
        }
    }; METHOD_COUNT];
    let tables = [STORE_METHODS, CELL_METHODS, OWNER_METHODS];
    let mut i = 0;
    let mut t = 0;
    while t < tables.len() {
        let mut j = 0;
        while j < tables[t].len() {
            let (n, f) = tables[t][j];
            a[i] = MethodSpec {
                name: index(n),
                entry: Some(f),
            };
            i += 1;
            j += 1;
        }
        t += 1;
    }
    a
};
unsafe extern "C" fn initialize(
    api: *const Protocol,
    image: Oop,
    l: *const Oop,
    count: usize,
    a: *const Oop,
    n: usize,
) -> Oop {
    if api.is_null() {
        std::process::abort();
    }
    API.store(api.cast_mut(), Ordering::Release);
    done((|| unsafe {
        let id = *api;
        let [ns] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        object(*ns)?;
        let old = INSTANCE.load(Ordering::Acquire);
        if !old.is_null() {
            let s = state(*ns)?;
            return if s.ready.get() && s.id.root == id.root {
                Ok(s.receiver.get())
            } else {
                Err(invalid())
            };
        }
        let literals = borrowed_slice(l, count)
            .filter(|v| v.len() == NAMES.len())
            .ok_or_else(invalid)?;
        let names: HashMap<_, _> = NAMES
            .iter()
            .copied()
            .zip(literals.iter().copied())
            .collect();
        let send = |r, name: &[u8], args: &[Oop]| id.send(r, names[name], args);
        let named = |name: &[u8]| object(send(*ns, b"at:", &[names[name]])?);
        let roots = named(b"Roots")?;
        let mut tokens = Vec::new();
        let mut keep = |o: Oop| -> Result<Oop, Oop> {
            tokens.push(send(roots, b"retain:", &[o])?);
            Ok(o)
        };
        let result = (|| {
            for &name in names.values() {
                keep(name)?;
            }
            let mut values = HashMap::new();
            for name in [
                b"Integer".as_slice(),
                b"true",
                b"false",
                b"nil",
                b"ObjectVt",
            ] {
                values.insert(name, named(name)?);
            }
            let parent = named(b"ObjectVt")?;
            let store_vt = keep(send(parent, b"delegated", &[])?)?;
            let cell_vt = keep(send(parent, b"delegated", &[])?)?;
            let policy = Runtime {
                cardinality: names[b"cardinality".as_slice()].addr(),
                one: names[b"one".as_slice()].addr(),
                many: names[b"many".as_slice()].addr(),
                history: names[b"history".as_slice()].addr(),
                true_value: values[b"true".as_slice()].addr(),
                compaction_transitions: positive_config(
                    "BIRNAM_STORE_COMPACTION_TRANSITIONS",
                    DEFAULT_COMPACTION_TRANSITIONS as usize,
                ) as u64,
                fact_bytes_limit: positive_config(
                    "BIRNAM_STORE_FACT_BYTES_LIMIT",
                    DEFAULT_FACT_BYTES_LIMIT,
                ),
            };
            if policy.fact_bytes_limit < size_of::<Fact>() * 4 {
                return Err(invalid());
            }
            let state = Box::into_raw(Box::new(State {
                id,
                namespace: *ns,
                roots,
                gc: named(b"GC")?,
                store_vt,
                cell_vt,
                receiver: Cell::new(ptr::null_mut()),
                method_observer: Cell::new(ptr::null_mut()),
                method_authorizer: Cell::new(ptr::null_mut()),
                policy,
                names: names.clone(),
                values,
                stores: RefCell::new(HashMap::new()),
                cells: RefCell::new(HashMap::new()),
                active: RefCell::new(Vec::new()),
                ready: Cell::new(false),
                thread: std::thread::current().id(),
            }));
            INSTANCE.store(state, Ordering::Release);
            let s = &*state;
            for (vt, methods) in [
                (store_vt, STORE_METHODS),
                (store_vt, OWNER_METHODS),
                (cell_vt, CELL_METHODS),
            ] {
                for &(name, _) in methods {
                    let method = keep(send(image, b"methodAt:", &[names[name]])?)?;
                    let method = keep(send(method, b"withContext:", &[*ns])?)?;
                    send(vt, b"addMethod", &[names[name], method])?;
                }
            }
            let receiver = keep(send(store_vt, b"allocate", &[word(0)?])?)?;
            s.receiver.set(receiver);
            s.validate_allocation(receiver, store_vt)?;
            s.stores.borrow_mut().insert(receiver, s.seeded());
            send(roots, b"register:", &[receiver])?;
            for vt in [store_vt, cell_vt] {
                send(s.gc, b"trace:with:", &[vt, receiver])?;
                send(id.root, b"ownerOf:put:", &[vt, receiver])?;
            }
            send(s.gc, b"registerRetirement:", &[receiver])?;
            for (name, value) in [
                (b"Store".as_slice(), receiver),
                (b"db", receiver),
                (b"StoreVt", store_vt),
                (b"CellVt", cell_vt),
            ] {
                send(*ns, b"at:put:", &[names[name], value])?;
            }
            s.ready.set(true);
            Ok(receiver)
        })();
        for token in tokens {
            send(roots, b"release:", &[token])?;
        }
        result
    })())
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
