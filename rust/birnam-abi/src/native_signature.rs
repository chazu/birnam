//! Explicit native calling-convention metadata. No object or dispatch authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    bytes: Vec<u8>,
}
impl Signature {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let body = bytes.strip_prefix(b"!").unwrap_or(bytes);
        let (&result, arguments) = body.split_first()?;
        if arguments.len() > 128
            || !matches!(
                result,
                b'o' | b'w' | b'b' | b'u' | b'x' | b'W' | b'B' | b'v' | b'm'
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
        self.bytes[usize::from(self.observes_failure())]
    }
    pub fn arguments(&self) -> &[u8] {
        &self.bytes[1 + usize::from(self.observes_failure())..]
    }
    pub fn observes_failure(&self) -> bool {
        self.bytes.first() == Some(&b'!')
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
        let guarded = Signature::parse(b"!uOww").unwrap();
        assert_eq!(guarded.result(), b'u');
        assert_eq!(guarded.arguments(), b"Oww");
        assert!(guarded.observes_failure());
        assert_eq!(guarded.bytes(), b"!uOww");
        assert!(Signature::parse(b"m").is_some());
        assert!(Signature::parse(b"O").is_none());
        for invalid in [b"".as_slice(), b"q", b"ou", b"wx", b"!", b"!!u", b"!uo!"] {
            assert!(Signature::parse(invalid).is_none());
        }
        assert!(Signature::parse(&[b'o'; 129]).is_some());
        assert!(Signature::parse(&[b'o'; 130]).is_none());
    }
}
