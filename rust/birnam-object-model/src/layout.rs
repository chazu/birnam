//! Exact-behavior slot descriptions, private to ordinary object storage.
//! Absorbed from payload-layout; no factory, carrier, or foreign view survives.
#[derive(Clone)]
pub(super) struct Layout {
    kinds: Box<[u8]>,
    rest: u8,
}
pub(super) const UNKNOWN: u8 = 0;
pub(super) const OOP: u8 = 1;
impl Layout {
    pub(super) fn new(kinds: &[u8], rest: u8) -> Self {
        Self {
            kinds: kinds.into(),
            rest,
        }
    }
    pub(super) fn kind(&self, word: usize) -> u8 {
        self.kinds.get(word).copied().unwrap_or(self.rest)
    }
    pub(super) fn all_oop(&self, words: usize) -> bool {
        self.kinds.iter().take(words).all(|kind| *kind == OOP)
            && (words <= self.kinds.len() || self.rest == OOP)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copied_prefix_exact_extent_and_arbitrary_tail_codes() {
        let mut source = [OOP, OOP, 9, UNKNOWN, 255];
        let owned = Layout::new(&source, OOP);
        source.fill(0);
        assert_eq!(owned.kind(2), 9);
        assert_eq!(owned.kind(usize::MAX), OOP);
        assert!(owned.all_oop(2));
        assert!(!owned.all_oop(3));
        for rest in [0, 1, 2, 9, 255] {
            let empty = Layout::new(&[], rest);
            assert_eq!(empty.kind(usize::MAX), rest);
            assert!(empty.all_oop(0));
            assert_eq!(empty.all_oop(usize::MAX), rest == OOP);
        }
    }
}
