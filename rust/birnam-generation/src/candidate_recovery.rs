//! Rebuild transient generation ownership only inside an Image publication.
use super::*;

pub(super) struct Recovery {
    pub identity: Vec<u8>,
    entries: Vec<Candidate>,
    prepared: Vec<preparation::Prepared>,
    ledger: Option<ledger::Ledger>,
}
impl Recovery {
    pub(super) fn roots(&self) -> Vec<Oop> {
        self.entries
            .iter()
            .flat_map(Candidate::roots)
            .chain(self.prepared.iter().flat_map(|p| p.roots()))
            .chain(self.ledger.iter().flat_map(|l| l.roots()))
            .collect()
    }
}

pub(crate) unsafe fn begin_recovery(
    s: &State,
    q: &Scope<'_>,
    identity: Oop,
) -> Result<Operand, Outcome> {
    unsafe {
        if s.candidates.busy.get()
            || s.candidates.observing.get()
            || s.candidates.quiescent.get()
            || s.candidates.recovery.borrow().is_some()
        {
            return fail(
                s,
                q,
                b"candidateBusy",
                b"image recovery cannot interrupt a candidate operation",
            );
        }
        let bytes = s.bytes(identity)?;
        if bytes.len() != 64
            || !bytes
                .iter()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
        {
            return fail(
                s,
                q,
                b"invalidImageComposition",
                b"expected a canonical image composition digest",
            );
        }
        // Move all process-local capabilities out of service without releasing
        // their roots. A failed Image transaction can restore the old world.
        *s.candidates.recovery.borrow_mut() = Some(Recovery {
            identity: bytes,
            entries: s.candidates.entries.take(),
            prepared: s.candidates.prepared.take(),
            ledger: s.candidates.ledger.take(),
        });
        Ok(Operand::object(s.named(b"true")?))
    }
}

pub(crate) fn cancel_recovery(s: &State) {
    if let Some(prior) = s.candidates.recovery.take() {
        *s.candidates.entries.borrow_mut() = prior.entries;
        *s.candidates.prepared.borrow_mut() = prior.prepared;
        *s.candidates.ledger.borrow_mut() = prior.ledger;
    }
}

pub(crate) unsafe fn recover_image(
    s: &State,
    q: &Scope<'_>,
    identity: Oop,
) -> Result<Operand, Outcome> {
    unsafe {
        let expected = s
            .candidates
            .recovery
            .borrow()
            .as_ref()
            .map(|r| r.identity.clone());
        if expected.as_deref() != Some(s.bytes(identity)?.as_slice()) {
            return fail(
                s,
                q,
                b"imageRecoveryRequired",
                b"generation recovery requires the admitted Image identity",
            );
        }
        struct Guard<'a>(&'a Cell<bool>);
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        s.candidates.recovering_policy.set(true);
        let _guard = Guard(&s.candidates.recovering_policy);
        let nil = s.named(b"nil")?;
        let generation = current(s, q)?;
        // Drifted revision data is retained as history, never adopted as
        // authority for the actual restored Methods. Its old capabilities were
        // removed at begin_recovery and are retired on successful completion.
        let descriptive = generation != nil
            && fact(s, q, generation, b"state")? == symbol(s, q, b"drifted")?;
        if generation != nil && !descriptive {
            let policy = call(
                s,
                q,
                s.named(b"Globals")?,
                b"at:",
                &[Operand::object(symbol(s, q, b"CompiledUnit")?)],
            )?;
            if policy == nil {
                return fail(
                    s,
                    q,
                    b"imageCompositionRejected",
                    b"managed Image has no lifecycle policy",
                );
            }
            let adopted = call(
                s,
                q,
                policy,
                b"reconcileImageComposition:",
                &[Operand::object(identity)],
            )?;
            if s.is_error(adopted)? {
                return Ok(Operand::object(adopted));
            }
            let ledger = s.candidates.ledger.borrow().clone();
            let valid = if let Some(ledger) = ledger {
                adopted == ledger.generation && ledger::matches(s, q, &ledger)? == Some(true)
            } else {
                false
            };
            if !valid {
                return fail(
                    s,
                    q,
                    b"imageCompositionRejected",
                    b"policy did not adopt the restored live generation",
                );
            }
        }
        // No fallible sends after committing: the old candidate mappings and
        // inverse ledgers lose their roots together. Execution retains any code
        // independently held by active frames, Methods or Blocks.
        let yes = s.named(b"true")?;
        s.candidates.recovery.take();
        Ok(Operand::object(yes))
    }
}
