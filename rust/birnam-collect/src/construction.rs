//! Upper allocation policy delegates through fresh messages to Id storage.
use super::*;
unsafe fn construct(
    memory: &Memory,
    root: Oop,
    operation: impl FnOnce() -> Result<Operand, Outcome>,
) -> Result<Operand, Outcome> {
    memory.check()?;
    if !matches!(memory.phase.get(), Phase::Idle | Phase::Inactive) {
        return Err(invalid());
    }
    let depth = memory.temporary.borrow().len();
    memory.temporary.borrow_mut().push(root);
    let nested = memory.constructing.replace(true);
    let result = (|| unsafe {
        if memory.automatic.get()
            && !nested
            && memory.phase.get() == Phase::Idle
            && !memory.below_threshold()
        {
            collection::conservative(memory)?;
        }
        let answer = operation()?;
        let value = object(answer)?;
        memory.temporary.borrow_mut().push(value);
        let bytes = memory
            .send(memory.id.receiver, b"allocationSizeOf:", &[answer])?
            .as_word()
            .ok_or_else(invalid)?;
        let charge = memory.allocation_charge(bytes)?;
        // Geometry lookup is a live send and may allocate or collect. Add to
        // the current total after it returns, never to a cached pre-call total.
        memory
            .charged
            .set(accounting::add(memory.charged.get(), charge));
        Ok(answer)
    })();
    memory.constructing.set(nested);
    memory.temporary.borrow_mut().truncate(depth);
    result
}
unsafe extern "C" fn allocate(
    context: *mut c_void,
    receiver: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let memory = memory(context)?;
        let Some([bytes]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        bytes.as_word().ok_or_else(invalid)?;
        construct(memory, receiver, || {
            memory.send(
                memory.id.receiver,
                b"allocate:bytes:",
                &[Operand::object(receiver), *bytes],
            )
        })
    })())
}
unsafe extern "C" fn delegated(
    context: *mut c_void,
    receiver: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        if count != 0 {
            return Err(invalid());
        }
        let memory = memory(context)?;
        construct(memory, receiver, || {
            memory.send(
                memory.id.receiver,
                b"newBehaviorWithParent:",
                &[Operand::word(receiver.addr())],
            )
        })
    })())
}
unsafe extern "C" fn intern(
    context: *mut c_void,
    _: Oop,
    args: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let memory = memory(context)?;
        let Some([bytes]) = borrowed_slice(args, count) else {
            return Err(invalid());
        };
        let bytes = bytes.as_bytes().ok_or_else(invalid)?;
        let bytes = birnam_abi::borrowed_bytes(bytes.pointer, bytes.length)
            .ok_or_else(invalid)?
            .to_vec();
        let current = memory.send(
            memory.id.receiver,
            b"symbolNamed:",
            &[Operand::bytes(&bytes)],
        )?;
        if current.kind() == native::OBJECT {
            return Ok(current);
        }
        construct(memory, memory.id.receiver, || {
            memory.send(
                memory.id.receiver,
                b"internBytes:",
                &[Operand::bytes(&bytes)],
            )
        })
    })())
}
// Existing-name interning calls only Id symbolNamed:, whose meaning must be
// fenced separately. With that canonical query and a rooted existing Symbol,
// this entry neither allocates nor collects. This is a code description, not
// an allocation permission or a declaration about a foreign implementation.
pub(super) unsafe extern "C" fn intern_query_semantics(
    context: *mut c_void, _: Oop, args: *const Operand, count: usize,
) -> Outcome {
    let Some([argument]) = (unsafe { borrowed_slice(args, count) }) else { return invalid() };
    let Some(entry) = argument.as_entry() else { return invalid() };
    Outcome::loaded(Operand::word(usize::from(entry == NativeEntry::new(intern, context))))
}
pub(super) unsafe extern "C" fn activate(
    context: *mut c_void,
    _: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let memory = memory(context)?;
        if count != 0 {
            return Err(invalid());
        }
        if memory.phase.get() == Phase::Idle {
            return Ok(Operand::unit());
        }
        if memory.phase.get() != Phase::Inactive {
            return Err(invalid());
        }
        // Already interned selectors; installation introduces no heap allocation.
        // Leave these wrappers installed after a failed validation so repair can
        // allocate while collection remains disabled, then retry activation.
        memory.phase.set(Phase::Marking);
        let result = (|| {
            for (name, method) in [
                (b"allocate".as_slice(), allocate as Method),
                (b"delegated", delegated),
                (b"intern:", intern),
            ] {
                memory.send(
                    memory.id.receiver,
                    b"addMethod",
                    &[
                        Operand::object(memory.selectors[name]),
                        Operand::entry(NativeEntry::new(method, context)),
                    ],
                )?;
            }
            memory.charged.set(collection::validate(memory)?);
            Ok(Operand::unit())
        })();
        *memory.walk.borrow_mut() = None;
        memory.phase.set(if result.is_ok() {
            Phase::Idle
        } else {
            Phase::Inactive
        });
        result
    })())
}

