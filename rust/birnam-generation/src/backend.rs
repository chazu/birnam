//! Current Id calls to the complete native-emission owner. Rust request records
//! remain frontend-local; no typed IR or compiler state crosses this interface.
use super::*;
use birnam_codegen::backend_request::Request;
#[derive(Debug)]
pub(super) enum Failure {
    Native(super::Failure),
    Rejected,
}
impl From<super::Failure> for Failure {
    fn from(e: super::Failure) -> Self {
        Self::Native(e)
    }
}
unsafe fn checked(s: &State, q: &Scope<'_>, name: &[u8], args: &[Oop]) -> Result<Vec<u8>, Failure> {
    unsafe {
        let backend = q.keep(s.named(b"NativeBackend")?)?;
        let value = q.keep(object(s.send(backend, name, args)?)?)?;
        if s.is_error(value)? {
            return Err(Failure::Rejected);
        }
        Ok(s.bytes(value)?)
    }
}
pub(super) unsafe fn emit(s: &State, q: &Scope<'_>, request: &Request) -> Result<Vec<u8>, Failure> {
    unsafe {
        let text = |b: &[u8]| s.text(q, b);
        let flag = |v| s.named(if v { b"true" } else { b"false" });
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
                    &[word(*arity as usize)?],
                )?)?)?;
                checked(s, q, b"named:synchronous:", &[a, flag(*synchronous)?])
            }
            Request::Factory {
                program,
                source,
                dispositions,
            }
            | Request::ModuleFactory {
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
                    if matches!(request, Request::ModuleFactory { .. }) {
                        b"moduleFactory:source:dispositions:"
                    } else {
                        b"factory:source:dispositions:"
                    },
                    &[text(program)?, text(source)?, v],
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
        for request in requests
            .iter()
            .filter(|r| !matches!(r, Request::Named { .. }))
        {
            let c = emit(s, q, request)?;
            total = total.checked_add(c.len()).ok_or_else(invalid)?;
            if total > 128 << 20 {
                return Err(Failure::Rejected);
            }
            output.push(c);
        }
        Ok(output)
    }
}
pub(super) unsafe fn descriptor(s: &State, q: &Scope<'_>, name: &[u8]) -> Result<Vec<u8>, Failure> {
    unsafe {
        let name = s.text(q, name)?;
        let codegen = q.keep(s.named(b"Codegen")?)?;
        let value = q.keep(object(s.send(
            codegen,
            b"methodDescriptorFor:",
            &[name],
        )?)?)?;
        if s.is_error(value)? {
            return Err(Failure::Rejected);
        }
        Ok(s.bytes(value)?)
    }
}
pub(super) unsafe fn hashes<'a>(
    s: &State,
    q: &Scope<'_>,
    names: impl Iterator<Item = &'a [u8]>,
) -> Result<std::collections::HashMap<Vec<u8>, Option<[u8; 32]>>, super::Failure> {
    unsafe {
        let mut output = std::collections::HashMap::new();
        for name in names {
            if output.contains_key(name) {
                continue;
            }
            let hash = match descriptor(s, q, name) {
                Ok(bytes) => Some(birnam_codegen::generation::sha256_raw(&bytes)),
                Err(Failure::Rejected) => None,
                Err(Failure::Native(e)) => return Err(e),
            };
            output.insert(name.to_vec(), hash);
        }
        Ok(output)
    }
}
