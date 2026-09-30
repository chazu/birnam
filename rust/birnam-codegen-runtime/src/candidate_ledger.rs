//! Transient observations of the durable current generation. The dictionary is
//! canonical; these snapshots detect drift and retain exact inverse Methods.
use super::*;
use preparation::{Binding, Global};

#[derive(Clone)]
pub(super) struct ManagedBinding {
    pub unit: Oop,
    pub binding: Binding,
}
#[derive(Clone)]
pub(super) struct ManagedGlobal {
    pub unit: Oop,
    pub global: Global,
}
#[derive(Clone)]
pub(super) struct Ledger {
    pub generation: Oop,
    pub source_map: Oop,
    pub digest: Vec<u8>,
    pub selection: Vec<u8>,
    pub bindings: Vec<ManagedBinding>,
    pub globals: Vec<ManagedGlobal>,
    pub units: Vec<Oop>,
    pub drifted: bool,
}
impl Ledger {
    pub(super) fn roots(&self) -> Vec<Oop> {
        let mut roots = vec![self.generation, self.source_map];
        roots.extend(&self.units);
        for b in &self.bindings {
            roots.extend([
                b.unit,
                b.binding.name,
                b.binding.target,
                b.binding.behavior,
                b.binding.selector,
            ]);
            roots.extend(b.binding.method);
        }
        for g in &self.globals {
            roots.extend([g.unit, g.global.name]);
            roots.extend(g.global.value);
        }
        roots
    }
}
pub(super) unsafe fn matches(
    s: &State,
    q: &Scope<'_>,
    ledger: &Ledger,
) -> Result<Option<bool>, Outcome> {
    unsafe {
        if ledger.drifted
            || current(s, q)? != ledger.generation
            || fact(s, q, ledger.generation, b"sourceMap")? != ledger.source_map
            || s.bytes(fact(s, q, ledger.generation, b"baseDigest")?)? != ledger.digest
            || s.bytes(fact(s, q, ledger.generation, b"generationSelection")?)? != ledger.selection
        {
            return Ok(Some(false));
        }
        // Unrelated Store edits do not invalidate a managed generation. They do
        // invalidate a prepared token; here only callbacks during this read are
        // required to leave one coherent observation.
        let basis = preparation::basis(s, q)?;
        let epoch = preparation::epoch(s)?;
        for b in &ledger.bindings {
            let b = &b.binding;
            if preparation::target(s, q, b.name)? != Some((b.target, b.behavior))
                || preparation::direct(s, b.behavior, b.selector)? != b.entry
            {
                return Ok(Some(false));
            }
        }
        for g in &ledger.globals {
            if preparation::global(s, q, g.global.name)? != g.global.value {
                return Ok(Some(false));
            }
        }
        let stable = current(s, q)? == ledger.generation
            && preparation::basis(s, q)? == basis
            && preparation::epoch(s)? == epoch;
        Ok(stable.then_some(true))
    }
}
/// Returns false for a missing ledger or irreversible observed drift. Never
/// adopts an intervening edit merely because reflected facts still say active.
pub(super) unsafe fn reconcile(s: &State, q: &Scope<'_>) -> Result<(bool, Option<Oop>), Outcome> {
    unsafe {
        let Some(ledger) = s.candidates.ledger.borrow().clone() else {
            return Ok((false, None));
        };
        match matches(s, q, &ledger)? {
            Some(true) => return Ok((true, Some(ledger.generation))),
            None => return Ok((false, Some(ledger.generation))),
            Some(false) => {}
        }
        if !ledger.drifted {
            // Publish the cached invalidation before outgoing Store callbacks.
            s.candidates
                .ledger
                .borrow_mut()
                .as_mut()
                .ok_or_else(invalid)?
                .drifted = true;
            let drifted = symbol(s, q, b"drifted")?;
            let no = s.named(b"false")?;
            let nil = s.named(b"nil")?;
            assert(s, q, ledger.generation, b"state", drifted)?;
            for unit in ledger.units {
                if unit != nil {
                    assert(s, q, unit, b"state", drifted)?;
                    assert(s, q, unit, b"rollbackAvailable", no)?;
                }
            }
        }
        Ok((false, Some(ledger.generation)))
    }
}
pub(super) unsafe fn reconcile_candidate(
    s: &State,
    q: &Scope<'_>,
    unit: Oop,
) -> Result<Operand, Outcome> {
    unsafe {
        let known = s
            .candidates
            .ledger
            .borrow()
            .as_ref()
            .is_some_and(|l| l.generation == unit || l.units.contains(&unit));
        if !known {
            return fail(
                s,
                q,
                b"generationNotAdopted",
                b"unit has no current binding ledger",
            );
        }
        if reconcile(s, q)?.0 {
            Ok(Operand::object(unit))
        } else {
            fail(s, q, b"staleGeneration", b"managed generation has drifted")
        }
    }
}
pub(super) unsafe fn adopt_initial(
    s: &State,
    q: &Scope<'_>,
    generation: Oop,
) -> Result<Operand, Outcome> {
    unsafe {
        let existing = s.candidates.ledger.borrow().as_ref().map(|l| l.generation);
        if let Some(existing) = existing {
            if existing != generation {
                return fail(
                    s,
                    q,
                    b"invalidLifecycleTransition",
                    b"initial generation is already adopted",
                );
            }
            return reconcile_candidate(s, q, generation);
        }
        let nil = s.named(b"nil")?;
        if current(s, q)? != generation
            || generation == nil
            || fact(s, q, generation, b"predecessor")? != nil
            || fact(s, q, generation, b"deltaUnit")? != nil
        {
            return fail(
                s,
                q,
                b"invalidLifecycleTransition",
                b"initial adoption requires the durable initial generation",
            );
        }
        let basis = preparation::basis(s, q)?;
        let source_map = fact(s, q, generation, b"sourceMap")?;
        let modules = modules(s, q, source_map)?;
        let rows = seq(s, q, source_map)?;
        let mut components = std::collections::BTreeMap::new();
        for row in rows {
            let row = seq(s, q, row)?;
            let [name, _, unit] = row.as_slice() else {
                return Err(invalid());
            };
            components.insert(s.symbol_bytes(q, *name)?.ok_or_else(invalid)?, *unit);
        }
        let registered = s.sources.entries.borrow().clone();
        if registered.len() != modules.len() {
            return fail(
                s,
                q,
                b"invalidGeneration",
                b"initial source map differs from registered sources",
            );
        }
        for module in &modules {
            let mut found = false;
            for &(name, source) in &registered {
                if s.symbol_bytes(q, name)?.as_deref() == Some(module.name.as_slice())
                    && s.bytes(source)? == module.source
                {
                    found = true;
                    break;
                }
            }
            if !found {
                return fail(
                    s,
                    q,
                    b"invalidGeneration",
                    b"initial source map differs from registered sources",
                );
            }
        }
        let digest = s.bytes(fact(s, q, generation, b"baseDigest")?)?;
        let selection = s.bytes(fact(s, q, generation, b"generationSelection")?)?;
        let Some(authority) =
            generation::dispatch_generation_authority_with(&modules, &selection, |m, i, _| {
                Some(generation::sha256_raw(
                    &emission::named_descriptor(&m.metadata.operations.get(i)?.selector).ok()?,
                ))
            })
        else {
            return fail(
                s,
                q,
                b"invalidGeneration",
                b"initial generation selection is not canonical",
            );
        };
        if digest != generation::raw_digest_spelling(authority) {
            return fail(
                s,
                q,
                b"invalidGeneration",
                b"initial generation digest mismatches",
            );
        }
        let mut ledger = Ledger {
            generation,
            source_map,
            digest,
            selection,
            bindings: vec![],
            globals: vec![],
            units: components.values().copied().collect(),
            drifted: false,
        };
        // Authority parsing may collect unrelated private Block behaviors.
        // Observe dictionary stability around the live binding snapshot, after
        // that inert work. The Store basis still covers the entire adoption.
        let epoch = preparation::epoch(s)?;
        for index in generation::generation_module_order(&modules).ok_or_else(invalid)? {
            let module = &modules[index];
            let unit = components[&module.name];
            for operation in &module.metadata.operations {
                let name = symbol(s, q, &operation.target)?;
                let Some((target, behavior)) = preparation::target(s, q, name)? else {
                    continue;
                };
                let selector = symbol(s, q, &operation.selector)?;
                let entry = preparation::direct(s, behavior, selector)?;
                let method = if let Some(entry) = entry {
                    // Native entries lacking a portable implementation cannot be
                    // adopted into an image-rebuildable rollback baseline.
                    let implementation = s.send(
                        s.named(b"Execution")?,
                        b"implementationOfEntry:",
                        &[Operand::entry(entry)],
                    )?;
                    if implementation.kind() != native::OBJECT {
                        return fail(
                            s,
                            q,
                            b"nonPortableGeneration",
                            b"managed initial binding has no portable implementation",
                        );
                    }
                    let implementation = q.keep(object(implementation)?)?;
                    if call(s, q, implementation, b"codeObject", &[])? == nil {
                        return fail(
                            s,
                            q,
                            b"nonPortableGeneration",
                            b"managed initial binding has no CodeObject",
                        );
                    }
                    Some(call(
                        s,
                        q,
                        s.named(b"Execution")?,
                        b"methodOfEntry:",
                        &[Operand::entry(entry)],
                    )?)
                } else {
                    None
                };
                ledger
                    .bindings
                    .retain(|b| b.binding.name != name || b.binding.selector != selector);
                ledger.bindings.push(ManagedBinding {
                    unit,
                    binding: Binding {
                        name,
                        target,
                        behavior,
                        selector,
                        entry,
                        method,
                    },
                });
            }
            for name in &module.metadata.projected_globals {
                let name = symbol(s, q, name)?;
                let value = preparation::global(s, q, name)?;
                ledger.globals.retain(|g| g.global.name != name);
                ledger.globals.push(ManagedGlobal {
                    unit,
                    global: Global { name, value },
                });
            }
        }
        let changed = if preparation::basis(s, q)? != basis {
            Some(b"Store basis changed during initial adoption".as_slice())
        } else if preparation::epoch(s)? != epoch {
            Some(b"behavior epoch changed during initial adoption".as_slice())
        } else if *s.sources.entries.borrow() != registered {
            Some(b"source registry changed during initial adoption".as_slice())
        } else if matches(s, q, &ledger)? != Some(true) {
            Some(b"live bindings changed during initial adoption".as_slice())
        } else {
            None
        };
        if let Some(message) = changed {
            return fail(s, q, b"staleGeneration", message);
        }
        *s.candidates.ledger.borrow_mut() = Some(ledger);
        Ok(Operand::object(generation))
    }
}