/// Keep the existing allocation admission point even when a private behavior
/// already exists. Callers root their inputs before sending this message.
pub(super) unsafe extern "C" fn prepare(
    context: *mut c_void,
    _: Oop,
    _: *const Operand,
    count: usize,
) -> Outcome {
    done((|| unsafe {
        let memory = memory(context)?;
        if count != 0 || !matches!(memory.phase.get(), Phase::Inactive | Phase::Idle) {
            return Err(invalid());
        }
        if memory.phase.get() == Phase::Idle
            && memory.automatic.get()
            && !memory.constructing.get()
            && !memory.below_threshold()
        {
            collection::conservative(memory)?;
        }
        Ok(Operand::unit())
    })())
}

/// True only for this owner's ordinary allocation path. No budget reservation,
/// allocation charge, collection or finalization is performed by this guard.
pub(super) unsafe extern "C" fn can_omit(
    c: *mut c_void,
    _: Oop,
    a: *const Operand,
    n: usize,
) -> Outcome {
    done((|| unsafe {
        let m = memory(c)?;
        let Some([behavior]) = borrowed_slice(a, n) else {
            return Err(invalid());
        };
        let behavior = object(*behavior)?;
        if m.phase.get() != Phase::Idle {
            return Ok(Operand::word(0));
        }
        let epoch = m
            .send(m.id.receiver, b"behaviorEpoch", &[])?
            .as_word()
            .ok_or_else(invalid)?;
        if epoch == usize::MAX
            || m.send(m.id.receiver, b"usesDefaultAllocation", &[])?
                .as_word()
                != Some(1)
        {
            return Ok(Operand::word(0));
        }
        let expected = [
            (behavior, m.selectors[b"allocate".as_slice()], NativeEntry::new(allocate, c)),
            (m.receiver, m.selectors[b"prepareAllocation".as_slice()], NativeEntry::new(prepare, c)),
        ];
        if !m.binding_guard.admits(m.id, &expected)? {
            for (receiver, name, method) in [
                (behavior, b"allocate".as_slice(), allocate as Method),
                (
                    m.receiver,
                    b"prepareAllocation".as_slice(),
                    prepare as Method,
                ),
            ] {
                if m.send(
                    m.id.receiver,
                    b"usesDefaultLookup:",
                    &[Operand::object(receiver)],
                )?
                .as_word()
                    != Some(1)
                    || m.send(
                        m.id.receiver,
                        b"bind:to:",
                        &[
                            Operand::object(m.selectors[name]),
                            Operand::object(receiver),
                        ],
                    )?
                    .as_entry()
                        != Some(NativeEntry::new(method, c))
                {
                    return Ok(Operand::word(0));
                }
            }
        }
        Ok(Operand::word(usize::from(
            m.phase.get() == Phase::Idle
                && m.send(m.id.receiver, b"behaviorEpoch", &[])?.as_word() == Some(epoch),
        )))
    })())
}
