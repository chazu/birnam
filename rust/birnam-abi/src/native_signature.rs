//! Explicit native calling-convention metadata. No object or dispatch authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    bytes: Vec<u8>,
}
impl Signature {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let (&result, arguments) = bytes.split_first()?;
        if arguments.len() > 128
            || !matches!(
                result,
                b'o' | b'w' | b'b' | b'u' | b'x' | b'W' | b'B' | b'v'
            )
            || arguments
                .iter()
                .any(|k| !matches!(k, b'o' | b'O' | b'w' | b'b'))
        {
            return None;
        }
        Some(Self {
            bytes: bytes.to_vec(),
        })
    }
    pub fn result(&self) -> u8 {
        self.bytes[0]
    }
    pub fn arguments(&self) -> &[u8] {
        &self.bytes[1..]
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
#[cfg(test)]
mod tests {
    use super::Signature;
    #[test]
    fn signatures_are_explicit_and_bounded() {
        assert!(Signature::parse(b"owb").is_some());
        assert!(Signature::parse(b"Wo").is_some());
        assert!(Signature::parse(b"vOO").is_some());
        assert!(Signature::parse(b"O").is_none());
        for invalid in [b"".as_slice(), b"q", b"ou", b"wx"] {
            assert!(Signature::parse(invalid).is_none());
        }
        assert!(Signature::parse(&[b'o'; 129]).is_some());
        assert!(Signature::parse(&[b'o'; 130]).is_none());
    }
}
