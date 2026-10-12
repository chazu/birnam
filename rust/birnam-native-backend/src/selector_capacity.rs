//! Static storage policy for a private name cache, not a semantic admission rule.
//! Generated names come from C literals. Counting all distinct literal spellings
//! also includes unused helpers and non-name strings, deliberately overestimating
//! ordinary demand. Any underestimate remains safe through full-cache fallback.
use std::collections::HashSet;

pub(super) fn for_source(source: &str) -> usize {
    let bytes = source.as_bytes();
    let mut strings = HashSet::new();
    let mut i = 0;
    while i < bytes.len() {
        match (bytes[i], bytes.get(i + 1).copied()) {
            (b'/', Some(b'/')) => {
                i += 2;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            (b'/', Some(b'*')) => {
                i += 2;
                while i + 1 < bytes.len() && &bytes[i..i + 2] != b"*/" {
                    i += 1;
                }
                if i + 1 >= bytes.len() {
                    return 512;
                }
                i += 2;
            }
            (quote @ (b'"' | b'\''), _) => {
                let start = i;
                i += 1;
                loop {
                    if i >= bytes.len() {
                        return 512;
                    }
                    if bytes[i] == quote {
                        i += 1;
                        break;
                    }
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                if quote == b'"' {
                    strings.insert(&bytes[start..i]);
                    if strings.len() >= 256 {
                        return 512;
                    }
                }
            }
            _ => i += 1,
        }
    }
    (strings.len().max(1) * 2)
        .next_power_of_two()
        .clamp(64, 512)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capacity_is_bounded_and_leaves_room_for_probing() {
        for (names, expected) in [
            (0, 64),
            (32, 64),
            (33, 128),
            (64, 128),
            (65, 256),
            (128, 256),
            (129, 512),
            (600, 512),
        ] {
            let source = (0..names)
                .map(|i| format!("\"name{i}\";\n"))
                .collect::<String>();
            assert_eq!(for_source(&source), expected);
        }
    }
    #[test]
    fn spelling_duplicates_comments_and_escaped_quotes_do_not_inflate_the_estimate() {
        let source = r#"/* "comment" */ char quote='"'; "one\"two"; // "comment"
"one\"two"; '#'; "third";"#;
        assert_eq!(for_source(&source.repeat(400)), 64);
        assert_eq!(for_source("\"unterminated"), 512);
        assert_eq!(for_source("/* unterminated"), 512);
        assert_eq!(for_source("/*"), 512);
    }
}
