//! Unsigned machine operations. Signed language arithmetic belongs to Integer.
pub(crate) fn calculate(left: u64, right: u64, op: usize) -> Option<u64> {
    Some(match op {
        6 => left.wrapping_add(right),
        7 => left.wrapping_sub(right),
        8 => left.wrapping_mul(right),
        9 => left.checked_div(right)?,
        10 => left.checked_rem(right)?,
        11 => left & right,
        12 => left | right,
        13 => left ^ right,
        // Counts outside the word width produce zero; no host shift masking.
        14 => {
            if right < 64 {
                left << right
            } else {
                0
            }
        }
        15 => {
            if right < 64 {
                left >> right
            } else {
                0
            }
        }
        _ => return None,
    })
}
#[cfg(test)]
mod tests {
    use super::calculate;
    #[test]
    fn overflow_and_invalid_division_have_machine_semantics() {
        assert_eq!(calculate(u64::MAX, 1, 6), Some(0));
        assert_eq!(calculate(0, 1, 7), Some(u64::MAX));
        assert_eq!(calculate(u64::MAX, 2, 8), Some(u64::MAX - 1));
        assert_eq!(calculate(u64::MAX, 2, 9), Some(u64::MAX / 2));
        assert_eq!(calculate(1, 0, 9), None);
        assert_eq!(calculate(1, 0, 10), None);
        assert_eq!(calculate(1, 63, 14), Some(1 << 63));
        assert_eq!(calculate(1, 64, 14), Some(0));
        assert_eq!(calculate(u64::MAX, u64::MAX, 15), Some(0));
    }
}
