//! Bounded copied-value conversion. No Oop escapes the owning thread or scope.
use super::*;
#[derive(Debug)]
pub(super) enum TransferError {
    Native(Outcome),
    Codec(CodecError),
}
impl From<Outcome> for TransferError {
    fn from(e: Outcome) -> Self {
        Self::Native(e)
    }
}
impl From<CodecError> for TransferError {
    fn from(e: CodecError) -> Self {
        Self::Codec(e)
    }
}
struct Capture<'a, 'b> {
    q: &'a Scope<'b>,
    active: Vec<usize>,
    nodes: usize,
    bytes: usize,
}
impl Capture<'_, '_> {
    fn reserve(&mut self, n: usize) -> Result<(), TransferError> {
        self.bytes = self.bytes.checked_add(n).ok_or(CodecError::FrameTooLarge)?;
        if self.bytes > MAX_FRAME {
            Err(CodecError::FrameTooLarge.into())
        } else {
            Ok(())
        }
    }
    unsafe fn size(&self, o: Oop) -> Result<usize, TransferError> {
        unsafe {
            let n = self.q.obj(o, b"size", &[])?;
            let n = self.q.word(n)?.ok_or(CodecError::NonTransferable)?;
            if n > MAX_CONTAINER {
                return Err(CodecError::ContainerTooLarge.into());
            }
            Ok(n)
        }
    }
    unsafe fn at(&self, o: Oop, i: usize) -> Result<Oop, TransferError> {
        unsafe {
            let i = self.q.int(i)?;
            Ok(self.q.obj(o, b"at:", &[Operand::object(i)])?)
        }
    }
    unsafe fn value(&mut self, o: Oop, depth: usize) -> Result<Value, TransferError> {
        unsafe {
            if depth > MAX_DEPTH {
                return Err(CodecError::TooDeep.into());
            }
            if self.nodes == MAX_NODES {
                return Err(CodecError::TooManyNodes.into());
            }
            self.nodes += 1;
            self.reserve(1)?;
            let q = self.q;
            q.keep(o)?;
            for (n, v) in [
                (b"nil".as_slice(), Value::Nil),
                (b"true", Value::True),
                (b"false", Value::False),
            ] {
                if o == q.named(n)? {
                    return Ok(v);
                }
            }
            let vt = q.obj(q.s.id.receiver, b"behaviorOf:", &[Operand::object(o)])?;
            if vt == q.named(b"IntegerVt")? {
                self.reserve(8)?;
                return Ok(Value::Int(
                    q.word(o)?.ok_or(CodecError::NonTransferable)? as i64
                ));
            }
            if vt == q.named(b"FloatVt")? {
                self.reserve(8)?;
                let bits = q
                    .call(q.named(b"Float")?, b"bitsOf:", &[Operand::object(o)])?
                    .as_word()
                    .ok_or(CodecError::NonTransferable)?;
                return Ok(Value::Float(bits as u64));
            }
            if vt == q.named(b"StringVt")? {
                let b = q.bytes(o)?;
                self.reserve(4 + b.len())?;
                return Ok(Value::String(b));
            }
            let name = q.call(q.s.id.receiver, b"symbolBytesOf:", &[Operand::object(o)])?;
            if let Some(b) = name.as_bytes() {
                let b = borrowed_bytes(b.pointer, b.length)
                    .ok_or(CodecError::InvalidRecord)?
                    .to_vec();
                self.reserve(4 + b.len())?;
                return Ok(Value::Symbol(b));
            }
            let vector = vt == q.named(b"VectorVt")?;
            let map = vt == q.named(b"MapVt")?;
            if !vector && !map {
                return Err(CodecError::NonTransferable.into());
            }
            if self.active.contains(&o.addr()) {
                return Err(CodecError::CyclicValue.into());
            }
            self.active.push(o.addr());
            let size = self.size(o)?;
            self.reserve(4)?;
            let v = if vector {
                let mut values = Vec::with_capacity(size);
                for i in 0..size {
                    let item = self.at(o, i)?;
                    values.push(self.value(item, depth + 1)?);
                }
                Value::Vector(values)
            } else {
                let keys = q.obj(o, b"keys", &[])?;
                if self.size(keys)? != size {
                    return Err(CodecError::InvalidRecord.into());
                }
                let mut values = Vec::with_capacity(size);
                for i in 0..size {
                    let key = self.at(keys, i)?;
                    let k = self.value(key, depth + 1)?;
                    if !valid_map_key(&k) {
                        return Err(CodecError::InvalidMapKey.into());
                    }
                    let value = q.obj(o, b"at:", &[Operand::object(key)])?;
                    values.push((k, self.value(value, depth + 1)?));
                }
                Value::Map(values)
            };
            self.active.pop();
            Ok(v)
        }
    }
}
pub(super) unsafe fn capture(q: &Scope<'_>, o: Oop) -> Result<Value, TransferError> {
    unsafe {
        Capture {
            q,
            active: Vec::new(),
            nodes: 0,
            bytes: 0,
        }
        .value(o, 0)
    }
}
pub(super) unsafe fn materialize(q: &Scope<'_>, v: &Value) -> Result<Oop, TransferError> {
    unsafe {
        Ok(match v {
            Value::Nil => q.named(b"nil")?,
            Value::True => q.named(b"true")?,
            Value::False => q.named(b"false")?,
            Value::Int(i) => q.int(*i as usize)?,
            Value::Float(b) => q.obj(
                q.named(b"Float")?,
                b"fromBits:",
                &[Operand::word(*b as usize)],
            )?,
            Value::String(b) => q.string(b)?,
            Value::Symbol(b) => q.symbol(b)?,
            Value::Vector(values) => {
                let mut out = q.obj(q.named(b"Vector")?, b"new", &[])?;
                for v in values {
                    let v = materialize(q, v)?;
                    out = q.obj(out, b"add:", &[Operand::object(v)])?;
                }
                out
            }
            Value::Map(values) => {
                let mut out = q.obj(q.named(b"Map")?, b"new", &[])?;
                for (k, v) in values {
                    let k = materialize(q, k)?;
                    if q.call(out, b"includesKey:", &[Operand::object(k)])?
                        .as_object()
                        == Some(q.named(b"true")?)
                    {
                        return Err(CodecError::DuplicateMapKey.into());
                    }
                    let v = materialize(q, v)?;
                    out = q.obj(out, b"at:put:", &[Operand::object(k), Operand::object(v)])?;
                }
                out
            }
        })
    }
}
