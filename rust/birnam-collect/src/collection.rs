//! Port of the production census/LIFO walk/ordered retirement algorithm to Oops.
//! All traversal storage remains in the explicit Memory/Walk owner.
use super::*;
#[derive(Default)]
pub(super) struct Walk {
    allocations: Vec<Allocation>,
    indices: HashMap<usize, usize>,
    marked: Vec<u8>,
    pending: Vec<Oop>,
    failure: Option<Oop>,
    core: CoreEdges,
}
#[derive(Default)]
struct CoreEdges {
    values: Vec<Oop>,
    accepting: bool,
    failure: Option<Oop>,
}
pub(super) struct Inventory {
    values: Vec<Allocation>,
    failure: Option<Oop>,
}
impl Walk {
    pub(super) fn marked(&self, value: Oop) -> bool {
        self.indices.get(&value.addr()).is_some_and(|&i| self.marked[i] != 0)
    }
    fn emit(&mut self, v: Oop) -> Result<(), ()> {
        let v=abi::object_reference(v);
        if v.is_null() || abi::as_size(v).is_some() {
            return Ok(());
        }
        let i = *self.indices.get(&v.addr()).ok_or(())?;
        if self.marked[i] == 0 {
            self.marked[i] = 1;
            self.pending.push(v)
        }
        Ok(())
    }
}
fn inventory_failure(s: &Memory) -> Oop {
    if let Some(i) = s.inventory.borrow_mut().as_mut() {
        i.failure = Some(s.invalid())
    }
    s.invalid()
}
fn walk_failure(s: &Memory) -> Oop {
    if let Some(w) = s.walk.borrow_mut().as_mut() {
        w.failure = Some(s.invalid())
    }
    s.invalid()
}
fn core_failure(s: &Memory) -> Oop {
    if let Some(w) = s.walk.borrow_mut().as_mut() {
        w.core.failure = Some(s.invalid())
    }
    s.invalid()
}
pub(super) unsafe extern "C" fn allocation(
    _id: Oop,
    c: Oop,
    r: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    let s = match unsafe { memory(c) } {
        Ok(s) => s,
        Err(e) => return abi::fault_reference(e),
    };
    if r != s.receiver {
        return inventory_failure(s);
    }
    let Some([o, size, id]) = (unsafe { abi::borrowed_slice(a, n) }) else {
        return inventory_failure(s);
    };
    let (Some(size), Some(identity)) = (abi::as_size(*size), abi::as_size(*id)) else {
        return inventory_failure(s);
    };
    if identity == 0 {
        return inventory_failure(s);
    }
    let mut inventory = s.inventory.borrow_mut();
    let Some(v) = inventory.as_mut() else {
        return s.invalid();
    };
    v.values.push(Allocation {
        object: *o,
        size,
        identity,
    });
    core::ptr::null_mut()
}
pub(super) unsafe extern "C" fn visit(_id: Oop, c: Oop, r: Oop, a: *const Oop, n: usize) -> Oop {
    let s = match unsafe { memory(c) } {
        Ok(s) => s,
        Err(e) => return abi::fault_reference(e),
    };
    if r != s.receiver || s.phase.get() != Phase::Marking {
        return walk_failure(s);
    }
    let Some([v]) = (unsafe { abi::borrowed_slice(a, n) }) else {
        return walk_failure(s);
    };
    let mut walk = s.walk.borrow_mut();
    let Some(w) = walk.as_mut() else {
        return s.invalid();
    };
    if w.emit(*v).is_err() {
        w.failure = Some(s.invalid());
        s.invalid()
    } else {
        core::ptr::null_mut()
    }
}
pub(super) unsafe extern "C" fn core_edge(
    _id: Oop,
    c: Oop,
    r: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    let s = match unsafe { memory(c) } {
        Ok(s) => s,
        Err(e) => return abi::fault_reference(e),
    };
    if r != s.core_visitor || s.phase.get() != Phase::Marking {
        return core_failure(s);
    }
    let Some([v]) = (unsafe { abi::borrowed_slice(a, n) }) else {
        return core_failure(s);
    };
    let mut walk = s.walk.borrow_mut();
    let Some(w) = walk.as_mut() else {
        return s.invalid();
    };
    if !w.core.accepting {
        w.core.failure = Some(s.invalid());
        return s.invalid();
    }
    w.core.values.push(*v);
    core::ptr::null_mut()
}
unsafe fn inventory(s: &Memory) -> Result<Vec<Allocation>, Oop> {
    if s.inventory.borrow().is_some() {
        return Err(s.invalid());
    }
    *s.inventory.borrow_mut() = Some(Inventory {
        values: std::mem::take(&mut s.spare.borrow_mut().allocations),
        failure: None,
    });
    let answer = unsafe { s.send(s.id.root, b"allocationsDo:", &[s.receiver]) };
    let inventory = s.inventory.borrow_mut().take().unwrap();
    let mut rows = inventory.values;
    let checked = (|| {
        answer?;
        if let Some(error) = inventory.failure { return Err(error); }
        rows.sort_unstable_by_key(|a| a.identity);
        if rows.windows(2).any(|w| w[0].identity == w[1].identity) { return Err(s.invalid()); }
        Ok(())
    })();
    if let Err(error) = checked {
        rows.clear();
        s.spare.borrow_mut().allocations = rows;
        return Err(error);
    }
    Ok(rows)
}
// Only capacity survives. No allocation identities, roots, marks or failures do.
pub(super) fn clear_walk(s: &Memory) {
    if let Some(mut walk) = s.walk.borrow_mut().take() {
        walk.allocations.clear(); walk.indices.clear(); walk.marked.clear();
        walk.pending.clear(); walk.failure = None;
        walk.core.values.clear(); walk.core.failure = None; walk.core.accepting = false;
        *s.spare.borrow_mut() = walk;
    }
}
unsafe fn begin_walk(s: &Memory) -> Result<usize, Oop> {
    let allocations = unsafe { inventory(s)? };
    let mut walk = std::mem::take(&mut *s.spare.borrow_mut());
    walk.allocations = allocations;
    let count = walk.allocations.len();
    walk.indices.extend(walk.allocations.iter().enumerate().map(|(i,a)|(a.object.addr(),i)));
    walk.marked.resize(count, 0);
    let valid = walk.indices.len() == count;
    *s.walk.borrow_mut() = Some(walk);
    if valid { Ok(count) } else { Err(s.invalid()) }
}

