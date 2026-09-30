//! The reflection operations consumed by stdlib and runtime tooling.
use super::*;
use std::collections::HashSet;
#[derive(Clone, Copy)]
pub(super) enum Op {
    Behavior,
    Global,
    Names,
    Methods,
}
impl Op {
    pub fn count(self) -> usize {
        if matches!(self, Self::Names) { 0 } else { 1 }
    }
}
unsafe fn call(
    s: &State,
    q: &Scope<'_>,
    r: Oop,
    name: &[u8],
    args: &[Operand],
) -> Result<Oop, Outcome> {
    unsafe { q.keep(object(s.send(r, name, args)?)?) }
}
pub(super) unsafe fn operation(
    s: &State,
    q: &Scope<'_>,
    op: Op,
    args: &[Operand],
) -> Result<Operand, Outcome> {
    unsafe {
        let nil = s.named(b"nil")?;
        if matches!(op, Op::Behavior) {
            return s.send(s.id.receiver, b"behaviorOf:", args);
        }
        if matches!(op, Op::Global) {
            let exported = s.send(s.ns, b"at:", args)?;
            return if exported.kind() == native::UNIT {
                s.send(s.named(b"Globals")?, b"at:", args)
            } else {
                Ok(exported)
            };
        }
        let mut out = call(s, q, s.named(b"Vector")?, b"new", &[])?;
        if matches!(op, Op::Names) {
            let count = s.send(s.ns, b"count", &[])?.as_word().ok_or_else(invalid)?;
            for index in 0..count {
                let name = call(s, q, s.ns, b"nameAt:", &[Operand::word(index)])?;
                out = call(s, q, out, b"add:", &[Operand::object(name)])?;
            }
            return Ok(Operand::object(out));
        }
        let mut behavior = call(s, q, s.id.receiver, b"behaviorOf:", args)?;
        let mut seen = HashSet::new();
        let mut visited = HashSet::new();
        loop {
            if !visited.insert(behavior) {
                return Err(invalid());
            }
            let owner = s
                .send(s.id.receiver, b"ownerOf:", &[Operand::object(behavior)])?
                .as_object()
                .unwrap_or(behavior);
            q.keep(owner)?;
            let target = s
                .send(s.ns, b"nameOf:", &[Operand::object(owner)])?
                .as_object()
                .unwrap_or(nil);
            q.keep(target)?;
            for index in 0..1_048_576 {
                let Some(selector) = s
                    .send(
                        s.id.receiver,
                        b"selectorAt:in:",
                        &[Operand::word(index), Operand::object(behavior)],
                    )?
                    .as_object()
                else {
                    break;
                };
                q.keep(selector)?;
                if !seen.insert(selector) {
                    continue;
                }
                let entry = s.send(
                    s.id.receiver,
                    b"bindingOf:in:",
                    &[Operand::object(selector), Operand::object(behavior)],
                )?;
                let implementation = if entry.as_entry().is_some() {
                    s.send(s.named(b"Execution")?, b"implementationOfEntry:", &[entry])?
                        .as_object()
                        .unwrap_or(nil)
                } else {
                    nil
                };
                q.keep(implementation)?;
                let mut row = call(s, q, s.named(b"Map")?, b"new", &[])?;
                for (key, value) in [
                    (b"selector".as_slice(), selector),
                    (b"target", target),
                    (b"method", implementation),
                ] {
                    let key = q.keep(s.id.intern_name(key)?)?;
                    row = call(
                        s,
                        q,
                        row,
                        b"at:put:",
                        &[Operand::object(key), Operand::object(value)],
                    )?;
                }
                out = call(s, q, out, b"add:", &[Operand::object(row)])?;
            }
            let Some(parent) = s
                .send(s.id.receiver, b"parentOf:", &[Operand::object(behavior)])?
                .as_object()
            else {
                break;
            };
            behavior = q.keep(parent)?;
        }
        Ok(Operand::object(out))
    }
}
