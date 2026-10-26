//! Operation-local observations of the authored source owner through Id only.
//! No registry state, slot-layout knowledge, cache, or rooting service lives here.
use super::*;
use candidate::{call, seq};

pub(super) unsafe fn owner(s: &State, q: &Scope<'_>) -> Result<Oop, Failure> {
    unsafe {
        let name = q.keep(s.intern(b"SourceRegistry")?)?;
        let owner = call(s, q, s.named(b"Globals")?, b"at:", &[name])?;
        if owner == s.named(b"nil")? {
            return Err(invalid());
        }
        Ok(owner)
    }
}
pub(super) unsafe fn record(
    s: &State,
    q: &Scope<'_>,
    owner: Oop,
    name: Oop,
) -> Result<Option<Oop>, Failure> {
    unsafe {
        let value = call(s, q, owner, b"recordFor:", &[name])?;
        if s.is_error(value)? {
            return Err(Failure::Value(value));
        }
        Ok((value != s.named(b"nil")?).then_some(value))
    }
}
pub(super) unsafe fn revision(s: &State, q: &Scope<'_>, owner: Oop) -> Result<usize, Failure> {
    unsafe { s.index(q, owner, b"revision") }
}
pub(super) unsafe fn entries(
    s: &State,
    q: &Scope<'_>,
    owner: Oop,
) -> Result<Vec<(Oop, Oop)>, Failure> {
    unsafe {
        let rows = call(s, q, owner, b"sourceRegistry", &[])?;
        let rows = seq(s, q, rows)?;
        let (&tag, rows) = rows.split_first().ok_or_else(invalid)?;
        if tag != s.intern(b"sourceRegistryV1")? {
            return Err(invalid());
        }
        let mut entries = Vec::new();
        for &row in rows {
            let fields = seq(s, q, row)?;
            let [name, record] = fields.as_slice() else {
                return Err(invalid());
            };
            entries.push((*name, *record));
        }
        Ok(entries)
    }
}
