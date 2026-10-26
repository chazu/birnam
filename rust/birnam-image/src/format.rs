//! Value-only requests to ImageFormat. Full-format validation/encoding never
//! calls the pure codec from the live restoration owner.
use super::*;
pub(super) struct Decoded {
    pub programs: Vec<u8>,
    pub bindings: Vec<u8>,
    pub selection: Vec<u8>,
    pub heaps: Vec<Vec<u8>>,
    pub facts: Vec<Vec<u8>>,
    pub code: Option<Oop>,
    pub sources: Option<Vec<u8>>,
    pub behaviors: Option<Vec<u8>>,
    pub providers: Option<Vec<u8>>,
}
impl Decoded {
    pub fn view(&self) -> v8::DecodedImage<'_> {
        v8::DecodedImage {
            programs: &self.programs,
            bindings: &self.bindings,
            selection: &self.selection,
            heaps: self.heaps.iter().map(Vec::as_slice).collect(),
            facts: self.facts.iter().map(Vec::as_slice).collect(),
            code: None,
            sources: self.sources.as_deref(),
            behaviors: self.behaviors.as_deref(),
            providers: self.providers.as_deref(),
        }
    }
}
unsafe fn optional(s: &State, q: &Scope<'_>, v: Oop) -> Result<Option<Vec<u8>>, Failure> {
    unsafe {
        if v == s.named(b"nil")? {
            Ok(None)
        } else {
            s.bytes(q.keep(v)?).map(Some)
        }
    }
}
unsafe fn admitted(s: &State, q: &Scope<'_>, answer: Oop) -> Result<Oop, Failure> {
    unsafe {
        let error = s.call(q, answer, b"isError", &[])?;
        if error == s.named(b"true")? {
            let kind = s.call(q, answer, b"kind", &[])?;
            if s.symbol(kind)? == b"imageRejected" {
                return Err(bad());
            }
            return Err(Failure::Value(answer));
        }
        Ok(answer)
    }
}
pub(super) unsafe fn inspect(s: &State, bytes: &[u8]) -> Result<Decoded, Failure> {
    unsafe {
        // Metadata is copied; native code stays an owner-provided Bytes Oop.
        // The caller must retain that Oop after this temporary scope ends.
        let scope = Scope::new(s);
        let q = &scope;
        let owner = s.call(
            q,
            s.named(b"Globals")?,
            b"at:",
            &[q.keep(s.intern(b"ImageFormat")?)?],
        )?;
        let value = s.blob(q, bytes)?;
        let arch = s.integer(q, support::IMAGE_ARCH_TAG as usize)?;
        let abi = s.integer(q, runtime_abi() as usize)?;
        let answer = s.call(q, owner, b"inspect:architecture:abi:", &[value, arch, abi])?;
        let answer = admitted(s, q, answer)?;
        let fields = s.sequence(q, answer)?;
        let [
            programs,
            bindings,
            selection,
            heaps,
            facts,
            code,
            sources,
            behaviors,
            providers,
        ] = fields.as_slice()
        else {
            return Err(bad());
        };
        let blobs = |v| -> Result<Vec<Vec<u8>>, Failure> {
            s.sequence(q, v)?.into_iter().map(|v| s.bytes(v)).collect()
        };
        let code = if *code == s.named(b"nil")? {
            None
        } else {
            let buffer = s.call(q, *code, b"bytes", &[])?;
            let code = s.call(q, buffer, b"clone", &[])?;
            if abi::as_size(s.send(code, b"sizeWord", &[])?)
                .ok_or_else(bad)?
                .checked_sub(72)
                .is_none()
            {
                return Err(bad());
            }
            Some(code)
        };
        Ok(Decoded {
            programs: s.bytes(*programs)?,
            bindings: s.bytes(*bindings)?,
            selection: s.bytes(*selection)?,
            heaps: blobs(*heaps)?,
            facts: blobs(*facts)?,
            code,
            sources: optional(s, q, *sources)?,
            behaviors: optional(s, q, *behaviors)?,
            providers: optional(s, q, *providers)?,
        })
    }
}
pub(super) unsafe fn compose(
    s: &State,
    q: &Scope<'_>,
    spec: v8::ImageSpec<'_>,
) -> Result<Vec<u8>, Failure> {
    unsafe {
        let owner = s.call(
            q,
            s.named(b"Globals")?,
            b"at:",
            &[q.keep(s.intern(b"ImageFormat")?)?],
        )?;
        let arch = s.integer(q, spec.architecture as usize)?;
        let abi = s.integer(q, spec.runtime_abi as usize)?;
        let blob = |b: &[u8]| s.blob(q, b);
        let opt = |b: Option<&[u8]>| -> Result<Oop, Failure> {
            if let Some(b) = b {
                blob(b)
            } else {
                s.named(b"nil")
            }
        };
        let blobs = |parts: &[&[u8]]| -> Result<Oop, Failure> {
            let values = parts
                .iter()
                .map(|b| blob(b))
                .collect::<Result<Vec<_>, _>>()?;
            s.vector(q, &values)
        };
        let values = [
            arch,
            abi,
            blob(spec.programs)?,
            blob(spec.bindings)?,
            blob(spec.selection)?,
            blobs(spec.heaps)?,
            blobs(spec.facts)?,
            opt(spec.code)?,
            opt(spec.sources)?,
            opt(spec.behaviors)?,
            opt(spec.providers)?,
        ];
        let input = s.vector(q, &values)?;
        let result = s.call(q, owner, b"compose:", &[input])?;
        s.bytes(admitted(s, q, result)?)
    }
}
