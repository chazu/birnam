//! Native integration. Dependency operations and GC enumeration use Id sends.
//! Only Store reads fact/Cell state; no Store borrow spans a callback.
use super::*;
use birnam_abi::{
    borrowed_bytes, borrowed_slice,
    id_protocol::{self as native, Method, NativeEntry, Operand, Outcome, Protocol},
};
use core::{
    cell::{Cell, RefCell},
    ffi::c_void,
};
use std::collections::HashMap;

struct State {
    id: Protocol,
    namespace: Oop,
    roots: Oop,
    gc: Oop,
    store_vt: Oop,
    cell_vt: Oop,
    globals_vt: Oop,
    receiver: Cell<Oop>,
    globals: Cell<Oop>,
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
    thread: std::thread::ThreadId,
}
fn invalid() -> Outcome {
    Outcome::failed(native::INVALID_REQUEST, 0)
}
fn object(v: Operand) -> Result<Oop, Outcome> {
    v.as_object().filter(|o| !o.is_null()).ok_or_else(invalid)
}
fn done(v: Result<Operand, Outcome>) -> Outcome {
    v.map_or_else(|e| e, Outcome::loaded)
}
unsafe fn state<'a>(c: *mut c_void) -> Result<&'a State, Outcome> {
    let s = unsafe { &*c.cast::<State>() };
    if s.thread != std::thread::current().id() {
        return Err(invalid());
    }
    Ok(s)
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
    unsafe fn send(&self, r: Oop, n: &'static [u8], a: &[Operand]) -> Result<Operand, Outcome> {
        unsafe { self.id.send(r, self.names[n], a) }
    }
    fn nil(&self) -> Oop {
        self.values[b"nil".as_slice()]
    }
    fn boolean(&self, yes: bool) -> Operand {
        Operand::object(
            self.values[if yes {
                b"true".as_slice()
            } else {
                b"false".as_slice()
            }],
        )
    }
    unsafe fn integer(&self, n: usize) -> Result<Operand, Outcome> {
        unsafe {
            self.send(
                self.values[b"Integer".as_slice()],
                b"fromWord:",
                &[Operand::word(n)],
            )
        }
    }
    unsafe fn number(&self, value: Oop) -> Result<i64, Outcome> {
        unsafe {
            self.send(
                self.values[b"Integer".as_slice()],
                b"wordOf:",
                &[Operand::object(value)],
            )
        }?
        .as_word()
        .map(|n| n as i64)
        .ok_or_else(invalid)
    }
    unsafe fn error(&self, kind: &'static [u8]) -> Result<Operand, Outcome> {
        unsafe {
            self.send(
                self.values[b"Error".as_slice()],
                b"kind:",
                &[Operand::object(self.names[kind])],
            )
        }
    }
    unsafe fn allocate(&self, scope: &Scope<'_>, vt: Oop) -> Result<Oop, Outcome> {
        let o = scope.keep(object(unsafe {
            self.send(vt, b"allocate", &[Operand::word(0)])
        }?)?);
        unsafe { self.validate_allocation(o, vt) }?;
        Ok(o)
    }
    unsafe fn validate_allocation(&self, o: Oop, vt: Oop) -> Result<(), Outcome> {
        if unsafe { self.send(self.id.receiver, b"behaviorOf:", &[Operand::object(o)]) }?
            .as_object()
            != Some(vt)
            || unsafe {
                self.send(
                    self.id.receiver,
                    b"allocationSizeOf:",
                    &[Operand::object(o)],
                )
            }?
            .as_word()
                != Some(0)
            || self.stores.borrow().contains_key(&o)
            || self.cells.borrow().contains_key(&o)
        {
            return Err(invalid());
        }
        Ok(())
    }
    unsafe fn fresh(&self, scope: &Scope<'_>) -> Result<Oop, Outcome> {
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
    fn read<T>(&self, r: Oop, f: impl FnOnce(&StoreState) -> T) -> Result<T, Outcome> {
        self.stores.borrow().get(&r).map(f).ok_or_else(invalid)
    }
    fn edit<T>(&self, r: Oop, f: impl FnOnce(&mut StoreState) -> T) -> Result<T, Outcome> {
        self.stores
            .borrow_mut()
            .get_mut(&r)
            .map(f)
            .ok_or_else(invalid)
    }
}

unsafe fn visit(s: &State, visitor: Oop, o: Oop) -> Result<(), Outcome> {
    unsafe { s.send(visitor, b"visit:", &[Operand::object(o)]) }?;
    Ok(())
}
unsafe extern "C" fn roots(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
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
            s.globals_vt,
            s.receiver.get(),
            s.globals.get(),
            s.method_observer.get(),
            s.method_authorizer.get(),
        ];
        values.extend(s.names.values().copied());
        values.extend(s.values.values().copied());
        values.extend(s.active.borrow().iter().flatten().copied());
        for o in values {
            visit(s, v, o)?;
        }
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn references(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
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
                .collect::<Vec<_>>()
        } else if let Some(cell) = s.cells.borrow().get(&o) {
            cell.to_vec()
        } else {
            Vec::new()
        };
        for edge in values {
            visit(s, v, edge)?;
        }
        Ok(Operand::unit())
    })())
}
unsafe extern "C" fn retire(c: *mut c_void, _: Oop, a: *const Operand, n: usize) -> Outcome {
    done((|| unsafe {
        let s = state(c)?;
        let Some([bytes]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let bytes = bytes.as_bytes().ok_or_else(invalid)?;
        let bytes = borrowed_bytes(bytes.pointer, bytes.length).ok_or_else(invalid)?;
        if bytes.len() % size_of::<usize>() != 0 {
            return Err(invalid());
        }
        for bytes in bytes.chunks_exact(size_of::<usize>()) {
            let o = usize::from_ne_bytes(bytes.try_into().unwrap()) as Oop;
            s.stores.borrow_mut().remove(&o);
            s.cells.borrow_mut().remove(&o);
        }
        Ok(Operand::unit())
    })())
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
    GlobalRead,
    GlobalElse,
    GlobalPut,
    GlobalHas,
    SetObserver,
    SetAuthorizer,
    AddMethod,
    RemoveMethod,
    Backfill,
    DictionaryChanged,
}
macro_rules! entry {
    ($name:ident,$op:ident,$arity:expr) => {
        unsafe extern "C" fn $name(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
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

unsafe fn execute(
    s: &State,
    scope: &Scope<'_>,
    r: Oop,
    a: &[Oop],
    op: OpCode,
) -> Result<Operand, Outcome> {
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
                | GlobalPut
                | SetObserver
                | SetAuthorizer
                | AddMethod
                | RemoveMethod
                | Backfill
        ) {
            s.send(s.gc, b"prepareAllocation", &[])?;
        }
        match op {
            New => return Ok(Operand::object(s.fresh(scope)?)),
            Cell => {
                s.read(r, |_| ())?;
                let cell = s.allocate(scope, s.cell_vt)?;
                s.cells.borrow_mut().insert(cell, [r, a[0], a[1]]);
                return Ok(Operand::object(cell));
            }
            CellImageField => {
                let index = s.number(a[0])? as usize;
                let fields = *s.cells.borrow().get(&r).ok_or_else(invalid)?;
                return Ok(Operand::object(*fields.get(index).ok_or_else(invalid)?));
            }
            CellImageRestore => {
                s.read(a[0], |_| ())?;
                *s.cells.borrow_mut().get_mut(&r).ok_or_else(invalid)? = [a[0], a[1], a[2]];
                return Ok(Operand::object(r));
            }
            CellValue | CellSet | CellHistory => {
                let [store, e, attr] = *s.cells.borrow().get(&r).ok_or_else(invalid)?;
                scope.keep_all([store, e, attr]);
                let mut args = vec![Operand::object(e), Operand::object(attr)];
                let selector = match op {
                    CellValue => b"currentValue:attr:".as_slice(),
                    CellHistory => b"history:attr:",
                    _ => {
                        args.push(Operand::object(a[0]));
                        b"assert:attr:value:"
                    }
                };
                let result = s.send(store, selector, &args)?;
                return Ok(
                    if matches!(op, CellSet) && result.as_object() == Some(store) {
                        Operand::object(r)
                    } else {
                        result
                    },
                );
            }
            GlobalRead | GlobalElse | GlobalPut | GlobalHas => {
                if r != s.globals.get() {
                    return Err(invalid());
                }
                let store = s.receiver.get();
                let mut args = vec![
                    Operand::object(s.names[b"__globals__".as_slice()]),
                    Operand::object(a[0]),
                ];
                let name = match op {
                    GlobalRead => b"currentValue:attr:".as_slice(),
                    GlobalHas => b"hasValue:attr:",
                    GlobalElse => b"rawCurrentValue:attr:",
                    _ => {
                        args.push(Operand::object(a[1]));
                        b"assert:attr:value:"
                    }
                };
                let value = s.send(store, name, &args)?;
                return if matches!(op, GlobalPut) && value.as_object() == Some(store) {
                    Ok(Operand::object(a[1]))
                } else if matches!(op, GlobalElse) && value.kind() == native::UNIT {
                    s.send(a[1], b"value", &[])
                } else {
                    Ok(value)
                };
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
                // This is Id's live notification, including direct native writes.
                // No sends or Store borrows escape this invalidation operation.
                for st in s.stores.borrow_mut().values_mut() {
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
                    if decision.as_object() != Some(as_oop(p.true_value)) {
                        return Ok(decision);
                    }
                }
                let selector = a[0];
                let behavior = a[1];
                if matches!(op, RemoveMethod) {
                    if s.send(s.id.receiver, b"ownerOf:", &[Operand::object(behavior)])?
                        .kind()
                        == native::UNIT
                    {
                        return s.error(b"unstableMethodTarget");
                    }
                    if s.send(
                        s.id.receiver,
                        b"bindingOf:in:",
                        &[Operand::object(selector), Operand::object(behavior)],
                    )?
                    .kind()
                        == native::UNIT
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
                    let result = s.send(
                        behavior,
                        b"addMethod:as:",
                        &[Operand::object(a[2]), Operand::object(selector)],
                    )?;
                    if result.as_object() != Some(a[2]) {
                        return Ok(result);
                    }
                    s.edit(r, |st| append_mirror_fact(st, behavior, selector, a[2]))?;
                } else {
                    s.send(behavior, b"removeMethod:", &[Operand::object(selector)])?;
                }
                let observer = s.method_observer.get();
                if !observer.is_null() {
                    scope.keep(observer);
                    s.send(
                        observer,
                        b"managedMethodChanged:on:",
                        &[Operand::object(selector), Operand::object(behavior)],
                    )?;
                }
            }
            Backfill => {
                let count = s
                    .send(s.id.receiver, b"behaviorCount", &[])?
                    .as_word()
                    .ok_or_else(invalid)?;
                let mut pairs = Vec::new();
                for i in 0..count {
                    let v = s.send(s.id.receiver, b"behaviorAt:", &[Operand::word(i)])?;
                    if v.kind() == native::UNIT {
                        continue;
                    }
                    let behavior = scope.keep(object(v)?);
                    let mut ancestor = behavior;
                    let mut ordinary = false;
                    loop {
                        if ancestor == s.values[b"ObjectVt".as_slice()] {
                            ordinary = true;
                            break;
                        }
                        let parent =
                            s.send(s.id.receiver, b"parentOf:", &[Operand::object(ancestor)])?;
                        if parent.kind() == native::UNIT {
                            break;
                        }
                        ancestor = scope.keep(object(parent)?);
                    }
                    if !ordinary {
                        continue;
                    }
                    let mut index = 0;
                    loop {
                        let selector = s.send(
                            s.id.receiver,
                            b"selectorAt:in:",
                            &[Operand::word(index), Operand::object(behavior)],
                        )?;
                        if selector.kind() == native::UNIT {
                            break;
                        }
                        pairs.push((behavior, scope.keep(object(selector)?)));
                        index += 1;
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
                    return Ok(Operand::object(v));
                }
                return match op {
                    RawRead => Ok(Operand::unit()),
                    ReadElse => s.send(a[2], b"value", &[]),
                    _ => Ok(Operand::object(s.nil())),
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
                    Ok(attrs
                        .get(i)
                        .map_or(Operand::unit(), |o| Operand::object(*o)))
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
                    Some(v) => Ok(Operand::object(v)),
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
                    s.send(
                        block,
                        b"value:value:value:",
                        &[
                            Operand::object(t.e),
                            Operand::object(t.a),
                            Operand::object(t.v),
                        ],
                    )?;
                }
            }
            Cardinality => {
                return Ok(Operand::object(as_oop(
                    s.read(r, |st| cardinality(p, st, a[0]))?,
                )));
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
                        &[
                            Operand::object(f.e),
                            Operand::object(f.a),
                            Operand::object(f.v),
                            Operand::object(tx),
                            Operand::object(op),
                            s.boolean(f.historied),
                        ],
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
                std::mem::swap(stores.get_mut(&r).ok_or_else(invalid)?, &mut staged);
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
        Ok(Operand::object(r))
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
entry!(global_read, GlobalRead, 1);
entry!(global_else, GlobalElse, 2);
entry!(global_put, GlobalPut, 2);
entry!(global_has, GlobalHas, 1);
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
const GLOBAL_METHODS: &[(&[u8], Method)] = &[
    (b"at:", global_read),
    (b"at:ifAbsent:", global_else),
    (b"at:put:", global_put),
    (b"includes:", global_has),
];
const NAMES: &[&[u8]] = &[
    b"imageFieldAt:",
    b"restoreImageStore:entity:attr:",
    b"visit:",
    b"CellVt",
    b"Error",
    b"GC",
    b"Globals",
    b"GlobalsVt",
    b"Integer",
    b"ObjectVt",
    b"Roots",
    b"Store",
    b"StoreVt",
    b"__globals__",
    b"addMethod",
    b"addMethod:as:",
    b"addMethod:on:with:",
    b"allocate",
    b"allocationSizeOf:",
    b"assert:attr:value:",
    b"at:",
    b"at:ifAbsent:",
    b"at:put:",
    b"attrAt:index:",
    b"attrCount:",
    b"backfillMirror",
    b"basis",
    b"behaviorAt:",
    b"behaviorCount",
    b"behaviorOf:",
    b"bindingOf:in:",
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
    b"imageAddFact:attr:value:tx:op:historied:",
    b"imageFactsInto:",
    b"imageLoadBegin",
    b"swapImageStateWith:",
    b"imageLoadEnd:raw:oldest:policy:argument:",
    b"imageTransient",
    b"includes:",
    b"kind:",
    b"liveTriplesAsOf:do:",
    b"liveTriplesBetween:and:do:",
    b"liveTriplesDo:",
    b"liveTriplesSince:do:",
    b"managedMethodChanged:on:",
    b"many",
    b"methodChanged:on:",
    b"methodChangesTo:",
    b"methodMutationAuthorizerAlreadySet",
    b"methodMutationObserverAlreadySet",
    b"missingDirectMethod",
    b"name",
    b"new",
    b"nil",
    b"oldestRetainedBasis",
    b"one",
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
    b"selectorAt:in:",
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
];

unsafe fn construct(id: Protocol, ns: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let named =
            |n: &[u8]| object(id.send_named(ns, b"at:", &[Operand::object(id.intern_name(n)?)])?);
        let existing = id.send_named(ns, b"at:", &[Operand::object(id.intern_name(b"Store")?)])?;
        if existing.kind() != native::UNIT {
            return Ok(existing);
        }
        let roots = named(b"Roots")?;
        let mut tokens = Vec::new();
        let mut keep = |o: Oop| -> Result<Oop, Outcome> {
            tokens.push(
                id.send_named(roots, b"retain:", &[Operand::object(o)])?
                    .as_word()
                    .filter(|n| *n != 0)
                    .ok_or_else(invalid)?,
            );
            Ok(o)
        };
        let result = (|| {
            let mut names = HashMap::new();
            for &n in NAMES {
                names.insert(n, keep(id.intern_name(n)?)?);
            }
            let mut values = HashMap::new();
            for n in [
                b"Integer".as_slice(),
                b"Error",
                b"true",
                b"false",
                b"nil",
                b"ObjectVt",
            ] {
                values.insert(n, named(n)?);
            }
            let parent = named(b"ObjectVt")?;
            let store_vt = keep(object(id.send_named(parent, b"delegated", &[])?)?)?;
            let cell_vt = keep(object(id.send_named(parent, b"delegated", &[])?)?)?;
            let globals_vt = keep(object(id.send_named(parent, b"delegated", &[])?)?)?;
            let policy = Runtime {
                cardinality: names[b"cardinality".as_slice()] as usize,
                one: names[b"one".as_slice()] as usize,
                many: names[b"many".as_slice()] as usize,
                history: names[b"history".as_slice()] as usize,
                true_value: values[b"true".as_slice()] as usize,
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
                namespace: ns,
                roots,
                gc: named(b"GC")?,
                store_vt,
                cell_vt,
                globals_vt,
                receiver: Cell::new(ptr::null_mut()),
                globals: Cell::new(ptr::null_mut()),
                method_observer: Cell::new(ptr::null_mut()),
                method_authorizer: Cell::new(ptr::null_mut()),
                policy,
                names,
                values,
                stores: RefCell::new(HashMap::new()),
                cells: RefCell::new(HashMap::new()),
                active: RefCell::new(Vec::new()),
                thread: std::thread::current().id(),
            }));
            // Native entries are bound to this owner-thread instance. Like the
            // other Bootstrap module instances, it lives with the Id domain.
            let s = &*state;
            let owner_methods = [
                (b"rootsDo:".as_slice(), self::roots as Method),
                (b"referencesOf:do:", references),
                (b"retire:", retire),
            ];
            for (vt, methods) in [
                (store_vt, STORE_METHODS),
                (store_vt, owner_methods.as_slice()),
                (cell_vt, CELL_METHODS),
                (globals_vt, GLOBAL_METHODS),
            ] {
                for &(name, method) in methods {
                    s.send(
                        vt,
                        b"addMethod",
                        &[
                            Operand::object(s.names[name]),
                            Operand::entry(NativeEntry::new(method, state.cast())),
                        ],
                    )?;
                }
            }
            // The first receiver must use construction roots: this instance's
            // active-scope enumerator cannot protect anything until registered.
            let receiver = keep(object(s.send(
                store_vt,
                b"allocate",
                &[Operand::word(0)],
            )?)?)?;
            s.receiver.set(receiver);
            s.validate_allocation(receiver, store_vt)?;
            s.stores.borrow_mut().insert(receiver, s.seeded());
            s.send(roots, b"register:", &[Operand::object(receiver)])?;
            for vt in [store_vt, cell_vt] {
                s.send(
                    s.gc,
                    b"trace:with:",
                    &[Operand::object(vt), Operand::object(receiver)],
                )?;
            }
            s.send(s.gc, b"registerRetirement:", &[Operand::object(receiver)])?;
            let scope = Scope::new(s, vec![]);
            let globals = s.allocate(&scope, globals_vt)?;
            s.globals.set(globals);
            for (vt, owner) in [
                (store_vt, receiver),
                (cell_vt, receiver),
                (globals_vt, globals),
            ] {
                s.send(
                    id.receiver,
                    b"ownerOf:put:",
                    &[Operand::object(vt), Operand::object(owner)],
                )?;
            }
            for (name, value) in [
                (b"Store".as_slice(), receiver),
                (b"db", receiver),
                (b"StoreVt", store_vt),
                (b"CellVt", cell_vt),
                (b"Globals", globals),
                (b"GlobalsVt", globals_vt),
            ] {
                s.send(
                    ns,
                    b"at:put:",
                    &[Operand::object(s.names[name]), Operand::object(value)],
                )?;
            }
            s.send(
                id.receiver,
                b"methodChangesTo:",
                &[Operand::object(receiver)],
            )?;
            Ok(Operand::object(receiver))
        })();
        for token in tokens {
            id.send_named(roots, b"release:", &[Operand::word(token)])?;
        }
        result
    }
}
unsafe extern "C" fn initialize(id: *const Protocol, a: *const Operand, n: usize) -> Outcome {
    if !unsafe { native::valid_protocol(id) } {
        return invalid();
    }
    done((|| unsafe {
        let Some([ns]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        construct(*id, object(*ns)?)
    })())
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: 1,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};
