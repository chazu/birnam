//! Inspect ABI bytes only inside a callback-free interval. Return owned scalars;
//! no borrowed byte/program view survives into the next Id message.
use birnam_artifact_codec::portable_program::{self, Error};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Shape {
    pub parameters: usize,
    pub captures: usize,
}
pub(super) fn inspect(bytes: &[u8], copy_first: bool) -> Result<Shape, Error> {
    let owned;
    let input = if copy_first {
        owned = bytes.to_vec();
        owned.as_slice()
    } else {
        bytes
    };
    let program = portable_program::inspect(input)?;
    Ok(Shape {
        parameters: program.parameter_count as usize,
        captures: program.capture_count as usize,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn borrowed_and_copied_inspection_match_valid_invalid_and_changed_bytes() {
        let fixtures: &[&[u8]] = &[
            include_bytes!("../../../tests/fixtures/portable-code-v1/program-empty.bin"),
            include_bytes!("../../../tests/fixtures/portable-code-v1/program-nested-inner.bin"),
            include_bytes!("../../../tests/fixtures/portable-code-v1/program-nested-captures.bin"),
            include_bytes!("../../../tests/fixtures/portable-code-v1/program-send.bin"),
        ];
        for bytes in fixtures {
            let expected = inspect(bytes, true);
            assert!(expected.is_ok());
            assert_eq!(inspect(bytes, false), expected);
            for offset in (0..bytes.len()).step_by(13) {
                let mut changed = bytes.to_vec();
                changed[offset] ^= 0x80;
                assert_eq!(inspect(&changed, false), inspect(&changed, true));
            }
            for length in (0..bytes.len()).step_by(17) {
                assert_eq!(
                    inspect(&bytes[..length], false),
                    inspect(&bytes[..length], true)
                );
            }
        }
    }
}
