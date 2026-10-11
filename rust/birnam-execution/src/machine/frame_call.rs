//! Guarded call-window preparation. No source values or logical frames disappear.
//! Refusal precedes every write, so generated code resumes its ordinary sequence.
use super::*;

unsafe fn canonical_namespace(s: &Execution, namespace: Oop) -> Result<bool, Outcome> {
    unsafe {
        if s.send(
            s.id.receiver,
            b"usesDefaultLookup:",
            &[Operand::object(namespace)],
        )?
        .as_word()
            != Some(1)
        {
            return Ok(false);
        }
        let Some(entry) = lookup_target(s, namespace, s.names[b"at:".as_slice()])? else {
            return Ok(false);
        };
        match s.send(namespace, b"isCanonicalLookup:", &[Operand::entry(entry)]) {
            Ok(v) => Ok(v.as_word() == Some(1)),
            Err(e) if e.status == native::UNBOUND => Ok(false),
            Err(e) => Err(e),
        }
    }
}

pub(super) unsafe fn prepare(
    s: &Execution,
    machine: Oop,
    args: &[Operand],
) -> Result<Operand, Outcome> {
    unsafe {
        let [site, sources, selector, namespace, tail] = args else {
            return Err(invalid());
        };
        let site = site.as_word().ok_or_else(invalid)?;
        let selector = object(*selector)?;
        let namespace = object(*namespace)?;
        let tail = tail.as_word().filter(|v| *v <= 1).ok_or_else(invalid)? != 0;
        let bytes = sources.as_bytes().ok_or_else(invalid)?;
        let bytes = borrowed_bytes(bytes.pointer, bytes.length).ok_or_else(invalid)?;
        if bytes.is_empty() || bytes.len() > 68 || bytes.len() % 4 != 0 {
            return Err(invalid());
        }
        // Copy the caller's borrow before any owner query can run callbacks.
        let mut indices = [0usize; 17];
        let count = bytes.len() / 4;
        for (slot, bytes) in indices.iter_mut().zip(bytes.chunks_exact(4)) {
            *slot = u32::from_le_bytes(bytes.try_into().unwrap()) as usize;
        }
        let cap = cap(s, machine)?;
        let (frame, pc, descriptor, call) = {
            let state = s.machines.borrow();
            let m = checked(state.engine.machine(OWNER, cap))?;
            let frame = m.arena().current_handle().ok_or_else(invalid)?;
            let descriptor = checked(m.arena().descriptor_arc(frame))?;
            let call = *descriptor.call_site(site).ok_or_else(invalid)?;
            (
                frame,
                checked(m.arena().program_counter(frame))?,
                descriptor,
                call,
            )
        };
        let kind = if tail {
            CallSiteKind::TailCall
        } else {
            CallSiteKind::Call
        };
        if call.kind != kind || call.argument_count + 1 != count {
            return Err(invalid());
        }
        for &index in &indices[..count] {
            if descriptor.slot_kind(index) != Some(SlotKind::Oop) {
                return Err(invalid());
            }
            // The original sequential copies must not overwrite later inputs.
            if index == call.receiver_slot
                || index == call.selector_slot
                || (call.argument_start..call.argument_start + call.argument_count).contains(&index)
            {
                return Ok(Operand::unit());
            }
        }
        let epoch = s
            .send(s.id.receiver, b"behaviorEpoch", &[])?
            .as_word()
            .ok_or_else(invalid)?;
        if epoch == usize::MAX
            || s.send(s.id.receiver, b"usesDefaultAllocation", &[])?
                .as_word()
                != Some(1)
            || !canonical_namespace(s, namespace)?
            || (namespace != s.namespace && !canonical_namespace(s, s.namespace)?)
        {
            return Ok(Operand::unit());
        }
        let key = Operand::object(s.names[b"Integer".as_slice()]);
        let integers = object(s.send(namespace, b"at:", &[key])?)?;
        let roots = s.scope(&[integers]);
        if s.integer_factory()? != integers {
            return Ok(Operand::unit());
        }
        match s.send(integers, b"canOmitIndexing", &[]) {
            Ok(v) if v.as_word() == Some(1) => (),
            Ok(_) => return Ok(Operand::unit()),
            Err(e) if e.status == native::UNBOUND => return Ok(Operand::unit()),
            Err(e) => return Err(e),
        }
        let expected = {
            let state = s.machines.borrow();
            let empty = (
                machine,
                b"".as_slice(),
                NativeEntry {
                    method: None,
                    context: ptr::null_mut(),
                },
            );
            let mut entries = [empty; 3];
            for (i, (name, kind)) in [
                (b"oopAt:".as_slice(), Op::Read),
                (b"oopAt:put:", Op::Write),
                (
                    if tail {
                        b"tailCallSite:".as_slice()
                    } else {
                        b"callSite:".as_slice()
                    },
                    Op::Call(tail),
                ),
            ]
            .into_iter()
            .enumerate()
            {
                let op = state
                    .contexts
                    .iter()
                    .find(|op| op.kind == kind)
                    .ok_or_else(invalid)?;
                entries[i] = (
                    machine,
                    name,
                    NativeEntry::new(operation, ptr::from_ref(&**op).cast_mut().cast()),
                );
            }
            entries
        };
        if !crate::binding_proof::matches_all(s, &expected)?
            || s.send(namespace, b"at:", &[key])?.as_object() != Some(integers)
            || s.integer_factory()? != integers
            || s.send(s.id.receiver, b"behaviorEpoch", &[])?.as_word() != Some(epoch)
        {
            return Ok(Operand::unit());
        }
        // All following accesses stay inside Execution. There is no callback,
        // collection, root cleanup or target send between admission and commit.
        let mut state = s.machines.borrow_mut();
        let m = checked(state.engine.machine(OWNER, cap))?;
        if m.arena().current_handle() != Some(frame)
            || checked(m.arena().program_counter(frame))? != pc
        {
            return Ok(Operand::unit());
        }
        let mut values = [ptr::null_mut(); 17];
        for (value, &index) in values.iter_mut().zip(&indices[..count]) {
            *value = checked(state.engine.read_current_oop(OWNER, cap, index))?;
        }
        checked(
            state
                .engine
                .write_current_oop(OWNER, cap, call.receiver_slot, values[0]),
        )?;
        checked(
            state
                .engine
                .write_current_oop(OWNER, cap, call.selector_slot, selector),
        )?;
        for (i, &value) in values[1..count].iter().enumerate() {
            checked(
                state
                    .engine
                    .write_current_oop(OWNER, cap, call.argument_start + i, value),
            )?;
        }
        let State {
            engine, storage, ..
        } = &mut *state;
        let result = checked(engine.publish_call_site(storage, OWNER, cap, site, kind))?;
        drop(state);
        drop(roots);
        Ok(Operand::object(result))
    }
}
