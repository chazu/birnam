//! Weak records of Execution's own native representations. Id alone allocates
//! and reclaims their storage. No dependency sends occur inside the pure engine.
use birnam_abi::Oop;
use super::word_index::WordIndex;
#[derive(Clone, Copy)]
struct Record {
    behavior: Oop,
    family: Oop,
    words: usize,
}
#[derive(Default)]
pub(crate) struct Objects {
    records: WordIndex<Record>,
}
impl Objects {
    pub(crate) fn insert(&mut self, object: Oop, behavior: Oop, words: usize) {
        self.records.insert(
            object.addr(),
            Record {
                behavior,
                family: behavior,
                words,
            },
        );
    }
    pub(crate) fn remove(&mut self, object: Oop) {
        self.records.remove(&object.addr());
    }
    pub(crate) fn family_of(&self, object: Oop) -> Option<Oop> {
        self.records.get(&object.addr()).map(|r| r.family)
    }
    pub(crate) fn set_family(&mut self, object: Oop, family: Oop) {
        self.records.get_mut(&object.addr()).unwrap().family = family;
    }
    pub(crate) fn refresh_behavior(&mut self, object: Oop, behavior: Oop) {
        self.records.get_mut(&object.addr()).unwrap().behavior = behavior;
    }
    pub(crate) fn vtable_of(&self, object: Oop) -> Option<Oop> {
        self.records.get(&object.addr()).map(|r| r.behavior)
    }
    pub(crate) fn payload_size(&self, object: Oop) -> Option<usize> {
        self.records
            .get(&object.addr())
            .map(|r| r.words * size_of::<usize>())
    }
    pub(crate) fn read_payload_word(&self, object: Oop, index: usize) -> Option<usize> {
        (index < self.records.get(&object.addr())?.words)
            .then(|| unsafe { object.cast::<usize>().add(index).read() })
    }
    pub(crate) fn write_payload_word(&mut self, object: Oop, index: usize, word: usize) -> bool {
        if !self
            .records
            .get(&object.addr())
            .is_some_and(|r| index < r.words)
        {
            return false;
        }
        unsafe {
            object.cast::<usize>().add(index).write(word);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ptr;
    use std::collections::BTreeMap;

    #[test]
    fn private_records_match_exact_addresses_through_growth_retirement_and_reuse() {
        let mut objects = Objects::default();
        let mut reference = BTreeMap::new();
        // Aligned, adjacent, distant and maximal addresses; never dereferenced.
        let keys: Vec<_> = (1usize..=4096)
            .map(|i| (i << 12) | (1usize << (usize::BITS - 2)))
            .chain([1, 8, 16, usize::MAX - 7, usize::MAX])
            .collect();
        for (i, &key) in keys.iter().enumerate() {
            let behavior = ptr::without_provenance_mut(i + 1);
            let family = ptr::without_provenance_mut(i + 5000);
            let object = ptr::without_provenance_mut(key);
            objects.insert(object, behavior, i % 19);
            objects.set_family(object, family);
            reference.insert(key, (behavior, family, i % 19));
        }
        for round in 0..3 {
            for &key in keys.iter().step_by(3) {
                objects.remove(ptr::without_provenance_mut(key));
                reference.remove(&key);
            }
            for &key in &keys {
                let object = ptr::without_provenance_mut(key);
                assert_eq!(objects.vtable_of(object), reference.get(&key).map(|x| x.0));
                assert_eq!(objects.family_of(object), reference.get(&key).map(|x| x.1));
                assert_eq!(objects.payload_size(object), reference.get(&key).map(|x| x.2 * size_of::<usize>()));
            }
            for &key in keys.iter().step_by(3) {
                let object = ptr::without_provenance_mut(key);
                let behavior = ptr::without_provenance_mut(round + 7000);
                let changed = ptr::without_provenance_mut(round + 8000);
                objects.insert(object, behavior, round + 1);
                objects.refresh_behavior(object, changed);
                // Reusing an address creates a fresh family/extent, not a stale record.
                reference.insert(key, (changed, behavior, round + 1));
            }
        }
        for (key, (behavior, family, words)) in reference {
            let object = ptr::without_provenance_mut(key);
            assert_eq!(objects.vtable_of(object), Some(behavior));
            assert_eq!(objects.family_of(object), Some(family));
            assert_eq!(objects.payload_size(object), Some(words * size_of::<usize>()));
        }
    }

    #[test]
    fn payload_access_requires_current_membership_and_valid_extent() {
        let mut words = [11usize, 22];
        let object = words.as_mut_ptr().cast();
        let mut objects = Objects::default();
        assert_eq!(objects.read_payload_word(object, 0), None);
        assert!(!objects.write_payload_word(object, 0, 33));
        objects.insert(object, ptr::without_provenance_mut(8), words.len());
        assert_eq!(objects.read_payload_word(object, 1), Some(22));
        assert!(objects.write_payload_word(object, 0, 33));
        assert_eq!(words, [33, 22]);
        assert_eq!(objects.read_payload_word(object, 2), None);
        assert!(!objects.write_payload_word(object, usize::MAX, 99));
        objects.remove(object);
        assert_eq!(objects.read_payload_word(object, 0), None);
        assert!(!objects.write_payload_word(object, 0, 99));
        objects.insert(object, ptr::without_provenance_mut(16), 1);
        assert_eq!(objects.read_payload_word(object, 0), Some(33));
        assert_eq!(objects.read_payload_word(object, 1), None);
    }
}
