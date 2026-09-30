//! Inert publication-plan snapshots. No candidate factory runs here. Each token
//! roots the precise inverse bindings and is bound to Store and Id observations.
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
pub(super) struct Prepared {
    pub rollback: bool,
    pub token: usize,
    pub unit: Oop,
    pub plan: [Oop; 8],
    pub basis: usize,
    pub epoch: usize,
    pub bindings: Vec<Binding>,
    pub globals: Vec<Global>,
}
impl Prepared {
    pub(super) fn roots(&self) -> Vec<Oop> {
        let mut roots = self.plan.to_vec();
        roots.push(self.unit);
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
pub(super) unsafe fn basis(s: &State, q: &Scope<'_>) -> Result<usize, Outcome> {
    unsafe { number(s, call(s, q, s.named(b"db")?, b"basis", &[])?) }
}
pub(super) unsafe fn epoch(s: &State) -> Result<usize, Outcome> {
    unsafe {
        s.send(s.id.receiver, b"behaviorEpoch", &[])?
            .as_word()
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
        let entity = symbol(s, q, b"__globals__")?;
        let value = s.send(
            s.named(b"db")?,
            b"rawCurrentValue:attr:",
            &[Operand::object(entity), Operand::object(name)],
        )?;
        if value.kind() == native::UNIT {
            Ok(None)
        } else {
            q.keep(object(value)?).map(Some)
        }
    }
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
/// Numeric Store basis catches callbacks/finalizers even if they restore facts;
/// the behavior epoch also catches raw dictionary mutation without a Store write.
pub(super) unsafe fn current_snapshot(
    s: &State,
    q: &Scope<'_>,
    p: &Prepared,
) -> Result<bool, Outcome> {
    unsafe {
        if current(s, q)? != p.plan[6] || basis(s, q)? != p.basis || epoch(s)? != p.epoch {
            return Ok(false);
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
        Ok(current(s, q)? == p.plan[6] && basis(s, q)? == p.basis && epoch(s)? == p.epoch)
    }
}
unsafe fn generation_plan(
    s: &State,
    q: &Scope<'_>,
    plan: &[Oop; 8],
    expected: &[u8; 64],
    selected: &[u8],
    metadata: &source_revision::RevisionMetadata,
    source: Oop,
) -> Result<bool, Outcome> {
    unsafe {
        let [_, unit, name, prior_unit, _, _, predecessor, successor] = *plan;
        if current(s, q)? != predecessor
            || s.bytes(fact(s, q, predecessor, b"baseDigest")?)? != *expected
            || fact(s, q, successor, b"predecessor")? != predecessor
            || fact(s, q, successor, b"deltaUnit")? != unit
        {
            return Ok(false);
        }
        let prior_map = fact(s, q, predecessor, b"sourceMap")?;
        let next_map = fact(s, q, successor, b"sourceMap")?;
        let prior = modules(s, q, prior_map)?;
        // Parse every revision field, not merely the source and claimed digest.
        let next = modules(s, q, next_map)?;
        let selection = s.bytes(fact(s, q, predecessor, b"generationSelection")?)?;
        let Some(projection) = generation::project_candidate_generation(
            prior,
            generation::Module::new(s.bytes(source)?, metadata.clone()).ok_or_else(invalid)?,
            &selection,
            selected,
            |m, i, _| {
                Some(generation::sha256_raw(
                    &emission::named_descriptor(&m.metadata.operations.get(i)?.selector).ok()?,
                ))
            },
        ) else {
            return Ok(false);
        };
        if s.bytes(fact(s, q, successor, b"baseDigest")?)?
            != generation::raw_digest_spelling(projection.authority)
            || s.bytes(fact(s, q, successor, b"generationSelection")?)? != projection.selection
        {
            return Ok(false);
        }
        let before = seq(s, q, prior_map)?;
        let after = seq(s, q, next_map)?;
        if before.len() != after.len() {
            return Ok(false);
        }
        let mut replacement = false;
        for (old, new) in before.into_iter().zip(after) {
            let old = seq(s, q, old)?;
            let new = seq(s, q, new)?;
            if old.len() != 3 || new.len() != 3 || old[0] != new[0] {
                return Ok(false);
            }
            if old[0] == name {
                if replacement
                    || old[2] != prior_unit
                    || new[2] != unit
                    || new[1] != fact(s, q, unit, b"sourceRevision")?
                {
                    return Ok(false);
                }
                let Some(module) = next.iter().find(|m| m.name == metadata.name) else {
                    return Ok(false);
                };
                if module.source != s.bytes(source)? {
                    return Ok(false);
                }
                replacement = true;
            } else if old != new {
                return Ok(false);
            }
        }
        Ok(replacement)
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
                    c.source,
                    c.expected,
                    c.selected.clone(),
                    c.plan.metadata.clone(),
                    c.plan.outputs.clone(),
                    c.mapped.is_some(),
                )
            });
        let Some((name, source, expected, selected, metadata, outputs, mapped)) = snapshot else {
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
        let Ok(plan) = <[Oop; 8]>::try_from(seq(s, q, raw_plan)?) else {
            return fail(
                s,
                q,
                b"candidatePreparationFailed",
                b"publication plan has the wrong shape",
            );
        };
        let nil = s.named(b"nil")?;
        if plan[0] != symbol(s, q, b"activate")?
            || plan[1] != unit
            || plan[2] != name
            || plan[3] == unit
            || [plan[4], plan[5], plan[6], plan[7]].contains(&nil)
            || fact(s, q, unit, b"unitName")? != name
            || fact(s, q, unit, b"state")? != symbol(s, q, b"candidate")?
        {
            return fail(
                s,
                q,
                b"candidatePreparationFailed",
                b"publication plan identity or state mismatches",
            );
        }
        let mut prepared = Prepared {
            rollback: false,
            token: 0,
            unit,
            plan,
            basis: number(s, plan[5])?,
            epoch: epoch(s)?,
            bindings: vec![],
            globals: vec![],
        };
        if basis(s, q)? != prepared.basis
            || !generation_plan(s, q, &plan, &expected, &selected, &metadata, source)?
        {
            return fail(
                s,
                q,
                b"staleGeneration",
                b"publication plan does not match the current generation",
            );
        }
        if !ledger::reconcile(s, q)?.0 {
            return fail(
                s,
                q,
                b"staleGeneration",
                b"current generation has no matching live binding ledger",
            );
        }
        // Generation validation can collect unrelated behavior dictionaries.
        // Reconciliation above checks the managed ledger; snapshot its live
        // targets only after that inert parsing and validation have completed.
        prepared.epoch = epoch(s)?;
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
            if entry.is_some()
                && !s.candidates.ledger.borrow().as_ref().is_some_and(|l| {
                    l.bindings
                        .iter()
                        .any(|b| b.binding.name == name && b.binding.selector == selector)
                })
            {
                return fail(
                    s,
                    q,
                    b"unmanagedMethod",
                    b"a present binding requires explicit portable adoption",
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
