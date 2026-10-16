//! Compiler-owned candidate lowering; only ordinary values and canonical bytes
//! leave this owner. Generation never receives a Rust CST or compiler state.
use super::*;
use birnam_codegen::{
    backend_request::Request,
    candidate::{Output, Plan},
};
#[derive(Clone, Copy)]
pub(super) enum Op {
    Metadata,
    Analyze,
    Inventory,
    Note,
    Clear,
}
impl Op {
    pub fn count(self) -> usize {
        match self {
            Self::Clear => 0,
            Self::Inventory => 2,
            _ => 1,
        }
    }
}
pub(super) unsafe fn operation(
    s: &State,
    q: &Scope<'_>,
    op: Op,
    a: &[Operand],
) -> Result<Operand, Outcome> {
    unsafe {
        if matches!(op, Op::Clear) {
            s.sources.clear_error();
            return Ok(Operand::object(s.named(b"true")?));
        }
        let source = object(a[0])?;
        if matches!(op, Op::Note) {
            s.sources.note(&s.bytes(source)?);
            return Ok(Operand::object(s.named(b"true")?));
        }
        if matches!(op, Op::Analyze) {
            return Ok(Operand::object(source_metadata::prepare(s,q,source)?.unwrap_or(s.named(b"nil")?)));
        }
        if matches!(op, Op::Metadata) {
            return Ok(Operand::object(
                if let Some(m) = source_metadata::read(s, q, source)? {
                    s.text(q, &m.encode().ok_or_else(invalid)?)?
                } else {
                    s.named(b"nil")?
                },
            ));
        }
        let bytes = s.source_bytes(q,source)?.ok_or_else(invalid)?;
        let selected = s.bytes(object(a[1])?)?;
        let Some(tree) = s.source_tree(q, source, bytes.len())? else {
            return Ok(Operand::object(s.named(b"nil")?));
        };
        let observed=birnam_codegen::source_metadata::Metadata::inspect(&tree,&bytes);
        let compilation=if let Some(record)=compiled_source::find(s,q,source)? {
            let retained=source_metadata::read(s,q,record)?.ok_or_else(invalid)?;
            if retained.encode()!=observed.encode(){return Err(invalid())}
            record
        } else {source_metadata::retain_metadata(s,q,source,&observed)?};
        let Some(plan) = Plan::new(&tree, &bytes, &selected, false) else {
            return Ok(Operand::object(s.named(b"nil")?));
        };
        let Some(Request::Factory {
            program,
            dispositions,
            ..
        }) = plan.requests.first()
        else {
            return Err(invalid());
        };
        let program = s.text(q, program)?;
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
                    q.keep(object(s.send(
                        s.named(b"Integer")?,
                        b"fromWord:",
                        &[Operand::word(*i)],
                    )?)?)?,
                ),
                Output::Global(name) => (b"global".as_slice(), s.text(q, name)?),
            };
            let tag = q.keep(s.id.intern_name(tag)?)?;
            outputs.push(s.vector(q, &[tag, value])?);
        }
        let outputs = s.vector(q, &outputs)?;
        let mut adapters = Vec::new();
        for request in &plan.requests[1..] {
            let Request::Named { arity, synchronous } = request else {
                return Err(invalid());
            };
            let arity = q.keep(object(s.send(
                s.named(b"Integer")?,
                b"fromWord:",
                &[Operand::word(*arity as usize)],
            )?)?)?;
            let style = s.named(if *synchronous { b"true" } else { b"false" })?;
            adapters.push(s.vector(q, &[arity, style])?);
        }
        let adapters = s.vector(q, &adapters)?;
        Ok(Operand::object(
            s.vector(q, &[program, styles, outputs, adapters, compilation])?,
        ))
    }
}
