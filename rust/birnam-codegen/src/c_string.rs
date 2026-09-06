//! Shared byte policy for C string bodies; appends to the caller's buffer.

#[inline]
pub(super) fn push_c_string_body(output: &mut Vec<u8>, bytes: &[u8]) {
    for &byte in bytes {
        match byte {
            b'"' => output.extend_from_slice(br#"\""#),
            b'\\' => output.extend_from_slice(br#"\\"#),
            b'\n' => output.extend_from_slice(br#"\n"#),
            b'\r' => output.extend_from_slice(br#"\r"#),
            b'\t' => output.extend_from_slice(br#"\t"#),
            0x20..=0x7e => output.push(byte),
            // Fixed-width octal prevents the following byte from extending
            // the escape (unlike greedy C hexadecimal escapes).
            other => output.extend_from_slice(&[
                b'\\',
                b'0' + ((other >> 6) & 7),
                b'0' + ((other >> 3) & 7),
                b'0' + (other & 7),
            ]),
        }
    }
}
