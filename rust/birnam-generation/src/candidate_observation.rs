//! Read-only observations used by optional certification. Candidate policy is authored.
use super::*;

#[derive(Clone)]
pub(super) struct Binding {
    pub name: Oop,
    pub target: Oop,
    pub behavior: Oop,
    pub selector: Oop,
    pub method: Option<Oop>,
}
#[derive(Clone)]
pub(super) struct Global {
    pub name: Oop,
    pub value: Option<Oop>,
}
pub(super) unsafe fn number(s: &State, value: Oop) -> Result<usize, Failure> {
    unsafe {
        abi::as_size(s.send(s.named(b"Integer")?, b"wordOf:", &[value])?)
            .filter(|n| *n <= isize::MAX as usize)
            .ok_or_else(invalid)
    }
}
pub(super) unsafe fn epoch(s: &State) -> Result<usize, Failure> {
    unsafe {
        abi::as_size(s.send(s.id.root, b"bindingEpoch", &[])?)
            .filter(|epoch| *epoch < usize::MAX)
            .ok_or_else(invalid)
    }
}
pub(super) unsafe fn direct(
    s: &State,
    q: &Scope<'_>,
    behavior: Oop,
    selector: Oop,
) -> Result<Option<Oop>, Failure> {
    unsafe {
        let method = call(
            s,
            q,
            s.named(b"Execution")?,
            b"directMethodFor:in:",
            &[selector, behavior],
        )?;
        if s.is_error(method)? {
            return Err(Failure::Value(method));
        }
        Ok((method != s.named(b"nil")?).then_some(method))
    }
}
pub(super) unsafe fn global(s: &State, q: &Scope<'_>, name: Oop) -> Result<Option<Oop>, Failure> {
    unsafe {
        let row = call(s, q, s.named(b"Globals")?, b"bindingAt:", &[name])?;
        let values = seq(s, q, row)?;
        match values.as_slice() {
            [] => Ok(None),
            [value] => Ok(Some(*value)),
            _ => Err(invalid()),
        }
    }
}
pub(super) unsafe fn global_basis(s: &State, q: &Scope<'_>) -> Result<usize, Failure> {
    unsafe { number(s, call(s, q, s.named(b"Globals")?, b"bindingBasis", &[])?) }
}
pub(super) unsafe fn target(
    s: &State,
    q: &Scope<'_>,
    name: Oop,
) -> Result<Option<(Oop, Oop)>, Failure> {
    unsafe {
        // Match source GlobalRead: Globals first, then the native namespace.
        let mut target = call(s, q, s.named(b"Globals")?, b"at:", &[name])?;
        if target == s.named(b"nil")? {
            let fallback = s.send(s.ns, b"at:", &[name])?;
            if fallback.is_null() {
                return Ok(None);
            }
            target = q.keep(object(fallback)?)?;
        }
        if abi::as_size(s.send(s.id.root, b"isBehavior:", &[target])?) == Some(1) {
            // Exported behavior receivers are themselves stable dictionary targets.
            let exported = s.send(s.ns, b"nameOf:", &[target])?;
            return Ok((!exported.is_null()).then_some((target, target)));
        }
        let behavior = call(s, q, s.id.root, b"behaviorOf:", &[target])?;
        let owner = s.send(s.id.root, b"ownerOf:", &[behavior])?;
        Ok((optional(owner) == Some(target)).then_some((target, behavior)))
    }
}
