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
        if !nested
            && memory.phase.get() == Phase::Idle
            && memory.charged.get() >= memory.budget.get()
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
        memory.charged.set(
            memory.charged.get().saturating_add(
                (bytes
                    .saturating_add(size_of::<Oop>())
                    .saturating_add(size_of::<Oop>() - 1))
                    & !(size_of::<Oop>() - 1),
            ),
        );
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
