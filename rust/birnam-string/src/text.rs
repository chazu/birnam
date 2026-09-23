//! Private text algorithms. Numeric operations return when number objects do.
#[allow(dead_code)]
pub(crate) fn bracket_depth(bytes: &[u8]) -> i64 {
    let mut depth = 0i64;
    let mut in_string = false;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            if byte == b'\\' {
                index = index.saturating_add(2);
                continue;
            }
            if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if byte == b'/' && index + 1 < bytes.len() && bytes[index + 1] == b'/' {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'[' | b'{' => depth += 1,
            b']' | b'}' => depth -= 1,
            _ => {}
        }
        index += 1;
    }
    depth
}
pub(crate) fn replace(bytes: &[u8], needle: &[u8], replacement: &[u8]) -> Vec<u8> {
    if needle.is_empty() {
        return bytes.to_vec();
    }
    let mut output = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if index + needle.len() <= bytes.len() && bytes[index..index + needle.len()] == needle[..] {
            output.extend_from_slice(replacement);
            index += needle.len();
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    output
}

pub(crate) fn trimmed_bytes(bytes: &[u8]) -> &[u8] {
    let space = |byte: &u8| matches!(*byte, b' ' | b'\t' | b'\r' | b'\n' | 0x0b | 0x0c);
    let start = bytes
        .iter()
        .position(|byte| !space(byte))
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !space(byte))
        .map_or(start, |index| index + 1);
    &bytes[start..end]
}
