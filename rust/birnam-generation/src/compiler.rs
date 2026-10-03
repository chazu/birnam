//! Live compiler requests. Metadata uses its existing canonical codec; factory
//! inventories contain portable programs and ordinary value collections.
use super::*;
use birnam_codegen::{
    backend_request::Request,
    candidate::{Output, Plan},
    source_metadata::Metadata,
    source_revision::RevisionMetadata,
};
impl State {
    unsafe fn compiler(&self, q: &Scope<'_>, name: &[u8], a: &[Operand]) -> Result<Oop, Outcome> {
        unsafe {
            let codegen = q.keep(self.named(b"Codegen")?)?;
            candidate::call(self, q, codegen, name, a)
        }
    }
    pub(super) unsafe fn note(&self, q: &Scope<'_>, message: &[u8]) -> Result<(), Outcome> {
        unsafe {
            let text = self.text(q, message)?;
            self.compiler(q, b"noteCompilationError:", &[Operand::object(text)])?;
            Ok(())
        }
    }
    pub(super) unsafe fn clear_error(&self, q: &Scope<'_>) -> Result<(), Outcome> {
        unsafe {
            self.compiler(q, b"clearCompilationError", &[])?;
            Ok(())
        }
    }
    pub(super) unsafe fn analyze(&self, q: &Scope<'_>, source: Oop) -> Result<(), Outcome> {
        unsafe {
            self.compiler(q, b"analyzeSourceMetadata:", &[Operand::object(source)])?;
            Ok(())
        }
    }
    pub(super) unsafe fn revision(
        &self,
        q: &Scope<'_>,
        source: Oop,
    ) -> Result<Option<(Vec<u8>, RevisionMetadata)>, Outcome> {
        unsafe {
            let Some(bytes) = self.source_bytes(q, source)? else {
                return Ok(None);
            };
            let encoded =
                self.compiler(q, b"compiledSourceMetadataFor:", &[Operand::object(source)])?;
            if encoded == self.named(b"nil")? {
                self.note(
                    q,
                    b"compiled source metadata is missing; rebuild the artifact or image",
                )?;
                return Ok(None);
            }
            let metadata = Metadata::decode(&self.bytes(encoded)?)
                .filter(|m| m.matches_source(&bytes))
                .ok_or_else(invalid)?;
            Ok(metadata.revision.map(|m| (bytes, m)))
        }
    }
    pub(super) unsafe fn candidate_plan(
        &self,
        q: &Scope<'_>,
        source: Oop,
        selection: &[u8],
    ) -> Result<Option<Plan>, Outcome> {
        unsafe {
            let selected = self.text(q, selection)?;
            let value = self.compiler(
                q,
                b"candidateInventory:selection:",
                &[Operand::object(source), Operand::object(selected)],
            )?;
            if value == self.named(b"nil")? {
                return Ok(None);
            }
            if self.is_error(value)? {
                return Err(Outcome::loaded(Operand::object(value)));
            }
            let parts = candidate::seq(self, q, value)?;
            let [program, styles, outputs, adapters] = parts.as_slice() else {
                return Err(invalid());
            };
            let boolean = |o| -> Result<bool, Outcome> {
                if o == self.named(b"true")? {
                    Ok(true)
                } else if o == self.named(b"false")? {
                    Ok(false)
                } else {
                    Err(invalid())
                }
            };
            let dispositions = candidate::seq(self, q, *styles)?
                .into_iter()
                .map(boolean)
                .collect::<Result<Vec<_>, _>>()?;
            let bytes = self.bytes(source)?;
            let mut requests = vec![Request::Factory {
                program: self.bytes(*program)?,
                source: bytes.clone(),
                dispositions,
            }];
            let mut projected = Vec::new();
            for row in candidate::seq(self, q, *outputs)? {
                let fields = candidate::seq(self, q, row)?;
                let [tag, value] = fields.as_slice() else {
                    return Err(invalid());
                };
                projected.push(match self.symbol_bytes(q, *tag)?.as_deref() {
                    Some(b"method") => Output::Method(
                        self.send(
                            self.named(b"Integer")?,
                            b"wordOf:",
                            &[Operand::object(*value)],
                        )?
                        .as_word()
                        .ok_or_else(invalid)?,
                    ),
                    Some(b"global") => Output::Global(self.bytes(*value)?),
                    _ => return Err(invalid()),
                });
            }
            for row in candidate::seq(self, q, *adapters)? {
                let fields = candidate::seq(self, q, row)?;
                let [arity, style] = fields.as_slice() else {
                    return Err(invalid());
                };
                let arity = self
                    .send(
                        self.named(b"Integer")?,
                        b"wordOf:",
                        &[Operand::object(*arity)],
                    )?
                    .as_word()
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(invalid)?;
                requests.push(Request::Named {
                    arity,
                    synchronous: boolean(*style)?,
                });
            }
            let (_, metadata) = self.revision(q, source)?.ok_or_else(invalid)?;
            Ok(Plan::from_inventory(
                metadata, projected, requests, &bytes, selection,
            ))
        }
    }
}
