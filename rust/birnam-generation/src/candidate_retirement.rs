//! Logical discard releases candidate roots; TCC observes physical disposition.
use super::*;

unsafe fn mapped_bytes(s: &State, identities: &[usize]) -> Result<usize, Failure> {
    unsafe {
        let tcc = s.named(b"TCC")?;
        let mut total = 0usize;
        for &identity in identities {
            total = total
                .checked_add(
                    abi::as_size(s.send(tcc, b"mappingBytes:", &[word(identity)?])?)
                        .ok_or_else(invalid)?,
                )
                .ok_or_else(invalid)?;
        }
        Ok(total)
    }
}
unsafe fn refresh(s: &State) -> Result<(usize, usize), Failure> {
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
pub(super) unsafe fn telemetry(s: &State, q: &Scope<'_>, bytes: bool) -> Result<Oop, Failure> {
    unsafe {
        let (count, size) = refresh(s)?;
        call(
            s,
            q,
            s.named(b"Integer")?,
            b"fromWord:",
            &[word(if bytes { size } else { count })?],
        )
    }
}
pub(super) unsafe fn watch(s: &State, q: &Scope<'_>, unit: Oop) -> Result<Oop, Failure> {
    unsafe {
        let units = s
            .candidates
            .entries
            .borrow()
            .iter()
            .find(|c| c.unit == unit)
            .map(|c| {
                c.mapped
                    .as_ref()
                    .map_or_else(Vec::new, |m| vec![m.factory_unit])
            });
        let Some(units) = units else {
            return fail(
                s,
                q,
                b"invalidCandidate",
                b"candidate carrier is unknown or discarded",
            );
        };
        for &value in &units {
            q.keep(value)?;
        }
        let mut identities = Vec::new();
        for value in units {
            identities.push(
                abi::as_size(s.send(s.named(b"TCC")?, b"mappingIdentityOf:", &[value])?)
                    .filter(|id| *id != 0)
                    .ok_or_else(invalid)?,
            );
        }
        let mut watched = s.candidates.retained.borrow_mut();
        if !identities.is_empty() && !watched.contains(&identities) {
            watched.push(identities);
        }
        Ok(unit)
    }
}

// File cleanup is explicit and independent of logical release. The handle must
// still be open so exact admitted bytes are available; each deletion is conditional.
pub(super) unsafe fn remove_files(s: &State, q: &Scope<'_>, unit: Oop) -> Result<Oop, Failure> {
    unsafe {
        let snapshot = s
            .candidates
            .entries
            .borrow()
            .iter()
            .find(|c| c.unit == unit)
            .map(|c| (c.path, c.plan.encode(&c.objects), c.envelope.clone()));
        let Some((path, artifact, envelope)) = snapshot else {
            return fail(
                s,
                q,
                b"invalidCandidate",
                b"candidate carrier is unknown or discarded",
            );
        };
        let artifact = artifact.ok_or_else(invalid)?;
        let system = s.named(b"System")?;
        let mut sidecar = s.bytes(path)?;
        sidecar.extend_from_slice(b".candidate");
        let sidecar = s.text(q, &sidecar)?;
        for (path, bytes) in [(path, artifact), (sidecar, envelope)] {
            let contents = s.text(q, &bytes)?;
            let result = call(s, q, system, b"removeFile:ifContents:", &[path, contents])?;
            if result != system {
                return if s.is_error(result)? {
                    Ok(result)
                } else {
                    fail(
                        s,
                        q,
                        b"candidateFileCleanupFailed",
                        b"candidate file removal failed",
                    )
                };
            }
        }
        Ok(unit)
    }
}

pub(super) unsafe fn release(s: &State, q: &Scope<'_>, unit: Oop) -> Result<Oop, Failure> {
    unsafe {
        if !s.candidates.entries.borrow().iter().any(|c| c.unit == unit) {
            return fail(
                s,
                q,
                b"invalidCandidate",
                b"candidate carrier is unknown or already discarded",
            );
        }
        // Release only this native artifact carrier and its opaque policy root.
        // Authored policy handles descriptions and explicit undo through Id.
        s.candidates.entries.borrow_mut().retain(|c| c.unit != unit);
        Ok(unit)
    }
}
