//! Transient observations of the durable current generation. The dictionary is
//! canonical; these snapshots detect drift and retain exact inverse Methods.
use super::*;
use preparation::{Binding, Global};

// Inspect only Block values through their owner messages. Behavior ancestry
// includes compiler-created private Block behaviors; it cannot admit a scalar
// merely because that value happens to answer a similarly named message.
unsafe fn projected_program(
    s: &State,
    q: &Scope<'_>,
    value: Oop,
) -> Result<Option<Vec<u8>>, Outcome> {
    unsafe {
        let block = s.named(b"BlockVt")?;
        let mut behavior = s
            .send(s.id.receiver, b"behaviorOf:", &[Operand::object(value)])?
            .as_object();
        let mut visited = std::collections::HashSet::new();
        loop {
            let Some(current) = behavior else {
                return Ok(None);
            };
            if current == block {
                break;
            }
            if !visited.insert(current) {
                return Ok(None);
            }
            q.keep(current)?;
            behavior = s
                .send(s.id.receiver, b"parentOf:", &[Operand::object(current)])?
                .as_object();
        }
        let nil = s.named(b"nil")?;
        let code = call(s, q, value, b"codeObject", &[])?;
        if code == nil {
            return Ok(None);
        }
        let captures = call(s, q, value, b"captureCount", &[])?;
        if preparation::number(s, captures)? != 0 {
            return Ok(None);
        }
        let bytes = call(
            s,
            q,
            s.named(b"CodeObject")?,
            b"canonicalBytesOf:",
            &[Operand::object(code)],
        )?;
        Ok(Some(s.bytes(bytes)?))
    }
}

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
        // Cache invalidation precedes callbacks. Repeated reconciliation also
        // retries durable publication after a prior Store failure.
        s.candidates
            .ledger
            .borrow_mut()
            .as_mut()
            .ok_or_else(invalid)?
            .drifted = true;
        publish_drift(s, q, &ledger)?;
        Ok((false, Some(ledger.generation)))
    }
}
unsafe fn publish_drift(s: &State, q: &Scope<'_>, ledger: &Ledger) -> Result<(), Outcome> {
    unsafe {
        if s.candidates.observing.replace(true) {
            return Ok(());
        }
        struct Observing<'a>(&'a Cell<bool>);
        impl Drop for Observing<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let _observing = Observing(&s.candidates.observing);
        for value in ledger.roots() {
            q.keep(value)?;
        }
        let drifted = symbol(s, q, b"drifted")?;
        let no = s.named(b"false")?;
        let nil = s.named(b"nil")?;
        assert(s, q, ledger.generation, b"state", drifted)?;
        for &unit in &ledger.units {
            if unit != nil {
                assert(s, q, unit, b"state", drifted)?;
                assert(s, q, unit, b"rollbackAvailable", no)?;
            }
        }
        Ok(())
    }
}
pub(super) fn has_drifted(s: &State) -> bool {
    s.candidates
        .ledger
        .borrow()
        .as_ref()
        .is_some_and(|l| l.drifted)
}
/// Store notifies after the dictionary and Method mirror have committed. A
/// transaction may acknowledge exactly one matching publication, never a whole
/// dynamic scope: nested foreign mutations still invalidate the generation.
pub(crate) unsafe fn method_changed(
    s: &State,
    q: &Scope<'_>,
    selector: Oop,
    behavior: Oop,
) -> Result<Operand, Outcome> {
    unsafe {
        let expected = *s.candidates.expected_mutation.borrow();
        if let Some((key, target, method)) = expected {
            if key == selector
                && target == behavior
                && transaction::is_method(s, preparation::direct(s, behavior, selector)?, method)?
            {
                s.candidates.expected_mutation.borrow_mut().take();
                return Ok(Operand::unit());
            }
        }
        let ledger = s.candidates.ledger.borrow().clone();
        if let Some(ledger) = ledger {
            let managed = ledger
                .bindings
                .iter()
                .any(|b| b.binding.selector == selector && b.binding.behavior == behavior);
            if managed {
                let entry = preparation::direct(s, behavior, selector)?;
                if ledger.drifted
                    || ledger.bindings.iter().any(|b| {
                        b.binding.selector == selector
                            && b.binding.behavior == behavior
                            && b.binding.entry != entry
                    })
                {
                    s.candidates
                        .ledger
                        .borrow_mut()
                        .as_mut()
                        .ok_or_else(invalid)?
                        .drifted = true;
                    publish_drift(s, q, &ledger)?;
                }
            }
        }
        Ok(Operand::unit())
    }
}
/// A saved managed source map must still describe the dispatch it authorizes.
/// Drift never becomes a second authority merely because its code is portable.
pub(crate) unsafe fn validate_image(s: &State, q: &Scope<'_>) -> Result<Operand, Outcome> {
    unsafe {
        if s.candidates.busy.get()
            || s.candidates.observing.get()
            || s.candidates.recovery.borrow().is_some()
        {
            return fail(
                s,
                q,
                b"candidateBusy",
                b"image save cannot interrupt generation publication",
            );
        }
        s.candidates.busy.set(true);
        struct Busy<'a>(&'a Cell<bool>);
        impl Drop for Busy<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let _busy = Busy(&s.candidates.busy);
        if s.candidates.ledger.borrow().is_none() {
            if current(s, q)? == s.named(b"nil")? {
                return Ok(Operand::object(s.named(b"true")?));
            }
            return fail(
                s,
                q,
                b"generationNotAdopted",
                b"managed image has no binding ledger",
            );
        }
        if reconcile(s, q)?.0 {
            Ok(Operand::object(s.named(b"true")?))
        } else {
            fail(
                s,
                q,
                b"staleGeneration",
                b"managed generation drift prevents image save",
            )
        }
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
pub(super) unsafe fn adopt(
    s: &State,
    q: &Scope<'_>,
    generation: Oop,
    composed: bool,
) -> Result<Operand, Outcome> {
    unsafe {
        let identity = s
            .candidates
            .recovery
            .borrow()
            .as_ref()
            .map(|r| r.identity.clone());
        if composed {
            let Some(identity) = identity.filter(|_| s.candidates.recovering_policy.get()) else {
                return fail(
                    s,
                    q,
                    b"imageRecoveryRequired",
                    b"composed adoption requires image recovery",
                );
            };
            if fact(s, q, generation, b"kind")? != symbol(s, q, b"composed")?
                || fact(s, q, generation, b"state")? != symbol(s, q, b"active")?
                || s.bytes(fact(s, q, generation, b"compositionIdentity")?)? != identity
            {
                return fail(
                    s,
                    q,
                    b"invalidGeneration",
                    b"composed generation identity mismatches image recovery",
                );
            }
        }
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
                b"adoption requires the durable predecessor-free generation",
            );
        }
        let store = s.named(b"Store")?;
        let registered = call(
            s,
            q,
            store,
            b"setMethodMutationObserver:",
            &[Operand::object(s.named(b"Codegen")?)],
        )?;
        if registered != store {
            return if s.is_error(registered)? {
                Ok(Operand::object(registered))
            } else {
                fail(
                    s,
                    q,
                    b"generationObservationUnavailable",
                    b"Store did not register generation observation",
                )
            };
        }
        let basis = preparation::basis(s, q)?;
        let source_map = fact(s, q, generation, b"sourceMap")?;
        let modules = modules(s, q, source_map)?;
        let rows = seq(s, q, source_map)?;
        let mut components = std::collections::BTreeMap::new();
        for row in rows {
            let row = seq(s, q, row)?;
            let [name, revision, unit] = row.as_slice() else {
                return Err(invalid());
            };
            if composed
                && (*unit == nil
                    || fact(s, q, *unit, b"kind")? != symbol(s, q, b"composed")?
                    || fact(s, q, *unit, b"state")? != symbol(s, q, b"active")?
                    || fact(s, q, *unit, b"rollbackAvailable")? != s.named(b"false")?
                    || fact(s, q, *unit, b"unitName")? != *name
                    || fact(s, q, *unit, b"sourceRevision")? != *revision
                    || s.bytes(fact(s, q, *unit, b"compositionIdentity")?)?
                        != s.bytes(fact(s, q, generation, b"compositionIdentity")?)?)
            {
                return fail(
                    s,
                    q,
                    b"invalidGeneration",
                    b"composed component has inconsistent authority or rollback state",
                );
            }
            components.insert(s.symbol_bytes(q, *name)?.ok_or_else(invalid)?, *unit);
        }
        let registered = s.sources.entries.borrow().clone();
        if registered.len() != modules.len() {
            return fail(
                s,
                q,
                b"invalidGeneration",
                b"generation source map differs from registered sources",
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
                    b"generation source map differs from registered sources",
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
                b"generation selection is not canonical",
            );
        };
        if digest != generation::raw_digest_spelling(authority) {
            return fail(s, q, b"invalidGeneration", b"generation digest mismatches");
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
        // Multiple source names can address one dictionary. Validate the final
        // writer in dependency order, while retaining every named observation.
        let mut programs = Vec::<(Oop, Oop, Option<&[u8]>, Option<Vec<u8>>)>::new();
        let mut global_programs = Vec::<(Oop, &[u8], Option<Oop>)>::new();
        for index in generation::generation_module_order(&modules).ok_or_else(invalid)? {
            let module = &modules[index];
            let unit = components[&module.name];
            for operation in &module.metadata.operations {
                let name = symbol(s, q, &operation.target)?;
                let Some((target, behavior)) = preparation::target(s, q, name)? else {
                    return fail(
                        s,
                        q,
                        b"sourceBindingMismatch",
                        format!("managed source target is unavailable: {} in {}",
                            String::from_utf8_lossy(&operation.target),
                            String::from_utf8_lossy(&module.name)).as_bytes(),
                    );
                };
                let selector = symbol(s, q, &operation.selector)?;
                let entry = preparation::direct(s, behavior, selector)?;
                let mut actual_program = None;
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
                            b"managed binding has no portable implementation",
                        );
                    }
                    let implementation = q.keep(object(implementation)?)?;
                    let code = call(s, q, implementation, b"codeObject", &[])?;
                    if code == nil {
                        return fail(
                            s,
                            q,
                            b"nonPortableGeneration",
                            b"managed binding has no CodeObject",
                        );
                    }
                    let bytes = call(
                        s,
                        q,
                        s.named(b"CodeObject")?,
                        b"canonicalBytesOf:",
                        &[Operand::object(code)],
                    )?;
                    actual_program = Some(s.bytes(bytes)?);
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
                programs.retain(|(b, sel, _, _)| *b != behavior || *sel != selector);
                programs.push((
                    behavior,
                    selector,
                    operation.program.as_deref(),
                    actual_program,
                ));
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
            for projected in &module.metadata.projected_globals {
                let name = symbol(s, q, &projected.name)?;
                let value = preparation::global(s, q, name)?;
                global_programs.retain(|(n, _, _)| *n != name);
                global_programs.push((name, &projected.program, value));
                ledger.globals.retain(|g| g.global.name != name);
                ledger.globals.push(ManagedGlobal {
                    unit,
                    global: Global { name, value },
                });
            }
        }
        for (name, expected, value) in global_programs {
            let actual = match value {
                Some(value) => projected_program(s, q, value)?,
                None => None,
            };
            if !actual
                .as_deref()
                .is_some_and(|actual| birnam_codegen::portable::matches_source(expected, actual))
            {
                let mut message = b"managed global differs from recorded source: ".to_vec();
                message.extend(s.symbol_bytes(q, name)?.ok_or_else(invalid)?);
                return fail(s, q, b"sourceBindingMismatch", &message);
            }
        }
        for (behavior, selector, expected, actual) in programs {
            let agrees = match (expected, actual) {
                (None, None) => true,
                (Some(source), Some(actual)) => {
                    birnam_codegen::portable::matches_source(source, &actual)
                }
                _ => false,
            };
            if !agrees {
                let name = ledger
                    .bindings
                    .iter()
                    .find(|b| b.binding.behavior == behavior && b.binding.selector == selector)
                    .ok_or_else(invalid)?
                    .binding
                    .name;
                let mut message = b"managed binding differs from recorded source: ".to_vec();
                message.extend(s.symbol_bytes(q, name)?.ok_or_else(invalid)?);
                message.push(b' ');
                message.extend(s.symbol_bytes(q, selector)?.ok_or_else(invalid)?);
                return fail(s, q, b"sourceBindingMismatch", &message);
            }
        }
        let changed = if preparation::basis(s, q)? != basis {
            Some(b"Store basis changed during generation adoption".as_slice())
        } else if preparation::epoch(s)? != epoch {
            Some(b"behavior epoch changed during generation adoption".as_slice())
        } else if *s.sources.entries.borrow() != registered {
            Some(b"source registry changed during generation adoption".as_slice())
        } else if matches(s, q, &ledger)? != Some(true) {
            Some(b"live bindings changed during generation adoption".as_slice())
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