pub(super) fn emit(s: &Memory, v: Oop) -> Result<(), Oop> {
    let mut state = s.walk.borrow_mut();
    let walk = state.as_mut().ok_or(s.invalid())?;
    if walk.emit(v).is_err() {
        walk.failure = Some(s.invalid());
        Err(s.invalid())
    } else { Ok(()) }
}
unsafe fn mark(s: &Memory, native: &[usize]) -> Result<Vec<Allocation>, Oop> {
    unsafe {
        let epoch = s.send(s.id.root, b"allocationEpoch", &[])?;
        begin_walk(s)?;
        s.send(s.id.root, b"rootsDo:", &[s.receiver])?;
        s.send(s.roots, b"rootsDo:", &[s.receiver])?;
        for address in native {
            let value={let walk=s.walk.borrow();let w=walk.as_ref().unwrap();
                w.indices.get(address).map(|&i|w.allocations[i].object)};
            if let Some(value)=value { emit(s,value)?; }
        }
        loop {
            let next = s.walk.borrow_mut().as_mut().unwrap().pending.pop();
            let Some(value) = next else { break };
            {
                let mut state = s.walk.borrow_mut();
                let core = &mut state.as_mut().unwrap().core;
                core.values.clear();
                core.failure = None;
                core.accepting = true;
            }
            let result = s.send(s.id.root, b"referencesOf:do:", &[value, s.core_visitor]);
            s.walk.borrow_mut().as_mut().unwrap().core.accepting = false;
            if let Some(e) = s.walk.borrow().as_ref().unwrap().core.failure {
                return Err(e);
            }
            if !result?.is_null() {
                return Err(s.invalid());
            }
            let count = s.walk.borrow().as_ref().unwrap().core.values.len();
            if count == 0 {
                return Err(s.invalid());
            }
            let behavior = s.walk.borrow().as_ref().unwrap().core.values[0];
            for i in 0..count {
                let edge = s.walk.borrow().as_ref().unwrap().core.values[i];
                emit(s, edge)?;
            }
            if let Some(owner) = s.trace_owner(behavior)? {
                emit(s, owner)?;
                s.send(owner, b"referencesOf:do:", &[value, s.receiver])?;
            } else {
                let walk = s.walk.borrow();
                let w = walk.as_ref().unwrap();
                if w.allocations[w.indices[&value.addr()]].size != 0 {
                    return Err(s.invalid());
                }
            }
            if let Some(error) = s.walk.borrow().as_ref().unwrap().failure {
                return Err(error);
            }
        }
        // A trace callback that allocates must not cause us to sweep from an obsolete
        // census. Abort marking without retirement; retry on a later explicit request.
        if s.send(s.id.root, b"allocationEpoch", &[])? != epoch {
            return Err(s.invalid());
        }
        let mut walk = s.walk.borrow_mut();
        let w = walk.as_mut().unwrap();
        if let Some(error) = w.failure {
            return Err(error);
        }
        let mut dead = std::mem::take(&mut w.allocations);
        let mut marks = w.marked.iter();
        dead.retain(|_| *marks.next().unwrap() == 0);
        Ok(dead)
    }
}
unsafe fn sweep(s: &Memory, dead: &[Allocation]) -> Result<usize, Oop> {
    unsafe {
        let objects: Vec<_> = dead.iter().map(|a| a.object).collect();
        // Pass the dead objects as Oop arguments. No managed object is created
        // after marking to hold references that will become invalid on release.
        // Owners remove weak records while every object's allocation is still
        // present. The same Oop argument slice is borrowed by each callback.
        let retirement = s.retirement.borrow().clone();
        for owner in retirement { s.send(owner, b"retire:", &objects)?; }
        s.send(s.id.root, b"removeDeadObjects", &objects)?;
        {
            let mut owners = s.tracers.borrow_mut();
            for a in dead {
                owners.remove(&a.object.addr());
            }
        }
        for a in dead {
            if s.identity(a.object)? != a.identity {
                return Err(s.invalid());
            }
            let token = s.send(s.id.root, b"detachAllocation:", &[a.object])?;
            let owner = s.finalizers.borrow_mut().remove(&a.object.addr());
            if let Some(owner) = owner {
                if owner.identity != a.identity {
                    return Err(s.invalid());
                }
                s.send(owner.object, b"finalize:", &[a.object])?;
            }
            s.send(s.id.root, b"releaseDetachedAllocation:", &[token])?;
        }
        Ok(dead.len())
    }
}
pub(super) unsafe extern "C" fn collect(_id: Oop, c: Oop, _: Oop, _: *const Oop, n: usize) -> Oop {
    (||unsafe{let s=memory(c)?;if n!=0{return Err(s.invalid());}run(s,&[])})().unwrap_or_else(abi::fault_reference)
}
pub(super) unsafe fn run(s: &Memory, native: &[usize]) -> Result<Oop,Oop> {
    match s.phase.get() {
        Phase::Marking | Phase::Finalizing => return Ok(abi::size(0).unwrap()),
        Phase::Inactive | Phase::Failed => return Err(s.invalid()),
        Phase::Idle => {}
    }
    s.phase.set(Phase::Marking);
    let result = match unsafe { mark(s, native) } {
        Ok(mut dead) => {
            s.phase.set(Phase::Finalizing);
            let result = match unsafe { sweep(s, &dead) } {
                Ok(n) => {
                    match unsafe { s.reset_accounting(0) } {
                        Ok(())=>{s.charged.set(0);s.phase.set(Phase::Idle);Ok(abi::size(n).unwrap())},
                        Err(e)=>{s.phase.set(Phase::Failed);Err(e)}
                    }
                }
                Err(e) => {
                    s.phase.set(Phase::Failed);
                    Err(e)
                }
            };
            dead.clear();
            s.walk.borrow_mut().as_mut().unwrap().allocations = dead;
            result
        }
        Err(e) => {
            s.phase.set(Phase::Idle);
            Err(e)
        }
    };
    clear_walk(s);
    result
}

pub(super) unsafe fn validate(s: &Memory) -> Result<usize,Oop>{
    unsafe{
        let epoch=s.send(s.id.root,b"allocationEpoch",&[])?;
        let count=begin_walk(s)?;
        s.send(s.id.root,b"rootsDo:",&[s.receiver])?;s.send(s.roots,b"rootsDo:",&[s.receiver])?;
        let mut charged=0;
        for i in 0..count{
            let row=s.walk.borrow().as_ref().unwrap().allocations[i];
            s.send(s.id.root,b"referencesOf:do:",&[row.object,s.receiver])?;
            let behavior=s.send(s.id.root,b"behaviorOf:",&[row.object])?;
            if row.size!=0 && s.trace_owner(behavior)?.is_none(){return Err(s.invalid());}
            charged=construction::add(charged,s.allocation_charge(row.size)?);
        }
        if s.send(s.id.root,b"allocationEpoch",&[])?!=epoch{return Err(s.invalid());}
        if let Some(error)=s.walk.borrow().as_ref().unwrap().failure{return Err(error);}
        Ok(charged)
    }
}
