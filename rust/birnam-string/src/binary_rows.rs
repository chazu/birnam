//! Generic tables of tagged tuples. Tag payload shapes are supplied by the caller.
//! This decoder has no knowledge of image headers, authority or heap semantics.
#[derive(Clone)]
pub struct Field {
    pub tag: u8,
    pub index: Option<u32>,
    pub data: Option<std::ops::Range<usize>>,
}
pub fn rows(bytes: &[u8], schema: &[u8], limit: usize) -> Option<Vec<(Field, Vec<Field>)>> {
    fn word(b: &[u8], p: &mut usize) -> Option<u32> {
        let end = p.checked_add(4)?;
        let n = u32::from_le_bytes(b.get(*p..end)?.try_into().ok()?); *p = end; Some(n)
    }
    fn field(b: &[u8], p: &mut usize, schema: &[u8]) -> Option<Field> {
        let tag = *b.get(*p)?; *p = p.checked_add(1)?;
        let mut f = Field { tag, index: None, data: None };
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
