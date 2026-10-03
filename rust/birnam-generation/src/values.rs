use super::*;
impl State {
    pub(super) unsafe fn exact(
        &self,
        scope: &Scope<'_>,
        value: Oop,
        family: &[u8],
    ) -> Result<bool, Outcome> {
        unsafe {
            let vt = scope.keep(self.named(family)?)?;
            Ok(self
                .send(self.id.receiver, b"behaviorOf:", &[Operand::object(value)])?
                .as_object()
                == Some(vt))
        }
    }
    pub(super) unsafe fn source_bytes(
        &self,
        scope: &Scope<'_>,
        value: Oop,
    ) -> Result<Option<Vec<u8>>, Outcome> {
        unsafe {
            if !self.exact(scope, value, b"StringVt")? {
                return Ok(None);
            }
            Ok(Some(self.bytes(value)?))
        }
    }
    pub(super) unsafe fn symbol_bytes(
        &self,
        scope: &Scope<'_>,
        value: Oop,
    ) -> Result<Option<Vec<u8>>, Outcome> {
        unsafe {
            if !self.exact(scope, value, b"SymbolVt")? {
                return Ok(None);
            }
            let bytes = self.send(
                self.id.receiver,
                b"symbolBytesOf:",
                &[Operand::object(value)],
            )?;
            let bytes = bytes.as_bytes().ok_or_else(invalid)?;
            Ok(Some(
                borrowed_bytes(bytes.pointer, bytes.length)
                    .ok_or_else(invalid)?
                    .to_vec(),
            ))
        }
    }
    pub(super) unsafe fn text(&self, scope: &Scope<'_>, bytes: &[u8]) -> Result<Oop, Outcome> {
        unsafe {
            let factory = scope.keep(self.named(b"String")?)?;
            scope.keep(object(self.send(
                factory,
                b"fromBytes:",
                &[Operand::bytes(bytes)],
            )?)?)
        }
    }
    pub(super) unsafe fn vector(&self, scope: &Scope<'_>, values: &[Oop]) -> Result<Oop, Outcome> {
        unsafe {
            let factory = scope.keep(self.named(b"Vector")?)?;
            let mut result = scope.keep(object(self.send(factory, b"new", &[])?)?)?;
            for &value in values {
                result = scope.keep(object(self.send(
                    result,
                    b"add:",
                    &[Operand::object(value)],
                )?)?)?;
            }
            Ok(result)
        }
    }
}
