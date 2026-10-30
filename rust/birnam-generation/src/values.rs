use super::*;
impl State {
    pub(super) unsafe fn exact(
        &self,
        scope: &Scope<'_>,
        value: Oop,
        family: &[u8],
    ) -> Result<bool, Failure> {
        unsafe {
            let vt = scope.keep(self.named(family)?)?;
            Ok(optional(self.send(self.id.root, b"behaviorOf:", &[value])?) == Some(vt))
        }
    }
    pub(super) unsafe fn source_text(&self, q: &Scope<'_>, value: Oop) -> Result<Oop, Failure> {
        unsafe {
            q.keep(object(self.send(
                self.named(b"Codegen")?,
                b"sourceTextOf:",
                &[value],
            )?)?)
        }
    }
    pub(super) unsafe fn source_data(&self, q: &Scope<'_>, value: Oop) -> Result<Vec<u8>, Failure> {
        unsafe { self.source_bytes(q, value)?.ok_or_else(invalid) }
    }
    pub(super) unsafe fn source_bytes(
        &self,
        scope: &Scope<'_>,
        value: Oop,
    ) -> Result<Option<Vec<u8>>, Failure> {
        unsafe {
            let text = self.source_text(scope, value)?;
            if text == self.named(b"nil")? {
                return Ok(None);
            }
            Ok(Some(self.bytes(text)?))
        }
    }
    pub(super) unsafe fn symbol_bytes(
        &self,
        scope: &Scope<'_>,
        value: Oop,
    ) -> Result<Option<Vec<u8>>, Failure> {
        unsafe {
            if !self.exact(scope, value, b"SymbolVt")? {
                return Ok(None);
            }
            scope.keep(value)?;
            Ok(Some(self.byte_reader.read(value)?))
        }
    }
    pub(super) unsafe fn text(&self, scope: &Scope<'_>, bytes: &[u8]) -> Result<Oop, Failure> {
        unsafe {
            let factory = scope.keep(self.named(b"String")?)?;
            scope.keep(object(
                self.send(
                    factory,
                    b"fromBytes:",
                    &[scope.keep(
                        self.send(
                            self.id.root,
                            b"sequence",
                            &bytes
                                .iter()
                                .map(|b| abi::size(*b as usize).unwrap())
                                .collect::<Vec<_>>(),
                        )?,
                    )?],
                )?,
            )?)
        }
    }
    pub(super) unsafe fn vector(&self, scope: &Scope<'_>, values: &[Oop]) -> Result<Oop, Failure> {
        unsafe {
            let factory = scope.keep(self.named(b"Vector")?)?;
            let mut result = scope.keep(object(self.send(factory, b"new", &[])?)?)?;
            for &value in values {
                result = scope.keep(object(self.send(result, b"add:", &[value])?)?)?;
            }
            Ok(result)
        }
    }
}
