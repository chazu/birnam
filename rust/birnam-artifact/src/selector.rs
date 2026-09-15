//! Unchecked, allocation-free selector byte classification.
//! Callers own name admission and quantity conversion; checked format readers
//! deliberately retain their independent validation.

pub fn arity(selector: &[u8]) -> usize {
    let colons = selector.iter().filter(|byte| **byte == b':').count();
    if colons != 0 {
        return colons;
    }
    match selector.first() {
        None => 0,
        Some(byte) if byte.is_ascii_alphabetic() || *byte == b'_' => 0,
        Some(_) => 1,
    }
}
