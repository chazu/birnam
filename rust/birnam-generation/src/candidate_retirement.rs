//! Logical discard releases candidate roots; TCC observes physical disposition.
use super::*;

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
pub(super) unsafe fn watch(s: &State, q: &Scope<'_>, unit: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let units = s.candidates.entries.borrow().iter().find(|c| c.unit == unit)
            .map(|c| c.mapped.as_ref().map_or_else(Vec::new, |m| std::iter::once(m.factory_unit).chain(m.adapters.iter().copied()).collect::<Vec<_>>()));
        let Some(units) = units else { return fail(s, q, b"invalidCandidate", b"candidate carrier is unknown or discarded"); };
        for &value in &units { q.keep(value)?; }
        let mut identities = Vec::new();
        for value in units {
            identities.push(s.send(s.named(b"TCC")?, b"mappingIdentityOf:", &[Operand::object(value)])?
                .as_word().filter(|id| *id != 0).ok_or_else(invalid)?);
        }
        let mut watched = s.candidates.retained.borrow_mut();
        if !identities.is_empty() && !watched.contains(&identities) { watched.push(identities); }
        Ok(Operand::object(unit))
    }
}

// File cleanup is explicit and independent of logical release. The handle must
// still be open so exact admitted bytes are available; each deletion is conditional.
pub(super) unsafe fn remove_files(s: &State, q: &Scope<'_>, unit: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let snapshot = s.candidates.entries.borrow().iter().find(|c| c.unit == unit)
            .map(|c| (c.path, c.plan.encode(&c.objects), c.envelope.clone()));
        let Some((path, artifact, envelope)) = snapshot else {
            return fail(s, q, b"invalidCandidate", b"candidate carrier is unknown or discarded");
        };
        let artifact = artifact.ok_or_else(invalid)?;
        let system = s.named(b"System")?;
        let mut sidecar = s.bytes(path)?;
        sidecar.extend_from_slice(b".candidate");
        let sidecar = s.text(q, &sidecar)?;
        for (path, bytes) in [(path, artifact), (sidecar, envelope)] {
            let contents = s.text(q, &bytes)?;
            let result = call(s, q, system, b"removeFile:ifContents:", &[Operand::object(path), Operand::object(contents)])?;
            if result != system {
                return if s.is_error(result)? { Ok(Operand::object(result)) }
                    else { fail(s, q, b"candidateFileCleanupFailed", b"candidate file removal failed") };
            }
        }
        Ok(Operand::object(unit))
    }
}

pub(super) unsafe fn discard(s: &State, q: &Scope<'_>, unit: Oop) -> Result<Operand, Outcome> {
    unsafe {
        if !s.candidates.entries.borrow().iter().any(|c| c.unit == unit) {
            return fail(s, q, b"invalidCandidate", b"candidate carrier is unknown or already discarded");
        }
        // Keep ownership until callback-capable cleanup succeeds. No file, GC or
        // physical-retirement operation participates in this logical release.
        transaction::release_inverse(s, q, unit)?;
        transaction::discard_fields(s, q, unit)?;
        s.candidates.prepared.borrow_mut().retain(|p| p.unit != unit);
        s.candidates.entries.borrow_mut().retain(|c| c.unit != unit);
        Ok(Operand::object(unit))
    }
}
