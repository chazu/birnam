//! Shared scalar mechanics; format policy stays with each decoder.
//! Checked and unchecked offset arithmetic intentionally remain distinct.
//! Independent fixture readers must not import this module.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_bytes_and_truncation_are_unchanged() {
        let mut bytes = vec![0xaa];
        push_u32(&mut bytes, 0x12345678);
        push_u64(&mut bytes, 0x0123456789abcdef);
        assert_eq!(
            bytes,
            [
                0xaa, 0x78, 0x56, 0x34, 0x12, 0xef, 0xcd, 0xab, 0x89, 0x67, 0x45, 0x23, 0x01
            ]
        );
        assert_eq!(read_u32(&bytes, 1), Some(0x12345678));
        assert_eq!(read_u64(&bytes, 5), Some(0x0123456789abcdef));
        for length in 0..=bytes.len() {
            for offset in 0..=bytes.len() {
                assert_eq!(
                    read_u32(&bytes[..length], offset),
                    read_u32_checked(&bytes[..length], offset)
                );
                assert_eq!(
                    read_u64(&bytes[..length], offset),
                    read_u64_checked(&bytes[..length], offset)
                );
            }
        }
        assert_eq!(read_u32_checked(&bytes, usize::MAX), None);
        assert_eq!(read_u64_checked(&bytes, usize::MAX), None);
    }
}

#[inline]
pub(super) fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

#[inline]
pub(super) fn push_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

#[inline]
pub(super) fn read_u32_checked(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

#[inline]
pub(super) fn read_u64_checked(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset.checked_add(8)?)?.try_into().ok()?,
    ))
}

#[inline]
pub(super) fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

#[inline]
pub(super) fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}
