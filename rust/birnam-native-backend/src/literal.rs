//! Source literal-body decoding. Callers retain their own envelope admission.
//! This is separate from escaping the decoded bytes into generated C.

pub fn decode_body(inner: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::with_capacity(inner.len());
    let mut index = 0;
    while index < inner.len() {
        if inner[index] == b'\\' && index + 1 < inner.len() {
            let escape = match inner[index + 1] {
                b'"' => Some(b'"'),
                b'\\' => Some(b'\\'),
                b'n' => Some(b'\n'),
                b't' => Some(b'\t'),
                b'r' => Some(b'\r'),
                b'0' => Some(0),
                _ => None,
            };
            if let Some(byte) = escape {
                decoded.push(byte);
                index += 2;
                continue;
            }
        }
        decoded.push(inner[index]);
        index += 1;
    }
    decoded
}
