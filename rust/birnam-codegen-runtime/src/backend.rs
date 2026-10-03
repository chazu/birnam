//! Current Id calls to the complete native-emission owner. Rust request records
//! remain frontend-local; no typed IR or compiler state crosses this interface.
use super::*;
use birnam_codegen::backend_request::Request;
#[derive(Debug)]
pub(super) enum Failure {
    Native(Outcome),
    Rejected(Vec<u8>),
}
impl Failure {
    pub fn outcome(self) -> Outcome {
        match self {
            Self::Native(e) => e,
            Self::Rejected(_) => invalid(),
        }
    }
}
impl From<Outcome> for Failure {
    fn from(e: Outcome) -> Self {
        Self::Native(e)
    }
}
unsafe fn checked(
    s: &State,
    q: &Scope<'_>,
    name: &[u8],
    args: &[Operand],
) -> Result<Vec<u8>, Failure> {
    unsafe {
        let backend = q.keep(s.named(b"NativeBackend")?)?;
        let value = q.keep(object(s.send(backend, name, args)?)?)?;
        if s.is_error(value)? {
            let message = q.keep(object(s.send(value, b"message", &[])?)?)?;
            return Err(Failure::Rejected(s.bytes(message)?));
        }
        Ok(s.bytes(value)?)
    }
}
pub(super) unsafe fn emit(s: &State, q: &Scope<'_>, request: &Request) -> Result<Vec<u8>, Failure> {
    unsafe {
        let text = |b: &[u8]| s.text(q, b).map(Operand::object);
        let flag = |v| {
            s.named(if v { b"true" } else { b"false" })
                .map(Operand::object)
        };
        match request {
            Request::Program {
                program,
                source,
                filename,
                persistent,
                synchronous,
            } => checked(
                s,
                q,
                b"emit:source:filename:persistent:synchronous:",
                &[
                    text(program)?,
                    text(source)?,
                    text(filename)?,
                    flag(*persistent)?,
                    flag(*synchronous)?,
                ],
            ),
            Request::Named { arity, synchronous } => {
                let a = q.keep(object(s.send(
                    s.named(b"Integer")?,
                    b"fromWord:",
                    &[Operand::word(*arity as usize)],
                )?)?)?;
                checked(
                    s,
                    q,
                    b"named:synchronous:",
                    &[Operand::object(a), flag(*synchronous)?],
                )
            }
            Request::Factory {
                program,
                source,
                dispositions,
            } => {
                let mut v = q.keep(object(s.send(s.named(b"Vector")?, b"new", &[])?)?)?;
                for d in dispositions {
                    v = q.keep(object(s.send(v, b"add:", &[flag(*d)?])?)?)?;
                }
                checked(
                    s,
                    q,
                    b"factory:source:dispositions:",
                    &[text(program)?, text(source)?, Operand::object(v)],
                )
            }
        }
    }
}
pub(super) unsafe fn all(
    s: &State,
    q: &Scope<'_>,
    requests: &[Request],
) -> Result<Vec<Vec<u8>>, Failure> {
    unsafe {
        let mut total = 0usize;
        let mut output = vec![];
        for request in requests {
            let c = emit(s, q, request)?;
            total = total.checked_add(c.len()).ok_or_else(invalid)?;
            if total > 128 << 20 {
                return Err(Failure::Rejected(
                    b"generated source exceeds bundle bound".to_vec(),
                ));
            }
            output.push(c);
        }
        Ok(output)
    }
}
pub(super) unsafe fn descriptor(s: &State, q: &Scope<'_>, name: &[u8]) -> Result<Vec<u8>, Failure> {
    unsafe {
        let name = s.text(q, name)?;
        checked(s, q, b"descriptorFor:", &[Operand::object(name)])
    }
}
pub(super) unsafe fn hashes<'a>(
    s: &State,
    q: &Scope<'_>,
    names: impl Iterator<Item = &'a [u8]>,
) -> Result<std::collections::HashMap<Vec<u8>, Option<[u8; 32]>>, Outcome> {
    unsafe {
        let mut output = std::collections::HashMap::new();
        for name in names {
            if output.contains_key(name) {
                continue;
            }
            let hash = match descriptor(s, q, name) {
                Ok(bytes) => Some(birnam_codegen::generation::sha256_raw(&bytes)),
                Err(Failure::Rejected(_)) => None,
                Err(Failure::Native(e)) => return Err(e),
            };
            output.insert(name.to_vec(), hash);
        }
        Ok(output)
    }
}
