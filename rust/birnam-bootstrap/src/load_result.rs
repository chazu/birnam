//! Typed language carrier for Id descriptor loading; admission stays in Id.
use super::*;

pub(super) unsafe extern "C" fn entry(c: *mut c_void, r: Oop, a: *const Operand, n: usize) -> Outcome {
    outcome((|| unsafe {
        let s = &*c.cast::<State>();
        let [path, arguments] = borrowed_slice(a, n).ok_or_else(invalid)? else { return Err(invalid()); };
        let (path, arguments) = (object(*path)?, object(*arguments)?);
        let mut tokens = Vec::new();
        let keep = |tokens: &mut Vec<usize>, value| -> Result<Oop, Outcome> {
            tokens.push(retain(s.id, s.roots, value)?); Ok(value)
        };
        let named = |name: &[u8]| -> Result<Oop, Outcome> {
            object(s.id.send_named(s.namespace, b"at:", &[Operand::object(s.id.intern_name(name)?)])?)
        };
        let answer = (|| {
            for value in [r, path, arguments] { keep(&mut tokens, value)?; }
            let raw = s.id.send_named(path, b"bytes", &[])?.as_bytes().ok_or_else(invalid)?;
            let bytes = borrowed_bytes(raw.pointer, raw.length).ok_or_else(invalid)?.to_vec();
            let integer = keep(&mut tokens, named(b"Integer")?)?;
            let size = keep(&mut tokens, object(s.id.send_named(arguments, b"size", &[])?)?)?;
            let count = s.id.send_named(integer, b"wordOf:", &[Operand::object(size)])?.as_word().filter(|n| *n <= isize::MAX as usize).ok_or_else(invalid)?;
            let mut request = vec![Operand::bytes(&bytes)];
            for i in 0..count {
                let index = keep(&mut tokens, object(s.id.send_named(integer, b"fromWord:", &[Operand::word(i)])?)?)?;
                let value = keep(&mut tokens, object(s.id.send_named(arguments, b"at:", &[Operand::object(index)])?)?)?;
                request.push(Operand::object(value));
            }
            let result = s.id.send(s.id.receiver, s.id.load, &request);
            if let Ok(value) = &result {
                if let Some(value) = value.as_object().filter(|o| !o.is_null()) { keep(&mut tokens, value)?; }
            }
            let nil = keep(&mut tokens, named(b"nil")?)?;
            let (status, kind, detail, value) = match result {
                Ok(value) => (0u32, value.kind(), 0usize, value.as_object().filter(|o| !o.is_null()).unwrap_or(nil)),
                Err(error) => (error.status, native::UNIT, error.detail, nil),
            };
            keep(&mut tokens, value)?;
            let mut metadata = status.to_le_bytes().to_vec();
            metadata.extend_from_slice(&kind.to_le_bytes());
            metadata.extend_from_slice(&(detail as u64).to_le_bytes());
            let bits = keep(&mut tokens, object(s.id.send_named(named(b"Bytes")?, b"fromBytes:", &[Operand::bytes(&metadata)])?)?)?;
            let mut result = keep(&mut tokens, object(s.id.send_named(named(b"Vector")?, b"new", &[])?)?)?;
            for value in [bits, value] {
                result = keep(&mut tokens, object(s.id.send_named(result, b"add:", &[Operand::object(value)])?)?)?;
            }
            Ok(Operand::object(result))
        })();
        for token in tokens { release(s.id, s.roots, token)?; }
        answer
    })())
}
