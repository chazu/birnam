//! Compiler-owned candidate lowering; only ordinary values and canonical bytes
//! leave this owner. Generation never receives a Rust CST or compiler state.
use super::*;
use birnam_codegen::{
    backend_request::Request,
    candidate::{Output, Plan},
};
pub(super) unsafe extern "C" fn inventory(_: Oop, c: Oop, _: Oop, a: *const Oop, n: usize) -> Oop {
    (|| unsafe {
        let s = state(c)?;
        let [source, selected] = borrowed_slice(a, n).ok_or_else(invalid)? else {
            return Err(invalid());
        };
        let q = s.scope(&[*source, *selected]);
        let q = &q;
        let source = *source;
        let bytes = s.source_bytes(q, source)?.ok_or_else(invalid)?;
        let selected = s.bytes(*selected)?;
        let Some(tree) = s.source_tree(q, source, bytes.len())? else {
            return Ok(s.named(b"nil")?);
        };
        let observed = birnam_codegen::source_metadata::Metadata::inspect(&tree, &bytes);
        let compilation = if let Some(record) = compiled_source::find(s, q, source)? {
            let retained = source_metadata::read(s, q, record)?.ok_or_else(invalid)?;
            if retained.encode() != observed.encode() {
                return Err(invalid());
            }
            record
        } else {
            source_metadata::retain_metadata(s, q, source, &observed)?
        };
        let Some(plan) = Plan::new(&tree, &bytes, &selected, false) else {
            return Ok(s.named(b"nil")?);
        };
        let Some(Request::Factory {
            program,
            dispositions,
            ..
        }) = plan.requests.first()
        else {
            return Err(invalid());
        };
        let program = s.source_text(q, program)?;
        let styles = dispositions
            .iter()
            .map(|&b| s.named(if b { b"true" } else { b"false" }))
            .collect::<Result<Vec<_>, _>>()?;
        let styles = s.vector(q, &styles)?;
        let mut outputs = Vec::new();
        for output in &plan.outputs {
            let (tag, value) = match output {
                Output::Method(i) => (
                    b"method".as_slice(),
                    q.keep(s.send(
                        s.named(b"Integer")?,
                        b"fromWord:",
                        &[abi::size(*i).ok_or_else(invalid)?],
                    )?),
                ),
                Output::Global(name) => (b"global".as_slice(), s.source_text(q, name)?),
            };
            let tag = q.keep(s.intern(tag)?);
            outputs.push(s.vector(q, &[tag, value])?);
        }
        let outputs = s.vector(q, &outputs)?;
        let mut adapters = Vec::new();
        for request in &plan.requests[1..] {
            let Request::Named { arity, synchronous } = request else {
                return Err(invalid());
            };
            let arity = q.keep(s.send(
                s.named(b"Integer")?,
                b"fromWord:",
                &[abi::size(*arity as usize).ok_or_else(invalid)?],
            )?);
            let style = s.named(if *synchronous { b"true" } else { b"false" })?;
            adapters.push(s.vector(q, &[arity, style])?);
        }
        let adapters = s.vector(q, &adapters)?;
        Ok(s.vector(q, &[program, styles, outputs, adapters, compilation])?)
    })()
    .unwrap_or_else(abi::fault_reference)
}
