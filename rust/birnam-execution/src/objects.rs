//! Weak records of Execution's own native representations. Id alone allocates
//! and reclaims their storage. No dependency sends occur inside the pure engine.
use birnam_abi::Oop;
use std::collections::HashMap;
#[derive(Clone, Copy)]
struct Record {
    behavior: Oop,
    family: Oop,
    words: usize,
}
#[derive(Default)]
pub(crate) struct Objects {
    records: HashMap<usize, Record>,
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
