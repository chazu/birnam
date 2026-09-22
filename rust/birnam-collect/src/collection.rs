//! Immediate admission, LIFO traversal, then ordered retirement. No registry
//! borrow spans a send or a visitor. Foreign payloads are enumerated by owners.
use super::*;

pub(super) struct Walk {
    allocations: Vec<Allocation>,
    indices: HashMap<usize, usize>,
    marked: HashSet<usize>,
    pending: Vec<Oop>,
    failure: Option<Outcome>,
}
impl Walk {
    fn emit(&mut self, value: Oop) -> Result<(), Outcome> {
        if value.is_null() {
            return Ok(());
        }
        if !self.indices.contains_key(&value.addr()) {
            return Err(invalid());
        }
        if self.marked.insert(value.addr()) {
            self.pending.push(value);
        }
        Ok(())
    }
    pub(super) fn marked(&self, value: Oop) -> bool {
        self.marked.contains(&value.addr())
    }
}
unsafe extern "C" fn allocation(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let Some([value, size, identity]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let value = Allocation {
            object: object(*value)?,
            size: size.as_word().ok_or_else(invalid)?,
            identity: identity.as_word().filter(|n| *n != 0).ok_or_else(invalid)?,
        };
        (&mut *context.cast::<Vec<Allocation>>()).push(value);
        Ok(Operand::unit())
    })())
}
pub(super) unsafe fn inventory(memory: &Memory) -> Result<Vec<Allocation>, Outcome> {
    let mut allocations = Vec::<Allocation>::new();
    unsafe {
        memory.send(
            memory.id.receiver,
            b"allocationsDo:",
            &[Operand::entry(NativeEntry::new(
                allocation,
                ptr::from_mut(&mut allocations).cast(),
            ))],
        )
    }?;
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
            marked: HashSet::new(),
            pending: Vec::new(),
            failure: None,
        });
        let visitor = Operand::entry(NativeEntry::new(
            visit,
            ptr::from_ref(memory).cast_mut().cast(),
        ));
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
            charged = charged.saturating_add(
                a.size
                    .saturating_add(size_of::<Oop>())
                    .saturating_add(size_of::<Oop>() - 1)
                    & !(size_of::<Oop>() - 1),
            );
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
unsafe extern "C" fn visit(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let memory = unsafe { &*context.cast::<Memory>() };
    let answer = (|| unsafe {
        let Some([value]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        emit(memory, value.as_object().ok_or_else(invalid)?)?;
        Ok(Operand::unit())
    })();
    if let Err(error) = answer {
        if let Some(walk) = memory.walk.borrow_mut().as_mut() {
            walk.failure = Some(error);
        }
    }
    done(answer)
}
struct CoreEdges {
    values: Vec<Oop>,
    failed: bool,
}
unsafe extern "C" fn core_edge(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let edges = unsafe { &mut *context.cast::<CoreEdges>() };
    if let Some([value]) = unsafe { borrowed_slice(args, count) } {
        if let Some(value) = value.as_object() {
            edges.values.push(value);
            return Outcome::loaded(Operand::unit());
        }
    }
    edges.failed = true;
    invalid()
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
            allocations,
            indices,
            marked: HashSet::new(),
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
        let visitor = Operand::entry(NativeEntry::new(
            visit,
            ptr::from_ref(memory).cast_mut().cast(),
        ));
        memory.send(memory.id.receiver, b"rootsDo:", &[visitor])?;
        memory.send(memory.roots, b"rootsDo:", &[visitor])?;
        loop {
            let next = memory.walk.borrow_mut().as_mut().unwrap().pending.pop();
            let Some(value) = next else {
                break;
            };
            let mut core = CoreEdges {
                values: Vec::new(),
                failed: false,
            };
            let kind = memory
                .send(
                    memory.id.receiver,
                    b"referencesOf:do:",
                    &[
                        Operand::object(value),
                        Operand::entry(NativeEntry::new(
                            core_edge,
                            ptr::from_mut(&mut core).cast(),
                        )),
                    ],
                )?
                .as_word()
                .filter(|k| *k <= 2)
                .ok_or_else(invalid)?;
            if core.failed || core.values.len() < if kind == 1 { 2 } else { 1 } {
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
            .filter(|a| !walk.marked.contains(&a.object.addr()))
            .copied()
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
                .as_entry()
                .filter(|e| e.method.is_some())
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
            // This is transferred allocation ownership, not a cached Id method.
            (release.method.unwrap())(release.context, a.object, ptr::null(), 0).into_result()?;
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
