//! Explicit PC candidates belong to Execution, including their visits and roots.
use super::*;
use std::rc::Rc;

pub(super) const CONTROLS: &[&[u8]] = &[
    b"oopAt:",
    b"oopAt:put:",
    b"returnAt:",
    b"currentFrameView",
    b"callSite:",
    b"tailCallSite:",
    b"continueAt:",
];
pub(super) struct Candidate {
    pub steps: Vec<Oop>,
    visits: Cell<usize>,
}

// Captured aliases share the owner's registration without exposing Method data.
unsafe fn registration(s: &State, method: Oop, q: &Scope<'_>) -> Result<Option<usize>, Oop> {
    unsafe {
        let records: Vec<_> = s
            .resume
            .borrow()
            .records
            .values()
            .map(|r| (r.descriptor.executable_owner.get().addr(), q.keep(r.method)))
            .collect();
        for (context, original) in records {
            if s.send(s.id.root, b"sameNativeMethod:as:", &[method, original])?
                == abi::size(1).unwrap()
            {
                return Ok(Some(context));
            }
        }
        Ok(None)
    }
}

pub(in crate::synchronous) unsafe extern "C" fn install(
    _: Oop,
    c: Oop,
    receiver: Oop,
    a: *const Oop,
    n: usize,
) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [sequence] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[receiver, *sequence]);
        let context = registration(s, receiver, &q)?.ok_or_else(invalid)?;
        let expected = s.resume.borrow().records[&context]
            .descriptor
            .program_counter_count();
        // Read the current sequence completely before publishing a replacement.
        // No registry borrow spans these callbacks, and a failure preserves state.
        let count = index(s, q.keep(s.send(*sequence, b"size", &[])?))?;
        if count != 0 && count != expected {
            return Err(invalid());
        }
        let mut steps = Vec::with_capacity(count);
        for i in 0..count {
            let index = q.keep(number(s, i)?);
            let method = q.keep(s.send(*sequence, b"at:", &[index])?);
            if s.send(s.id.root, b"sameNativeMethod:as:", &[method, method])?
                != abi::size(1).unwrap()
                || registration(s, method, &q)?.is_some()
            {
                return Err(invalid());
            }
            steps.push(method);
        }
        let candidate = (!steps.is_empty()).then(|| {
            Rc::new(Candidate {
                steps,
                visits: Cell::new(0),
            })
        });
        s.resume
            .borrow_mut()
            .records
            .get_mut(&context)
            .ok_or_else(invalid)?
            .candidate = candidate;
        Ok(receiver)
    })()
    .unwrap_or_else(abi::fault_reference)
}

pub(super) unsafe fn select(
    s: &State,
    machine: Oop,
    context: Oop,
    pc: usize,
    q: &Scope<'_>,
) -> Result<Option<Oop>, Oop> {
    unsafe {
        let context = context.addr();
        let candidate = s
            .resume
            .borrow()
            .records
            .get(&context)
            .and_then(|r| r.candidate.clone());
        let Some(candidate) = candidate else {
            return Ok(None);
        };
        let step = q.keep(*candidate.steps.get(pc).ok_or_else(invalid)?);
        if !controls_current(s, machine, CONTROLS)? {
            if let Some(record) = s.resume.borrow_mut().records.get_mut(&context) {
                if record
                    .candidate
                    .as_ref()
                    .is_some_and(|v| Rc::ptr_eq(v, &candidate))
                {
                    record.candidate = None;
                }
            }
            return Ok(None);
        }
        // A replacement made by the proof operation wins over this observation.
        if !s
            .resume
            .borrow()
            .records
            .get(&context)
            .and_then(|r| r.candidate.as_ref())
            .is_some_and(|v| Rc::ptr_eq(v, &candidate))
        {
            return Ok(None);
        }
        let visits = candidate.visits.get().saturating_add(1);
        candidate.visits.set(visits);
        Ok((visits >= 64).then_some(step))
    }
}
