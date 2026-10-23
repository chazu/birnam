//! Port of the production census/LIFO walk/ordered retirement algorithm to Oops.
//! All traversal storage remains in the explicit Memory/Walk owner.
use super::*;
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
        Err(e) => return e,
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
        Err(e) => return e,
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
        Err(e) => return e,
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
        values: Vec::new(),
        failure: None,
    });
    let answer = unsafe { s.send(s.id.root, b"allocationsDo:", &[s.receiver]) };
    let inventory = s.inventory.borrow_mut().take().unwrap();
    answer?;
    if let Some(error) = inventory.failure {
        return Err(error);
    }
    let mut rows = inventory.values;
    rows.sort_unstable_by_key(|a| a.identity);
    if rows.windows(2).any(|w| w[0].identity == w[1].identity) {
        return Err(s.invalid());
    }
    Ok(rows)
}
pub(super) fn emit(s: &Memory, v: Oop) -> Result<(), Oop> {
    let mut state = s.walk.borrow_mut();
    let walk = state.as_mut().ok_or(s.invalid())?;
    if walk.emit(v).is_err() {
        walk.failure = Some(s.invalid());
        Err(s.invalid())
    } else { Ok(()) }
}
unsafe fn mark(s: &Memory) -> Result<Vec<Allocation>, Oop> {
    unsafe {
        let epoch = s.send(s.id.root, b"allocationEpoch", &[])?;
        let allocations = inventory(s)?;
        let count = allocations.len();
        let indices: HashMap<_, _> = allocations
            .iter()
            .enumerate()
            .map(|(i, a)| (a.object.addr(), i))
            .collect();
        if indices.len() != count {
            return Err(s.invalid());
        }
        *s.walk.borrow_mut() = Some(Walk {
            allocations,
            indices,
            marked: vec![0; count],
            pending: Vec::new(),
            failure: None,
            core: CoreEdges::default(),
        });
        s.send(s.id.root, b"rootsDo:", &[s.receiver])?;
        s.send(s.roots, b"rootsDo:", &[s.receiver])?;
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
        let walk = s.walk.borrow();
        let w = walk.as_ref().unwrap();
        if let Some(error) = w.failure {
            return Err(error);
        }
        Ok(w.allocations
            .iter()
            .enumerate()
            .filter(|(i, _)| w.marked[*i] == 0)
            .map(|(_, a)| *a)
            .collect())
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
    let s = match unsafe { memory(c) } {
        Ok(s) => s,
        Err(e) => return e,
    };
    if n != 0 {
        return s.invalid();
    }
    match s.phase.get() {
        Phase::Marking | Phase::Finalizing => return abi::size(0).unwrap(),
        Phase::Failed => return s.invalid(),
        Phase::Idle => {}
    }
    s.phase.set(Phase::Marking);
    let result = match unsafe { mark(s) } {
        Ok(dead) => {
            s.phase.set(Phase::Finalizing);
            match unsafe { sweep(s, &dead) } {
                Ok(n) => {
                    s.phase.set(Phase::Idle);
                    Ok(abi::size(n).unwrap())
                }
                Err(e) => {
                    s.phase.set(Phase::Failed);
                    Err(e)
                }
            }
        }
        Err(e) => {
            s.phase.set(Phase::Idle);
            Err(e)
        }
    };
    *s.walk.borrow_mut() = None;
    result.unwrap_or_else(|e| e)
}
