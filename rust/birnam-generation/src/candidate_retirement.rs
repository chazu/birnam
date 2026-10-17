//! Logical discard releases candidate roots; TCC observes physical disposition.
use super::*;

fn protected(s: &State, unit: Oop) -> bool {
    s.candidates
        .prepared
        .borrow()
        .iter()
        .any(|p| p.unit == unit)
        || s.candidates
            .entries
            .borrow()
            .iter()
            .any(|c| c.unit == unit && c.activation.is_some())

}
unsafe fn mapped_bytes(s: &State, identities: &[usize]) -> Result<usize, Outcome> {
    unsafe {
        let tcc = s.named(b"TCC")?;
        let mut total = 0usize;
        for &identity in identities {
            total = total
                .checked_add(
                    s.send(tcc, b"mappingBytes:", &[Operand::word(identity)])?
                        .as_word()
                        .ok_or_else(invalid)?,
                )
                .ok_or_else(invalid)?;
        }
        Ok(total)
    }
}
unsafe fn refresh(s: &State) -> Result<(usize, usize), Outcome> {
    unsafe {
        let groups = s.candidates.retained.borrow().clone();
        let mut live = Vec::new();
        let mut bytes = 0usize;
        for group in groups {
            let size = mapped_bytes(s, &group)?;
            if size != 0 {
                bytes = bytes.checked_add(size).ok_or_else(invalid)?;
                live.push(group);
            }
        }
        let count = live.len();
        *s.candidates.retained.borrow_mut() = live;
        Ok((count, bytes))
    }
}
pub(super) unsafe fn telemetry(s: &State, q: &Scope<'_>, bytes: bool) -> Result<Operand, Outcome> {
    unsafe {
        let (count, size) = refresh(s)?;
        call(
            s,
            q,
            s.named(b"Integer")?,
            b"fromWord:",
            &[Operand::word(if bytes { size } else { count })],
        )
        .map(Operand::object)
    }
}
unsafe fn receipt(s: &State, q: &Scope<'_>, unit: Oop, retained: bool) -> Result<Oop, Outcome> {
    unsafe {
        let model = s.named(b"ObjectModel")?;
        let receipt = call(
            s,
            q,
            model,
            b"allocate:bytes:",
            &[
                Operand::object(s.named(b"DiscardReceiptVt")?),
                Operand::word(4 * core::mem::size_of::<Oop>()),
            ],
        )?;
        let fields = [
            unit,
            symbol(s, q, b"discarded")?,
            symbol(s, q, if retained { b"retained" } else { b"disposed" })?,
            if retained {
                symbol(s, q, b"codeGenerationPinned")?
            } else {
                s.named(b"nil")?
            },
        ];
        for (index, field) in fields.into_iter().enumerate() {
            s.send(
                model,
                b"slotAt:in:put:",
                &[
                    Operand::word(index),
                    Operand::object(receipt),
                    Operand::object(field),
                ],
            )?;
        }
        Ok(receipt)
    }
}
pub(super) unsafe fn discard(s: &State, q: &Scope<'_>, unit: Oop) -> Result<Operand, Outcome> {
    unsafe {
        if protected(s, unit) {
            return fail(
                s,
                q,
                b"candidateInUse",
                b"active, prepared or rollback-reachable candidates cannot be discarded",
            );
        }
        let snapshot = s
            .candidates
            .entries
            .borrow()
            .iter()
            .find(|c| c.unit == unit)
            .map(|c| {
                (
                    c.path,
                    c.plan.encode(&c.objects),
                    c.envelope.clone(),
                    c.mapped
                        .as_ref()
                        .map_or_else(Vec::new, |m| m.identities.clone()),
                )
            });
        let Some((path, artifact, envelope, identities)) = snapshot else {
            return fail(
                s,
                q,
                b"invalidCandidate",
                b"candidate carrier is unknown or already discarded",
            );
        };
        let artifact = artifact.ok_or_else(invalid)?;
        refresh(s)?;
        // Allocate both outcomes before publication. Neither receipt retains a
        // native Unit: only the durable carrier and descriptive Symbols.
        let disposed = receipt(s, q, unit, false)?;
        let retained = receipt(s, q, unit, true)?;
        let system = s.named(b"System")?;
        let mut sidecar = s.bytes(path)?;
        sidecar.extend_from_slice(b".candidate");
        let sidecar = s.text(q, &sidecar)?;
        for (path, bytes) in [(path, artifact), (sidecar, envelope)] {
            let contents = s.text(q, &bytes)?;
            let result = call(
                s,
                q,
                system,
                b"removeFile:ifContents:",
                &[Operand::object(path), Operand::object(contents)],
            )?;
            if result != system {
                return if s.is_error(result)? {
                    Ok(Operand::object(result))
                } else {
                    fail(
                        s,
                        q,
                        b"candidateDiscardFailed",
                        b"candidate file removal failed",
                    )
                };
            }
        }
        transaction::discard_fields(s, q, unit)?;
        // No fallible publication follows. Dropping these roots never forcibly
        // unmaps code: live bindings, Methods, Blocks and Machines retain Units.
        s.candidates.entries.borrow_mut().retain(|c| c.unit != unit);
        if identities.is_empty() {
            return Ok(Operand::object(disposed));
        }
        s.candidates.retained.borrow_mut().push(identities.clone());
        let collected = s
            .named(b"GC")
            .and_then(|gc| s.send(gc, b"collectPrecisely", &[]));
        let checkpointed = collected.and_then(|_| {
            s.named(b"TCC")
                .and_then(|tcc| s.send(tcc, b"checkpoint", &[]))
        });
        // A deferred checkpoint is a retained physical outcome, not a failed
        // logical transition. Subsequent ordinary checkpoints finish retirement.
        let empty = checkpointed.is_ok() && mapped_bytes(s, &identities).is_ok_and(|n| n == 0);
        let _ = refresh(s);
        Ok(Operand::object(if empty { disposed } else { retained }))
    }
}
