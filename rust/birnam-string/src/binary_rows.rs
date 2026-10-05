//! Generic tables of tagged tuples. Payload shapes come from the caller.
#[derive(Clone)]
pub struct Field {
    pub tag: u8,
    pub index: Option<u32>,
    pub data: Option<std::ops::Range<usize>>,
    pub offset: usize,
}
fn word(b: &[u8], p: &mut usize) -> Option<u32> {
    let end = p.checked_add(4)?;
    let n = u32::from_le_bytes(b.get(*p..end)?.try_into().ok()?); *p = end; Some(n)
}
fn field(b: &[u8], p: &mut usize, schema: &[u8]) -> Option<Field> {
    let offset = *p; let tag = *b.get(*p)?; *p = p.checked_add(1)?;
    let mut f = Field { tag, index: None, data: None, offset };
    match *schema.get(tag as usize)? {
        b'0' => (),
        b'1' => *p = p.checked_add(8)?,
        b'2' => f.index = Some(word(b, p)?),
        b'3' | b'4' => {
            let length = word(b, p)? as usize; let end = p.checked_add(length)?;
            let data = b.get(*p..end)?;
            if schema[tag as usize] == b'4' && (data.is_empty() || data.contains(&0)) { return None }
            f.data = Some(*p..end); *p = end;
        }
        _ => return None,
    }
    (*p <= b.len()).then_some(f)
}
pub fn rows(bytes: &[u8], schema: &[u8], limit: usize) -> Option<Vec<(Field, Vec<Field>)>> {
    let mut p = 0; let count = word(bytes, &mut p)? as usize;
    if count > limit || count > bytes.len().saturating_sub(p) / 5 { return None }
    let mut result = Vec::with_capacity(count);
    for _ in 0..count {
        let locator = field(bytes, &mut p, schema)?;
        let count = word(bytes, &mut p)? as usize;
        if count > bytes.len().saturating_sub(p) { return None }
        let mut fields = Vec::with_capacity(count);
        for _ in 0..count { fields.push(field(bytes, &mut p, schema)?); }
        result.push((locator, fields));
    }
    (p == bytes.len()).then_some(result)
}
/// Fixed-prefix records containing a caller-selected number of tagged cells.
/// Prefix bytes are opaque. This performs framing, not domain admission.
pub fn records(bytes: &[u8], start: usize, count: usize, prefix: usize, fields: usize, schema: &[u8]) -> Option<Vec<(Field, Vec<Field>)>> {
    let width = prefix.checked_add(fields)?;
    if start > bytes.len() || count > 1_048_576 || width == 0 || count > (bytes.len()-start)/width { return None }
    let mut p = start; let mut rows = Vec::with_capacity(count);
    for _ in 0..count {
        let begin = p; p = p.checked_add(prefix)?;
        bytes.get(begin..p)?;
        let head = Field { tag: 0, index: None, data: Some(begin..p), offset: begin };
        let mut values = Vec::with_capacity(fields);
        for _ in 0..fields { values.push(field(bytes, &mut p, schema)?); }
        rows.push((head, values));
    }
    (p == bytes.len()).then_some(rows)
}

#[cfg(test)]
mod tests {
    use super::records;

    #[test]
    fn fixed_records_preserve_byte_offsets_and_require_exact_framing() {
        let bytes = [99, 8, 9, 2, 7, 0, 0, 0, 4, 5, 0];
        let rows = records(&bytes, 1, 2, 2, 1, b"002").unwrap();
        assert_eq!(rows[0].0.data, Some(1..3));
        assert_eq!(rows[0].1[0].offset, 3);
        assert_eq!(rows[0].1[0].index, Some(7));
        assert_eq!(rows[1].0.offset, 8);
        assert_eq!(rows[1].1[0].offset, 10);
        assert!(records(&bytes, 1, 1, 2, 1, b"002").is_none());
        assert!(records(&bytes[..10], 1, 2, 2, 1, b"002").is_none());
        assert!(records(&bytes, 1, 2, 2, 1, b"00").is_none());
        assert!(records(&bytes, usize::MAX, 2, 2, 1, b"002").is_none());
        assert!(records(&bytes, 1, 2, usize::MAX, 1, b"002").is_none());
    }
}
