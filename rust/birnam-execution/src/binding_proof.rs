//! Admit capture borrowing inside Id's dictionary authority.
//! This is never a saved executable or a foreign payload/context read.
use super::*;

pub(super) const SERVICES: &[&[u8]; 5] = &[
    b"usesDefaultLookup:",
    b"bind:to:",
    b"intern:",
    b"symbolNamed:",
    b"nativeBindingsMatch:",
];

// A single Id-owned query checks a reader and its borrowing grant together.
// This prevents a forwarding grant wrapper from mutating the reader after
// receiving a positive result from the canonical grant implementation.
pub(super) unsafe fn matches_all(
    s: &Execution,
    expected: &[(Oop, &'static [u8], NativeEntry)],
) -> Result<bool, Outcome> {
    let prefix = SERVICES.len() * 4;
    if expected.len() > 32 - SERVICES.len() { return Err(invalid()); }
    let mut request = [0usize; 128];
    for (i, (&service, entry)) in SERVICES.iter().zip(s.binding_proof_services).enumerate() {
        let Some(entry) = entry else { return Ok(false); };
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
        Ok(_) | Err(Outcome { status: native::UNBOUND, .. }) => Ok(false),
        Err(error) => Err(error),
    }
}
