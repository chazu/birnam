//! Immediate admission, LIFO traversal, then ordered retirement. No registry
//! borrow spans a send or a visitor. Foreign payloads are enumerated by owners.
use super::*;

pub(super) struct Walk {
    allocations: Vec<Allocation>,
    indices: HashMap<usize, usize>,
    marked: Vec<u8>,
    pending: Vec<Oop>,
    failure: Option<Outcome>,
}
impl Walk {
    fn emit(&mut self, value: Oop) -> Result<(), Outcome> {
        if value.is_null() {
            return Ok(());
        }
        let index = self
            .indices
            .get(&value.addr())
            .copied()
            .ok_or_else(invalid)?;
        if self.marked[index] == 0 {
            self.marked[index] = 1;
            self.pending.push(value);
        }
        Ok(())
    }
    pub(super) fn marked(&self, value: Oop) -> bool {
        self.indices
            .get(&value.addr())
            .is_some_and(|i| self.marked[*i] != 0)
    }
}
pub(super) struct Inventory {
    values: Vec<Allocation>,
    failure: Option<Outcome>,
}
pub(super) unsafe extern "C" fn allocation(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let memory = memory(context)?;
        let mut inventory = memory.inventory.borrow_mut();
        let inventory = inventory.as_mut().ok_or_else(invalid)?;
        let result = (|| {
            if receiver != memory.receiver {
                return Err(invalid());
            }
            let Some([value, size, identity]) = borrowed_slice(args, count) else {
                return Err(invalid());
            };
            inventory.values.push(Allocation {
                object: object(*value)?,
                size: size.as_word().ok_or_else(invalid)?,
                identity: identity.as_word().filter(|n| *n != 0).ok_or_else(invalid)?,
            });
            Ok(Operand::unit())
        })();
        if let Err(e) = result {
            inventory.failure.get_or_insert(e);
        }
        result
    })())
}
pub(super) unsafe fn inventory(memory: &Memory) -> Result<Vec<Allocation>, Outcome> {
    if memory.inventory.borrow().is_some() {
        return Err(invalid());
    }
    *memory.inventory.borrow_mut() = Some(Inventory {
        values: Vec::new(),
        failure: None,
    });
    let result = unsafe {
        memory.send(
            memory.id.receiver,
            b"allocationsDo:",
            &[Operand::object(memory.receiver)],
        )
    };
    let inventory = memory.inventory.borrow_mut().take().unwrap();
    result?;
    if let Some(e) = inventory.failure {
        return Err(e);
    }
    let mut allocations = inventory.values;
    // Allocation identity gives stable creation order independent of Id's index.
    allocations.sort_unstable_by_key(|a| a.identity);
    if allocations
        .windows(2)
        .any(|w| w[0].identity == w[1].identity)
    {
        return Err(invalid());
    }
    Ok(allocations)
}
/// Activation refuses an ordinary native payload without an enumeration owner.
pub(super) unsafe fn validate(memory: &Memory) -> Result<usize, Outcome> {
    unsafe {
        let allocations = inventory(memory)?;
        let indices: HashMap<_, _> = allocations
            .iter()
            .enumerate()
            .map(|(i, a)| (a.object.addr(), i))
            .collect();
        if indices.len() != allocations.len() {
            return Err(invalid());
        }
        *memory.walk.borrow_mut() = Some(Walk {
            allocations: allocations.clone(),
            indices,
            marked: vec![0; allocations.len()],
            pending: Vec::new(),
            failure: None,
        });
        let visitor = Operand::object(memory.receiver);
        memory.send(memory.id.receiver, b"rootsDo:", &[visitor])?;
        memory.send(memory.roots, b"rootsDo:", &[visitor])?;
        let mut charged = 0usize;
        for a in allocations {
            let kind = memory
                .send(
                    memory.id.receiver,
                    b"referencesOf:do:",
                    &[Operand::object(a.object), visitor],
                )?
                .as_word()
                .filter(|k| *k <= 2)
                .ok_or_else(invalid)?;
            if kind == 0 && a.size != 0 {
                let behavior = object(memory.send(
                    memory.id.receiver,
                    b"behaviorOf:",
                    &[Operand::object(a.object)],
                )?)?;
                if memory.trace_owner(behavior)?.is_none() {
                    return Err(invalid());
                }
            }
            charged = accounting::add(charged, memory.allocation_charge(a.size)?);
        }
        if let Some(error) = memory.walk.borrow().as_ref().unwrap().failure {
            return Err(error);
        }
        Ok(charged)
    }
}
pub(super) fn emit(memory: &Memory, value: Oop) -> Result<(), Outcome> {
    let mut state = memory.walk.borrow_mut();
    let walk = state.as_mut().ok_or_else(invalid)?;
    let answer = walk.emit(value);
    if let Err(error) = answer {
        walk.failure = Some(error);
    }
    answer
}
pub(super) unsafe extern "C" fn visit(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let memory = match unsafe { memory(context) } {
        Ok(m) => m,
        Err(e) => return e,
    };
    let answer = (|| unsafe {
        if receiver != memory.receiver || memory.phase.get() != Phase::Marking {
            return Err(invalid());
        }
        let Some([value]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        emit(memory, value.as_object().ok_or_else(invalid)?)?;
        Ok(Operand::unit())
    })();
    if let Err(error) = answer {
        if let Some(walk) = memory.walk.borrow_mut().as_mut() {
            walk.failure.get_or_insert(error);
        }
    }
    done(answer)
}
pub(super) struct CoreEdges {
    values: Vec<Oop>,
    failure: Option<Outcome>,
}
pub(super) unsafe extern "C" fn core_edge(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let memory = memory(context)?;
        let mut edges = memory.core_edges.borrow_mut();
        let edges = edges.as_mut().ok_or_else(invalid)?;
        let result = (|| {
            if receiver != memory.core_visitor || memory.phase.get() != Phase::Marking {
                return Err(invalid());
            }
            let Some([value]) = borrowed_slice(args, count) else {
                return Err(invalid());
            };
            edges.values.push(value.as_object().ok_or_else(invalid)?);
            Ok(Operand::unit())
        })();
        if let Err(e) = result {
            edges.failure.get_or_insert(e);
        }
        result
    })())
}
unsafe fn mark(memory: &Memory, native: &[usize]) -> Result<Vec<Allocation>, Outcome> {
    unsafe {
        let allocations = inventory(memory)?;
        let indices: HashMap<_, _> = allocations
            .iter()
            .enumerate()
            .map(|(i, a)| (a.object.addr(), i))
            .collect();
        if indices.len() != allocations.len() {
            return Err(invalid());
        }
        *memory.walk.borrow_mut() = Some(Walk {
            marked: vec![0; allocations.len()],
            allocations,
            indices,
            pending: Vec::new(),
            failure: None,
        });
        // Conservative native candidates are admitted only if they name an exact
        // allocation. Typed reference providers must emit valid Oops (or null).
        for address in native {
            let value = {
                let state = memory.walk.borrow();
                let walk = state.as_ref().unwrap();
                walk.indices
                    .get(address)
                    .map(|i| walk.allocations[*i].object)
            };
            if let Some(value) = value {
                emit(memory, value)?;
            }
        }
        let visitor = Operand::object(memory.receiver);
        memory.send(memory.id.receiver, b"rootsDo:", &[visitor])?;
        memory.send(memory.roots, b"rootsDo:", &[visitor])?;
        loop {
            let next = memory.walk.borrow_mut().as_mut().unwrap().pending.pop();
            let Some(value) = next else {
                break;
            };
            *memory.core_edges.borrow_mut() = Some(CoreEdges {
                values: Vec::new(),
                failure: None,
            });
            let result = memory.send(
                memory.id.receiver,
                b"referencesOf:do:",
                &[Operand::object(value), Operand::object(memory.core_visitor)],
            );
            let core = memory.core_edges.borrow_mut().take().unwrap();
            let kind = result?.as_word().filter(|k| *k <= 2).ok_or_else(invalid)?;
            if let Some(e) = core.failure {
                return Err(e);
            }
            if core.values.len() < if kind == 1 { 2 } else { 1 } {
                return Err(invalid());
            }
            let behavior = core.values[0];
            // Preserve behavior edge order: header, parent, upper owner, keys.
            // Id alone interprets dictionary storage; upper owners only see Oops.
            let prefix = if kind == 1 { 2 } else { core.values.len() };
            for edge in &core.values[..prefix] {
                emit(memory, *edge)?;
            }
            if kind != 2 {
                if let Some(owner) = memory.trace_owner(behavior)? {
                    emit(memory, owner)?;
                    memory.send(
                        owner,
                        b"referencesOf:do:",
                        &[Operand::object(value), visitor],
                    )?;
                } else if kind == 0 {
                    let size = {
                        let state = memory.walk.borrow();
                        let walk = state.as_ref().unwrap();
                        walk.allocations[walk.indices[&value.addr()]].size
                    };
                    if size != 0 {
                        return Err(invalid());
                    }
                }
            }
            for edge in &core.values[prefix..] {
                emit(memory, *edge)?;
            }
            if let Some(error) = memory.walk.borrow().as_ref().unwrap().failure {
                return Err(error);
            }
        }
        let state = memory.walk.borrow();
        let walk = state.as_ref().unwrap();
        if let Some(error) = walk.failure {
            return Err(error);
        }
        Ok(walk
            .allocations
            .iter()
            .enumerate()
            .filter(|(i, _)| walk.marked[*i] == 0)
            .map(|(_, a)| *a)
            .collect())
    }
}
unsafe fn sweep(memory: &Memory, dead: &[Allocation]) -> Result<usize, Outcome> {
    unsafe {
        // Retire weak keys, Symbols, behavior indexes and weak hook targets before
        // any address can be reused. Keep per-object finalizers live until detach.
        let addresses: HashSet<_> = dead.iter().map(|a| a.object.addr()).collect();
        let bytes: Vec<_> = dead
            .iter()
            .flat_map(|a| a.object.addr().to_ne_bytes())
            .collect();
        let owners = memory.retirement.borrow().clone();
        for owner in owners {
            memory.send(owner, b"retire:", &[Operand::bytes(&bytes)])?;
        }
        memory.send(
            memory.id.receiver,
            b"retireIndexesFor:",
            &[Operand::bytes(&bytes)],
        )?;
        memory
            .tracers
            .borrow_mut()
            .retain(|address, _| !addresses.contains(address));
        for a in dead {
            if memory.identity(a.object)? != a.identity {
                return Err(invalid());
            }
            let release = memory
                .send(
                    memory.id.receiver,
                    b"detachAllocationAt:",
                    &[Operand::word(a.object.addr())],
                )?
                .as_word()
                .filter(|h| *h != 0)
                .ok_or_else(invalid)?;
            // Detachment is a fresh send and may itself reenter. Select/remove
            // the finalizer only after that call has completed.
            let owner = memory.finalizers.borrow_mut().remove(&a.object.addr());
            if let Some(owner) = owner {
                if owner.identity != a.identity {
                    return Err(invalid());
                }
                memory.send(owner.object, b"finalize:", &[Operand::object(a.object)])?;
            }
            memory.send(
                memory.id.receiver,
                b"releaseDetachedAllocation:",
                &[Operand::word(release)],
            )?;
        }
        Ok(dead.len())
    }
}
pub(super) unsafe fn run(memory: &Memory, native: &[usize]) -> Result<Operand, Outcome> {
    memory.check()?;
    match memory.phase.get() {
        Phase::Marking | Phase::Finalizing => return Ok(Operand::word(0)),
        Phase::Idle => {}
        _ => return Err(invalid()),
    }
    memory.phase.set(Phase::Marking);
    let marked = unsafe { mark(memory, native) };
    let result = match marked {
        Ok(dead) => {
            memory.phase.set(Phase::Finalizing);
            match unsafe { sweep(memory, &dead) } {
                Ok(count) => {
                    memory.charged.set(0);
                    memory.phase.set(Phase::Idle);
                    Ok(Operand::word(count))
                }
                // Once retirement starts, failure cannot resume the language on a
                // partially retired heap. Keep detached storage unreclaimed.
                Err(error) => {
                    memory.phase.set(Phase::Failed);
                    Err(error)
                }
            }
        }
        Err(error) => {
            memory.phase.set(Phase::Idle);
            Err(error)
        }
    };
    *memory.walk.borrow_mut() = None;
    result
}
pub(super) unsafe fn conservative(memory: &Memory) -> Result<Operand, Outcome> {
    // Nested collection must return before trying to capture another stack.
    if matches!(memory.phase.get(), Phase::Marking | Phase::Finalizing) {
        return Ok(Operand::word(0));
    }
    unsafe { native_stack::with_roots(|roots| run(memory, roots)) }
}
pub(super) unsafe extern "C" fn collect(
    context: *mut c_void,
    _: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        if count != 0 {
            return Err(invalid());
        }
        conservative(memory(context)?)
    })())
}
pub(super) unsafe extern "C" fn collect_precisely(
    context: *mut c_void,
    _: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        if count != 0 {
            return Err(invalid());
        }
        run(memory(context)?, &[])
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(n: usize) -> Walk {
        let allocations: Vec<_> = (0..n)
            .map(|i| Allocation {
                object: ptr::without_provenance_mut((i + 1) * 16),
                size: i,
                identity: i + 1,
            })
            .collect();
        Walk {
            indices: allocations
                .iter()
                .enumerate()
                .map(|(i, a)| (a.object.addr(), i))
                .collect(),
            marked: vec![0; n],
            allocations,
            pending: Vec::new(),
            failure: None,
        }
    }
    #[test]
    fn indexed_marks_match_hash_set_admission_pending_order_and_retirement() {
        let mut walk = fixture(257);
        let mut expected = HashSet::new();
        let mut pending = Vec::new();
        let mut seed = 73usize;
        for _ in 0..8192 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let value = walk.allocations[seed % walk.allocations.len()].object;
            if expected.insert(value.addr()) {
                pending.push(value);
            }
            walk.emit(value).unwrap();
            assert_eq!(walk.pending, pending);
            assert!(walk.marked(value));
        }
        let before = walk.pending.clone();
        walk.emit(ptr::null_mut()).unwrap();
        assert_eq!(walk.pending, before);
        assert!(walk.emit(ptr::without_provenance_mut(7)).is_err());
        assert_eq!(walk.pending, before);
        assert!(!walk.marked(ptr::without_provenance_mut(7)));
        let dead: Vec<_> = walk
            .allocations
            .iter()
            .enumerate()
            .filter(|(i, _)| walk.marked[*i] == 0)
            .map(|(_, a)| a.identity)
            .collect();
        let reference: Vec<_> = walk
            .allocations
            .iter()
            .filter(|a| !expected.contains(&a.object.addr()))
            .map(|a| a.identity)
            .collect();
        assert_eq!(dead, reference);
    }
    #[test]
    fn sparse_inventory_keeps_creation_order_for_unmarked_objects() {
        let mut walk = fixture(9);
        for i in [8, 1, 3, 1, 8] {
            walk.emit(walk.allocations[i].object).unwrap();
        }
        assert_eq!(
            walk.pending.iter().map(|o| o.addr()).collect::<Vec<_>>(),
            vec![144, 32, 64]
        );
        assert_eq!(
            walk.allocations
                .iter()
                .enumerate()
                .filter(|(i, _)| walk.marked[*i] == 0)
                .map(|(_, a)| a.identity)
                .collect::<Vec<_>>(),
            vec![1, 3, 5, 6, 7, 8]
        );
    }
}
