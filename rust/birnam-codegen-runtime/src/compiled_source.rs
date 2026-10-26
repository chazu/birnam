//! A compilation is an ordinary persistent Vector: tag, original source, metadata.
//! Registries and revision snapshots own these values. There is no native table,
//! String attachment, or new bootstrap object family.
use super::*;
pub(super) unsafe fn is_record(s: &State, q: &Scope<'_>, value: Oop) -> Result<bool, Oop> {
    unsafe {
        if !s.exact(q, value, b"VectorVt")? || s.index(q.keep(s.send(value, b"size", &[])?))? != 3 {
            return Ok(false);
        }
        let zero = q.keep(s.send(
            s.named(b"Integer")?,
            b"fromWord:",
            &[abi::size(0).ok_or_else(invalid)?],
        )?);
        let tag = q.keep(s.send(value, b"at:", &[zero])?);
        Ok(s.symbol_bytes(q, tag)?.as_deref() == Some(b"compiledSourceV1"))
    }
}
pub(super) unsafe fn field(s: &State, q: &Scope<'_>, value: Oop, index: usize) -> Result<Oop, Oop> {
    unsafe {
        let index = q.keep(s.send(
            s.named(b"Integer")?,
            b"fromWord:",
            &[abi::size(index).ok_or_else(invalid)?],
        )?);
        Ok(q.keep(s.send(value, b"at:", &[index])?))
    }
}
pub(super) unsafe fn text(s: &State, q: &Scope<'_>, value: Oop) -> Result<Option<Oop>, Oop> {
    unsafe {
        if s.exact(q, value, b"StringVt")? {
            return Ok(Some(value));
        }
        if !is_record(s, q, value)? {
            return Ok(None);
        }
        let text = field(s, q, value, 1)?;
        Ok(s.exact(q, text, b"StringVt")?.then_some(text))
    }
}
pub(super) unsafe fn find(s: &State, q: &Scope<'_>, value: Oop) -> Result<Option<Oop>, Oop> {
    unsafe {
        if is_record(s, q, value)? {
            return Ok(Some(value));
        }
        if !s.exact(q, value, b"StringVt")? {
            return Ok(None);
        }
        let owner = s.source_owner(q)?;
        let found = q.keep(s.send(owner, b"compilationForSource:", &[value])?);
        Ok((found != s.named(b"nil")?).then_some(found))
    }
}
pub(super) unsafe fn make(s: &State, q: &Scope<'_>, text: Oop, metadata: Oop) -> Result<Oop, Oop> {
    unsafe {
        let tag = q.keep(s.intern(b"compiledSourceV1")?);
        s.vector(q, &[tag, text, metadata])
    }
}
