//! Alias transport for already-validated canonical closure continuations.
//! This carries provenance only; callers retain all execution authority checks.
use super::ColoredEdge;

pub(super) fn continuation_aliases(aliases: &[bool], edge: &ColoredEdge) -> Option<Vec<bool>> {
    let mut next = aliases.to_vec();
    // Every source comes from the old state, including copy cycles.
    for copy in &edge.copies {
        next[copy.destination as usize] = aliases[copy.source as usize];
    }
    next[edge.result_initialization_slot? as usize] = false;
    for (slot, live) in next.iter_mut().enumerate() {
        if edge
            .clear_bitmap_words
            .get(slot / 64)
            .is_some_and(|bits| bits & (1 << (slot % 64)) != 0)
        {
            *live = false;
        }
    }
    Some(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resumable::{ProgramCounter, SlotCopy};

    #[test]
    fn copies_read_the_old_state_before_result_and_clear_stores() {
        let edge = ColoredEdge {
            source: ProgramCounter(0), target: ProgramCounter(1),
            copies: vec![
                SlotCopy { source: 0, destination: 1 },
                SlotCopy { source: 1, destination: 0 },
                SlotCopy { source: 2, destination: 3 },
            ],
            result_initialization_slot: Some(2),
            clear_bitmap_words: vec![(1 << 2) | (1 << 3)],
        };
        let original = [true, false, true, true];
        assert_eq!(continuation_aliases(&original, &edge), Some(vec![false, true, false, false]));
        assert_eq!(original, [true, false, true, true]);
    }

    #[test]
    fn bitmap_word_boundary_and_result_refusal_are_explicit() {
        let mut edge = ColoredEdge {
            source: ProgramCounter(0), target: ProgramCounter(1), copies: vec![],
            result_initialization_slot: Some(0),
            clear_bitmap_words: vec![1 << 63, 1],
        };
        let mut expected = vec![true; 65];
        expected[0] = false;
        expected[63] = false;
        expected[64] = false;
        assert_eq!(continuation_aliases(&[true; 65], &edge), Some(expected));
        edge.result_initialization_slot = None;
        assert_eq!(continuation_aliases(&[true; 65], &edge), None);
    }
}
