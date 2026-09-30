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

#[cfg(test)]
mod tests {
    #[test]
    fn c_strings_preserve_every_byte_and_adjacent_pair() {
        // Independent spelling oracle: formatted octal rather than the emitter's
        // shifts. Every pair includes octal/hex digits after escaped bytes.
        fn expected(bytes: &[u8]) -> Vec<u8> {
            let mut result = String::new();
            for &byte in bytes {
                match byte {
                    b'"' => result.push_str("\\\""),
                    b'\\' => result.push_str("\\\\"),
                    b'\n' => result.push_str("\\n"),
                    b'\r' => result.push_str("\\r"),
                    b'\t' => result.push_str("\\t"),
                    32..=126 => result.push(char::from(byte)),
                    _ => result.push_str(&format!("\\{byte:03o}")),
                }
            }
            result.into_bytes()
        }
        fn check(bytes: &[u8]) {
            let body = expected(bytes);
            let mut output = b"prefix:".to_vec();
            super::push_c_string_body(&mut output, bytes);
            assert_eq!(output, [b"prefix:".as_slice(), &body].concat());
        }
        check(&[]);
        check(&(0..=255).collect::<Vec<u8>>());
        for first in 0..=255 {
            check(&[first]);
            for second in 0..=255 {
                check(&[first, second]);
            }
        }
    }
}
