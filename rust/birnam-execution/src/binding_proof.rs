//! Coalesce fixed ABI admission queries inside Id's dictionary authority.
//! This is never a saved executable or a foreign payload/context read.
use super::*;

pub(super) const SERVICES: &[&[u8]; 5] = &[
    b"usesDefaultLookup:",
    b"bind:to:",
    b"intern:",
    b"symbolNamed:",
    b"nativeBindingsMatch:",
];

pub(super) unsafe fn matches(
    s: &Execution,
    receiver: Oop,
    name: &'static [u8],
    expected: NativeEntry,
) -> Result<bool, Outcome> {
    unsafe { matches_all(s, &[(receiver, name, expected)], false) }
}

// A single Id-owned query checks a reader and its borrowing grant together.
// This prevents a forwarding grant wrapper from mutating the reader after
// receiving a positive result from the canonical grant implementation.
pub(super) unsafe fn matches_all(
    s: &Execution,
    expected: &[(Oop, &'static [u8], NativeEntry)],
    canonical_only: bool,
) -> Result<bool, Outcome> {
    let prefix = SERVICES.len() * 4;
    if expected.len() > 32 - SERVICES.len() { return Err(invalid()); }
    if !s.batch_binding_proofs.get() && !canonical_only {
        for &(receiver, name, entry) in expected {
            if !unsafe { separate(s, receiver, name, entry) }? { return Ok(false); }
        }
        return Ok(true);
    }
    let mut request = [0usize; 128];
    for (i, (&service, entry)) in SERVICES.iter().zip(s.binding_proof_services).enumerate() {
        let Some(entry) = entry else {
            if canonical_only { return Ok(false); }
            for &(receiver, name, entry) in expected {
                if !unsafe { separate(s, receiver, name, entry) }? { return Ok(false); }
            }
            return Ok(true);
        };
        let Some(method) = entry.method else { return Ok(false); };
        request[i * 4..i * 4 + 4].copy_from_slice(&[
            s.id.receiver.addr(), s.names[service].addr(), method as usize, entry.context.addr(),
        ]);
    }
    for (i, &(receiver, name, entry)) in expected.iter().enumerate() {
        let Some(method) = entry.method else { return Ok(false); };
        request[prefix + i * 4..prefix + (i + 1) * 4].copy_from_slice(&[
            receiver.addr(), s.names[name].addr(), method as usize, entry.context.addr(),
        ]);
    }
    let request = &request[..prefix + 4 * expected.len()];
    let bytes = unsafe {core::slice::from_raw_parts(request.as_ptr().cast::<u8>(), size_of_val(request))};
    match unsafe {s.send(s.id.receiver, b"nativeBindingsMatch:", &[Operand::bytes(bytes)])} {
        Ok(value) if value.as_word() == Some(1) => Ok(true),
        Ok(_) | Err(Outcome { status: native::UNBOUND, .. }) => {
            if canonical_only { return Ok(false); }
            for &(receiver, name, entry) in expected {
                if !unsafe { separate(s, receiver, name, entry) }? { return Ok(false); }
            }
            Ok(true)
        }
        Err(error) => Err(error),
    }
}

unsafe fn separate(
    s: &Execution,
    receiver: Oop,
    name: &'static [u8],
    expected: NativeEntry,
) -> Result<bool, Outcome> {
    unsafe {
        if s.id
            .send_named(
                s.id.receiver,
                b"usesDefaultLookup:",
                &[Operand::object(receiver)],
            )?
            .as_word()
            != Some(1)
        {
            return Ok(false);
        }
        Ok(s.id
            .send_named(
                s.id.receiver,
                b"bind:to:",
                &[
                    Operand::object(s.id.intern_name(name)?),
                    Operand::object(receiver),
                ],
            )?
            .as_entry()
            == Some(expected))
    }
}
