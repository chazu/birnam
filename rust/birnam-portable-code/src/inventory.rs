//! Value-only projections of this owner's canonical portable inventories.
//! Image framing and heap validation belong to the authored ImageFormat module.
use super::*;
use birnam_artifact::{code_binding_inventory, materialization_selection, portable_inventory};
fn bytes(q: &Scope<'_>, o: Oop) -> Result<Vec<u8>, Oop> {
    let s = q.state;
    if s.behavior(o)? != object(s.named(b"BytesVt")?)? {
        return Err(invalid());
    }
    q.read_bytes(o)
}
fn blob(q: &Scope<'_>, b: &[u8]) -> Result<Oop, Oop> {
    q.keep(object(q.state.send(
        object(q.state.named(b"Bytes")?)?,
        b"fromBytes:",
        &[q.byte_values(b)?],
    )?)?)
}
fn vector(q: &Scope<'_>, values: &[Oop]) -> Result<Oop, Oop> {
    let mut out = q.call(object(q.state.named(b"Vector")?)?, b"new", &[])?;
    for &v in values {
        out = q.call(out, b"add:", &[v])?;
    }
    Ok(out)
}
fn collection(q: &Scope<'_>, o: Oop) -> Result<Vec<Oop>, Oop> {
    let n = q.integer(q.call(o, b"size", &[])?)?.ok_or_else(invalid)?;
    if !(0..=1_048_576).contains(&n) {
        return Err(invalid());
    }
    (0..n).map(|i| q.call(o, b"at:", &[q.number(i)?])).collect()
}
pub(super) fn operation(q: &Scope<'_>, a: &[Oop], op: &[u8]) -> Result<Oop, Oop> {
    let s = q.state;
    match (op, a) {
        (b"inspectInventory:bindings:selection:", [programs, bindings, selection]) => {
            let (programs, bindings, selection) = (
                bytes(q, *programs)?,
                bytes(q, *bindings)?,
                bytes(q, *selection)?,
            );
            let Ok(pi) = portable_inventory::inspect(&programs) else {
                return Ok(s.nil);
            };
            let Ok(bi) = code_binding_inventory::inspect(&bindings) else {
                return Ok(s.nil);
            };
            if materialization_selection::inspect(&selection, &programs, &bindings).is_err() {
                return Ok(s.nil);
            }
            let mut rows = Vec::new();
            for i in 0..pi.program_count as usize {
                let program = pi.program(i).ok_or_else(invalid)?;
                let info = portable::inspect(program).map_err(|_| invalid())?;
                rows.push(vector(
                    q,
                    &[blob(q, program)?, q.number(i64::from(info.capture_count))?],
                )?);
            }
            vector(q, &[vector(q, &rows)?, blob(q, &bi.semantic_digest)?])
        }
        (b"inventory:matchesRoots:", [inventory, roots]) => {
            let inventory = bytes(q, *inventory)?;
            let roots = collection(q, *roots)?
                .into_iter()
                .map(|o| bytes(q, o))
                .collect::<Result<Vec<_>, _>>()?;
            let roots = roots.iter().map(Vec::as_slice).collect::<Vec<_>>();
            object(s.named(
                if portable_inventory::inspect_exact_closure(&inventory, &roots).is_ok() {
                    b"true"
                } else {
                    b"false"
                },
            )?)
        }
        (b"bindings:referencesBelow:", [bindings, count]) => {
            let bindings = bytes(q, *bindings)?;
            let count = q
                .integer(*count)?
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(invalid)?;
            let valid = code_binding_inventory::inspect(&bindings)
                .is_ok_and(|b| b.validate_heap_references(count).is_ok());
            object(s.named(if valid { b"true" } else { b"false" })?)
        }
        _ => Err(invalid()),
    }
}
