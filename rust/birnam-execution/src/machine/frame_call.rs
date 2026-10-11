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
        let (site, sources, copies, selector, namespace, tail) = match args {
            [site, sources, selector, namespace, tail] => {
                (site, sources, None, selector, namespace, tail)
            }
            [site, sources, copies, selector, namespace, tail] => {
                (site, sources, Some(copies), selector, namespace, tail)
            }
            _ => return Err(invalid()),
        };
        let mut transfers = [(0usize, 0usize); 32];
        let mut transfer_count = 0;
        if let Some(copies) = copies {
            let bytes = copies.as_bytes().ok_or_else(invalid)?;
            if bytes.length > 256 || bytes.length % 8 != 0 {
                return Err(invalid());
            }
            let bytes = borrowed_bytes(bytes.pointer, bytes.length).ok_or_else(invalid)?;
            transfer_count = bytes.len() / 8;
            for (pair, bytes) in transfers.iter_mut().zip(bytes.chunks_exact(8)) {
                *pair = (
                    u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize,
                    u32::from_le_bytes(bytes[4..].try_into().unwrap()) as usize,
                );
            }
        }
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
        for &(source, destination) in &transfers[..transfer_count] {
            for index in [source, destination] {
                if descriptor.slot_kind(index) != Some(SlotKind::Oop) {
                    return Err(invalid());
                }
                if index == call.receiver_slot
                    || index == call.selector_slot
                    || (call.argument_start..call.argument_start + call.argument_count)
                        .contains(&index)
                {
                    return Ok(Operand::unit());
                }
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
        let (copied, values) =
            resolve_transfers(&transfers[..transfer_count], &indices[..count], |index| {
                checked(state.engine.read_current_oop(OWNER, cap, index))
            })?;
        // Every source has been checked before the first write. No callbacks
        // occur here; the original logical slots are populated before the call.
        for (&(_, destination), &value) in transfers[..transfer_count].iter().zip(&copied) {
            checked(
                state
                    .engine
                    .write_current_oop(OWNER, cap, destination, value),
            )?;
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

// Simulate ordered copies against the existing frame before committing writes.
// Bounds are admitted by prepare; this function neither allocates nor mutates it.
fn resolve_transfers(
    transfers: &[(usize, usize)],
    indices: &[usize],
    mut read: impl FnMut(usize) -> Result<Oop, Outcome>,
) -> Result<([Oop; 32], [Oop; 17]), Outcome> {
    let mut copied = [ptr::null_mut(); 32];
    for (i, &(source, _)) in transfers.iter().enumerate() {
        copied[i] =
            if let Some(previous) = transfers[..i].iter().rposition(|&(_, dest)| dest == source) {
                copied[previous]
            } else {
                read(source)?
            };
    }
    let mut values = [ptr::null_mut(); 17];
    for (value, &index) in values.iter_mut().zip(indices) {
        *value = if let Some(previous) = transfers.iter().rposition(|&(_, dest)| dest == index) {
            copied[previous]
        } else {
            read(index)?
        };
    }
    Ok((copied, values))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordered_copy_plan_matches_sequential_storage_for_chains_and_overwrites() {
        let initial = [1usize, 2, 3].map(ptr::without_provenance_mut);
        for encoded in 0..729 {
            let mut code = encoded;
            let mut pairs = [(0, 0); 3];
            let mut expected = initial;
            for pair in &mut pairs {
                *pair = (code % 3, (code / 3) % 3);
                code /= 9;
                expected[pair.1] = expected[pair.0];
            }
            let (copies, args) = resolve_transfers(&pairs, &[0, 1, 2], |i| Ok(initial[i])).unwrap();
            let mut committed = initial;
            for ((_, destination), value) in pairs.iter().zip(copies) {
                committed[*destination] = value;
            }
            assert_eq!(committed, expected);
            assert_eq!(&args[..3], &expected);
        }
    }
}
