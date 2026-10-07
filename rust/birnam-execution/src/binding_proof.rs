//! Coalesce fixed ABI admission queries inside Id's dictionary authority.
//! This is never a saved executable or a foreign payload/context read.
use super::*;

pub(super) const SERVICES: &[&[u8]; 4] = &[
    b"usesDefaultLookup:",
    b"bind:to:",
    b"intern:",
    b"symbolNamed:",
];

pub(super) unsafe fn matches(
    s: &Execution,
    receiver: Oop,
    name: &'static [u8],
    expected: NativeEntry,
) -> Result<bool, Outcome> {
    let Some(method) = expected.method else {
        return Ok(false);
    };
    if !s.batch_binding_proofs.get() {
        return unsafe { separate(s, receiver, name, expected) };
    }
    // Fence the original query and interning meanings as well as the target.
    // A replacement must take the ordinary guard path, in its original order.
    // This relies on bootstrap's canonical, callback-free proof services and
    // an already rooted Symbol; it is not admission for arbitrary guard code.
    let mut request = [0usize; 20];
    for (i, (&service, entry)) in SERVICES.iter().zip(s.binding_proof_services).enumerate() {
        let Some(entry) = entry else {
            return unsafe { separate(s, receiver, name, expected) };
        };
        let Some(method) = entry.method else {
            return Ok(false);
        };
        request[i * 4..i * 4 + 4].copy_from_slice(&[
            s.id.receiver.addr(),
            s.names[service].addr(),
            method as usize,
            entry.context.addr(),
        ]);
    }
    // Initialized Words, not NativeEntry/Operand union padding. Context values
    // are compared by Id; Execution never dereferences a foreign context.
    request[16..].copy_from_slice(&[
        receiver.addr(),
        s.names[name].addr(),
        method as usize,
        expected.context.addr(),
    ]);
    let bytes = unsafe {
        core::slice::from_raw_parts(request.as_ptr().cast::<u8>(), size_of_val(&request))
    };
    match unsafe {
        s.send(
            s.id.receiver,
            b"nativeBindingsMatch:",
            &[Operand::bytes(bytes)],
        )
    } {
        Ok(value) if value.as_word() == Some(1) => Ok(true),
        Ok(_) => unsafe { separate(s, receiver, name, expected) },
        Err(error) if error.status == native::UNBOUND => unsafe {
            separate(s, receiver, name, expected)
        },
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
