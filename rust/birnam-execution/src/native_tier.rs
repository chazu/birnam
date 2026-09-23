//! Selection of already constructed PC executables. Construction/lowering stays
//! with their producer; this owner retains hotness, live guards and entered code.
use super::*;
use std::rc::Rc;
const HOT_ENTRIES: usize = 64;

pub(super) struct Candidate {
    steps: Vec<Oop>,
    visits: Cell<usize>,
}
impl Candidate {
    pub fn references(&self) -> &[Oop] {
        &self.steps
    }
}

pub(super) unsafe fn install(
    s: &Execution,
    receiver: Oop,
    args: &[Operand],
) -> Result<Operand, Outcome> {
    unsafe {
        let [steps] = args else {
            return Err(invalid());
        };
        let roots = s.scope(&[receiver, object(*steps)?]);
        let registration = s
            .methods
            .borrow()
            .get(&receiver.addr())
            .and_then(|m| m.resumable.clone())
            .ok_or_else(invalid)?;
        // Read effectful sequences before changing the original candidate. Failure
        // preserves its installation, and no native registry borrow spans callbacks.
        let steps = machine::sequence_bounded(
            s,
            object(*steps)?,
            &roots,
            registration.descriptor.program_counter_count(),
        )?;
        if !steps.is_empty() && steps.len() != registration.descriptor.program_counter_count() {
            return Err(invalid());
        }
        for step in &steps {
            let methods = s.methods.borrow();
            let value = methods.get(&step.addr()).ok_or_else(invalid)?;
            if value.resumable.is_some() {
                return Err(invalid());
            }
        }
        let candidate = (!steps.is_empty()).then(|| {
            Rc::new(Candidate {
                steps,
                visits: Cell::new(0),
            })
        });
        s.methods
            .borrow_mut()
            .get_mut(&registration.origin.addr())
            .ok_or_else(invalid)?
            .candidate = candidate;
        Ok(Operand::object(receiver))
    }
}

/// A selected entry is rooted independently of the attached candidate. Callback
/// replacement can withdraw future admission without releasing entered code.
pub(super) unsafe fn select(
    s: &Execution,
    machine: Oop,
    origin: Oop,
    pc: usize,
    roots: &Scope<'_>,
) -> Result<Option<NativeEntry>, Outcome> {
    unsafe {
        let candidate = s
            .methods
            .borrow()
            .get(&origin.addr())
            .and_then(|m| m.candidate.clone());
        let Some(candidate) = candidate else {
            return Ok(None);
        };
        let method = *candidate.steps.get(pc).ok_or_else(invalid)?;
        roots.keep(method);
        if !machine::tier_controls_current(s, machine)? {
            let mut methods = s.methods.borrow_mut();
            if let Some(origin) = methods.get_mut(&origin.addr()) {
                if origin
                    .candidate
                    .as_ref()
                    .is_some_and(|current| Rc::ptr_eq(current, &candidate))
                {
                    origin.candidate = None;
                }
            }
            return Ok(None);
        }
        // A guard callback may install a replacement without changing Id's epoch.
        if !s
            .methods
            .borrow()
            .get(&origin.addr())
            .and_then(|m| m.candidate.as_ref())
            .is_some_and(|current| Rc::ptr_eq(current, &candidate))
        {
            return Ok(None);
        }
        let visits = candidate.visits.get().saturating_add(1);
        candidate.visits.set(visits);
        if visits < HOT_ENTRIES {
            return Ok(None);
        }
        Ok(Some(
            s.methods
                .borrow()
                .get(&method.addr())
                .ok_or_else(invalid)?
                .entry,
        ))
    }
}
