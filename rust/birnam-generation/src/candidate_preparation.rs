//! Inert publication-plan snapshots. No candidate factory runs here. Each token
//! roots the precise inverse bindings and is bound to owned fields, Globals and Id observations.
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
#[derive(Clone)]
pub(super) struct Prepared {
    pub rollback: bool,
    pub token: usize,
    pub unit: Oop,
    pub plan: [Oop; 4],
    pub source: Oop,
    pub bindings: Vec<Binding>,
    pub globals: Vec<Global>,
    pub fields: Vec<(Oop, Oop, Oop)>,
}
impl Prepared {
    pub(super) fn roots(&self) -> Vec<Oop> {
        let mut roots = self.plan.to_vec();
        roots.extend([self.unit, self.source]);
        for &(owner, key, value) in &self.fields { roots.extend([owner, key, value]); }
        for b in &self.bindings {
            roots.extend([b.name, b.target, b.behavior, b.selector]);
            roots.extend(b.method);
        }
        for g in &self.globals {
            roots.push(g.name);
            roots.extend(g.value);
        }
        roots
    }
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
/// Observe the lifecycle data read by this plan. Unrelated facts do not participate.
pub(super) unsafe fn plan_fields(s: &State, q: &Scope<'_>, plan: &[Oop; 4]) -> Result<Vec<(Oop, Oop, Oop)>, Outcome> {
    unsafe {
        let mut fields = vec![];
        for (owner, names) in [
            (plan[1], b"unitName state sourceRevision rollbackAvailable provides".as_slice()),
            (plan[3], b"state".as_slice()),
        ] {
            if owner == s.named(b"nil")? { continue; }
            for name in names.split(|b| *b == b' ') {
                fields.push((owner, symbol(s, q, name)?, field(s, q, owner, name)?));
            }
        }
        Ok(fields)
    }
}
/// Explicit coordination observes its read set, binding owners and source revision.
pub(super) unsafe fn current_snapshot(
    s: &State,
    q: &Scope<'_>,
    p: &Prepared,
) -> Result<bool, Outcome> {
    unsafe {
        // These stamps bound this observation only. They are not token expiry
        // conditions: unrelated work between prepare and commit is allowed.
        let observed_globals = global_basis(s, q)?;
        let observed_epoch = epoch(s)?;
        if s.registry.record(p.plan[2]) != Some(p.source)
        {
            return Ok(false);
        }
        for &(owner, key, value) in &p.fields {
            if field(s, q, owner, &s.symbol_bytes(q, key)?.ok_or_else(invalid)?)? != value { return Ok(false); }
        }
        for b in &p.bindings {
            if target(s, q, b.name)? != Some((b.target, b.behavior))
                || direct(s, b.behavior, b.selector)? != b.entry
            {
                return Ok(false);
            }
        }
        for g in &p.globals {
            if global(s, q, g.name)? != g.value {
                return Ok(false);
            }
        }
        Ok(s.registry.record(p.plan[2]) == Some(p.source)
            && global_basis(s, q)? == observed_globals
            && epoch(s)? == observed_epoch)
    }
}
pub(super) unsafe fn prepare(
    s: &State,
    q: &Scope<'_>,
    unit: Oop,
    raw_plan: Oop,
) -> Result<Operand, Outcome> {
    unsafe {
        let snapshot = s
            .candidates
            .entries
            .borrow()
            .iter()
            .find(|c| c.unit == unit)
            .map(|c| {
                (
                    c.name,
                    c.expected,
                    c.plan.metadata.clone(),
                    c.plan.outputs.clone(),
                    c.mapped.is_some(),
                )
            });
        let Some((name, expected, metadata, outputs, mapped)) = snapshot else {
            return fail(s, q, b"invalidCandidate", b"candidate carrier is unknown");
        };
        if !mapped {
            return fail(
                s,
                q,
                b"candidatePreparationFailed",
                b"candidate is not mapped",
            );
        }
        if s.candidates
            .prepared
            .borrow()
            .iter()
            .any(|p| p.unit == unit)
        {
            return fail(
                s,
                q,
                b"candidatePreparationFailed",
                b"candidate is already prepared",
            );
        }
        if !s.exact(q, raw_plan, b"VectorVt")? {
            return fail(
                s,
                q,
                b"candidatePreparationFailed",
                b"publication plan is not a Vector",
            );
        }
        let Ok(request) = <[Oop; 2]>::try_from(seq(s, q, raw_plan)?) else {
            return fail(
                s,
                q,
                b"candidatePreparationFailed",
                b"publication plan has the wrong shape",
            );
        };
        let nil = s.named(b"nil")?;
        let plan = [request[0], request[1], name, active(s, name).unwrap_or(nil)];
        if plan[0] != symbol(s, q, b"activate")?
            || plan[1] != unit
            || plan[2] != name
            || plan[3] == unit
            || field(s, q, unit, b"unitName")? != name
            || field(s, q, unit, b"state")? != symbol(s, q, b"candidate")?
        {
            return fail(
                s,
                q,
                b"candidatePreparationFailed",
                b"publication plan identity or state mismatches",
            );
        }
        let Some(prior_source) = s.registry.record(name) else {
            return fail(s, q, b"moduleNotRegistered", b"candidate module source is absent");
        };
        q.keep(prior_source)?;
        let Some((_, prior_metadata)) = s.revision(q, prior_source)? else { return Err(invalid()); };
        if prior_metadata.revision_digest != expected || !generation::transition_allowed(&prior_metadata, &metadata) {
            return fail(s, q, b"staleGeneration", b"candidate module source has changed");
        }
        let mut prepared = Prepared {
            rollback: false,
            token: 0,
            unit,
            plan,
            source: prior_source,
            bindings: vec![],
            globals: vec![],
            fields: plan_fields(s, q, &plan)?,
        };
        for operation in &metadata.operations {
            let name = symbol(s, q, &operation.target)?;
            let Some((target, behavior)) = target(s, q, name)? else {
                return fail(
                    s,
                    q,
                    b"unstableMethodTarget",
                    b"candidate target is missing or has no owned behavior",
                );
            };
            let selector = symbol(s, q, &operation.selector)?;
            let entry = direct(s, behavior, selector)?;
            if !operation.install && entry.is_none() {
                return fail(
                    s,
                    q,
                    b"missingDirectMethod",
                    b"candidate removal requires a direct binding",
                );
            }
            let method = if let Some(entry) = entry {
                let known = s.send(
                    s.named(b"Execution")?,
                    b"methodOfEntry:",
                    &[Operand::entry(entry)],
                )?;
                Some(if known.kind() == native::UNIT {
                    call(s, q, behavior, b"methodFor:", &[Operand::object(selector)])?
                } else {
                    q.keep(object(known)?)?
                })
            } else {
                None
            };
            prepared.bindings.push(Binding {
                name,
                target,
                behavior,
                selector,
                entry,
                method,
            });
        }
        for output in outputs {
            if let code::Output::Global(name) = output {
                let name = symbol(s, q, &name)?;
                prepared.globals.push(Global {
                    name,
                    value: global(s, q, name)?,
                });
            }
        }
        // Monotonic process-local capabilities, never a native address. Consumed
        // IDs are not reused even if a callback makes this preparation stale.
        let Some(token) = s
            .candidates
            .next_token
            .get()
            .checked_add(1)
            .filter(|n| *n <= isize::MAX as usize)
        else {
            return fail(
                s,
                q,
                b"candidatePreparationFailed",
                b"prepared token space exhausted",
            );
        };
        s.candidates.next_token.set(token);
        let value = call(
            s,
            q,
            s.named(b"Integer")?,
            b"fromWord:",
            &[Operand::word(token)],
        )?;
        if number(s, value)? != token || !current_snapshot(s, q, &prepared)? {
            return fail(
                s,
                q,
                b"staleGeneration",
                b"publication preparation changed its base",
            );
        }
        prepared.token = token;
        s.candidates.prepared.borrow_mut().push(prepared);
        Ok(Operand::object(value))
    }
}
pub(super) unsafe fn discard(s: &State, q: &Scope<'_>, token: Oop) -> Result<Operand, Outcome> {
    unsafe {
        let token = number(s, token)?;
        let prepared = {
            let mut entries = s.candidates.prepared.borrow_mut();
            entries
                .iter()
                .position(|p| p.token == token)
                .map(|i| entries.remove(i))
        };
        let Some(prepared) = prepared else {
            return fail(
                s,
                q,
                b"invalidPreparedToken",
                b"prepared token is unknown or consumed",
            );
        };
        // Keep the answer live while dropping the preparation-only ownership.
        Ok(Operand::object(q.keep(prepared.unit)?))
    }
}
