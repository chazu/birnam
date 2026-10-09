//! Optional batching of canonical lookup/binding facts through current Id.
//! Saved entries are equality witnesses only; they are never invoked here.
use crate::{
    id_protocol::{NativeEntry, Operand, Outcome, Protocol, UNBOUND},
    Oop,
};

pub const SERVICES: [&[u8]; 3] = [b"usesDefaultLookup:", b"bind:to:", b"nativeBindingsMatch:"];

pub struct BindingGuard {
    selectors: [Oop; 3],
    entries: [Option<NativeEntry>; 3],
}
impl BindingGuard {
    /// The caller owns and traces these interned selectors for this guard's
    /// lifetime. Semantic declarations are Id protocol contracts, as for
    /// Execution's capture guard. A missing/noncanonical service disables use.
    pub unsafe fn discover(id: Protocol, selectors: [Oop; 3], declaration: Oop) -> Self {
        let entries = core::array::from_fn(|i| {
            let entry = unsafe {
                id.send(
                    id.receiver,
                    selectors[1],
                    &[Operand::object(selectors[i]), Operand::object(id.receiver)],
                )
            }
            .ok()?
            .as_entry()?;
            let known = unsafe {
                id.send(
                    id.receiver,
                    declaration,
                    &[Operand::entry(entry), Operand::bytes(SERVICES[i])],
                )
            }
            .ok()?
            .as_word()
                == Some(1);
            known.then_some(entry)
        });
        Self { selectors, entries }
    }

    /// True admits the pure standard checks. Every other reply declines and
    /// the caller must execute its original ordered checks, including errors.
    /// Inputs remain caller-owned through the synchronous Id invocation.
    pub unsafe fn admits(
        &self,
        id: Protocol,
        expected: &[(Oop, Oop, NativeEntry)],
    ) -> Result<bool, Outcome> {
        if expected.len() > 29 {
            return Ok(false);
        }
        let mut rows = [0usize; 128];
        for i in 0..3 {
            let Some(entry) = self.entries[i] else {
                return Ok(false);
            };
            let Some(method) = entry.method else {
                return Ok(false);
            };
            rows[i * 4..i * 4 + 4].copy_from_slice(&[
                id.receiver.addr(),
                self.selectors[i].addr(),
                method as usize,
                entry.context.addr(),
            ]);
        }
        for (i, &(receiver, selector, entry)) in expected.iter().enumerate() {
            let Some(method) = entry.method else {
                return Ok(false);
            };
            rows[(i + 3) * 4..(i + 4) * 4].copy_from_slice(&[
                receiver.addr(),
                selector.addr(),
                method as usize,
                entry.context.addr(),
            ]);
        }
        let rows = &rows[..(3 + expected.len()) * 4];
        let bytes = unsafe {
            core::slice::from_raw_parts(rows.as_ptr().cast::<u8>(), core::mem::size_of_val(rows))
        };
        // Include the matcher itself: a forwarding wrapper cannot obtain a
        // positive canonical answer then mutate a target before returning it.
        match unsafe { id.send(id.receiver, self.selectors[2], &[Operand::bytes(bytes)]) } {
            Ok(value) => Ok(value.as_word() == Some(1)),
            Err(error) if error.status == UNBOUND => Ok(false),
            Err(error) => Err(error),
        }
    }
}
