//! Read-only observations used by optional certification. Candidate policy is authored.
use super::*;

#[derive(Clone)]
pub(super) struct Binding {
    pub name: Oop,
    pub target: Oop,
    pub behavior: Oop,
    pub selector: Oop,
    pub entry: Option<NativeEntry>,
    pub method: Option<Oop>,
}
#[derive(Clone)]
pub(super) struct Global {
    pub name: Oop,
    pub value: Option<Oop>,
}
pub(super) unsafe fn number(s: &State, value: Oop) -> Result<usize, Outcome> {
    unsafe {
        s.send(s.named(b"Integer")?, b"wordOf:", &[Operand::object(value)])?
            .as_word()
            .filter(|n| *n <= isize::MAX as usize)
            .ok_or_else(invalid)
    }
}
pub(super) unsafe fn epoch(s: &State) -> Result<usize, Outcome> {
    unsafe {
        s.send(s.id.receiver, b"bindingEpoch", &[])?
            .as_word()
            .filter(|epoch| *epoch < usize::MAX)
            .ok_or_else(invalid)
    }
}
pub(super) unsafe fn direct(
    s: &State,
    behavior: Oop,
    selector: Oop,
) -> Result<Option<NativeEntry>, Outcome> {
    unsafe {
        let value = s.send(
            s.id.receiver,
            b"bindingOf:in:",
            &[Operand::object(selector), Operand::object(behavior)],
        )?;
        if value.kind() == native::UNIT {
            Ok(None)
        } else {
            value.as_entry().map(Some).ok_or_else(invalid)
        }
    }
}
pub(super) unsafe fn global(s: &State, q: &Scope<'_>, name: Oop) -> Result<Option<Oop>, Outcome> {
    unsafe {
        let row = call(
            s,
            q,
            s.named(b"Globals")?,
            b"bindingAt:",
            &[Operand::object(name)],
        )?;
        let values = seq(s, q, row)?;
        match values.as_slice() {
            [] => Ok(None),
            [value] => Ok(Some(*value)),
            _ => Err(invalid()),
        }
    }
}
pub(super) unsafe fn global_basis(s: &State, q: &Scope<'_>) -> Result<usize, Outcome> {
    unsafe { number(s, call(s, q, s.named(b"Globals")?, b"bindingBasis", &[])?) }
}
pub(super) unsafe fn target(
    s: &State,
    q: &Scope<'_>,
    name: Oop,
) -> Result<Option<(Oop, Oop)>, Outcome> {
    unsafe {
        // Match source GlobalRead: Globals first, then the native namespace.
        let mut target = call(s, q, s.named(b"Globals")?, b"at:", &[Operand::object(name)])?;
        if target == s.named(b"nil")? {
            let fallback = s.send(s.ns, b"at:", &[Operand::object(name)])?;
            if fallback.kind() == native::UNIT {
                return Ok(None);
            }
            target = q.keep(object(fallback)?)?;
        }
        if s.send(s.id.receiver, b"isBehavior:", &[Operand::object(target)])?
            .as_word()
            == Some(1)
        {
            // Exported behavior receivers are themselves stable dictionary targets.
            let exported = s.send(s.ns, b"nameOf:", &[Operand::object(target)])?;
            return Ok((exported.kind() != native::UNIT).then_some((target, target)));
        }
        let behavior = call(
            s,
            q,
            s.id.receiver,
            b"behaviorOf:",
            &[Operand::object(target)],
        )?;
        let owner = s.send(s.id.receiver, b"ownerOf:", &[Operand::object(behavior)])?;
        Ok((owner.as_object() == Some(target)).then_some((target, behavior)))
    }
}
