//! Ordered import/export namespace and reverse-name lookup.

use super::index::AddressMap;
use birnam_abi::Oop;
use birnam_abi::namespace::BindingV1 as Export;

pub(crate) struct Environment {
    ordered: Vec<Export>,
    // Integer identity keys add no roots. ordered remains the sole namespace,
    // enumeration and reverse-alias authority.
    by_name: AddressMap<usize>,
}

impl Environment {
    pub(crate) fn new() -> Self {
        Self {
            ordered: Vec::new(),
            by_name: AddressMap::default(),
        }
    }

    pub(crate) fn export(&mut self, name: Oop, value: Oop) -> Oop {
        if let Some(index) = self.by_name.get(&name.addr()).copied() {
            self.ordered[index].value = value;
            return value;
        }
        self.by_name.insert(name.addr(), self.ordered.len());
        self.ordered.push(Export { name, value });
        value
    }

    pub(crate) fn import(&self, name: Oop) -> Oop {
        self.by_name
            .get(&name.addr())
            .map_or(core::ptr::null_mut(), |index| self.ordered[*index].value)
    }

    pub(crate) fn exported_name_of(&self, value: Oop) -> Oop {
        if value.is_null() {
            return core::ptr::null_mut();
        }
        self.ordered
            .iter()
            .find(|entry| entry.value == value)
            .map_or(core::ptr::null_mut(), |entry| entry.name)
    }

    /// Snapshot both halves of the process export authority. Names remain live
    /// because import and reverse lookup depend on their Symbol identity;
    /// values remain live because the namespace publishes them.
    #[cfg(test)]
    pub(crate) fn roots(&self) -> Vec<Oop> {
        self.ordered
            .iter()
            .flat_map(|entry| [entry.name, entry.value])
            .collect()
    }

    pub(crate) fn bindings(&self) -> birnam_abi::namespace::ViewV1 {
        birnam_abi::namespace::ViewV1 {
            bindings: self.ordered.as_ptr(),
            count: self.ordered.len(),
        }
    }

    pub(crate) fn count(&self) -> usize {
        self.ordered.len()
    }

    pub(crate) fn name_at(&self, index: usize) -> Oop {
        self.ordered
            .get(index)
            .map_or(core::ptr::null_mut(), |entry| entry.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ffi::c_void;

    fn oop(value: &mut usize) -> Oop {
        core::ptr::from_mut(value).cast::<c_void>()
    }

    #[test]
    fn rebinding_preserves_order_and_reverse_lookup_chooses_first_alias() {
        let mut environment = Environment::new();
        let mut name_a = 0;
        let mut name_b = 0;
        let mut value_a = 0;
        let mut value_b = 0;
        let name_a = oop(&mut name_a);
        let name_b = oop(&mut name_b);
        let value_a = oop(&mut value_a);
        let value_b = oop(&mut value_b);

        assert_eq!(environment.export(name_a, value_a), value_a);
        assert_eq!(environment.export(name_b, value_a), value_a);
        assert_eq!(environment.export(name_a, value_b), value_b);
        assert_eq!(environment.count(), 2);
        assert_eq!(environment.name_at(0), name_a);
        assert_eq!(environment.name_at(1), name_b);
        assert_eq!(environment.import(name_a), value_b);
        assert_eq!(environment.import(name_b), value_a);
        assert_eq!(environment.exported_name_of(value_a), name_b);
        assert_eq!(environment.exported_name_of(value_b), name_a);

        assert_eq!(environment.export(name_a, value_a), value_a);
        assert_eq!(environment.exported_name_of(value_a), name_a);
        assert!(
            environment
                .exported_name_of(core::ptr::null_mut())
                .is_null()
        );
    }

    #[test]
    fn indexed_imports_track_current_values_without_cross_runtime_state() {
        let mut left = Environment::new();
        let mut right = Environment::new();
        let mut names = [0usize; 32];
        let names: Vec<_> = names.iter_mut().map(oop).collect();
        let mut value_storage = [0usize; 16];
        let values: Vec<_> = value_storage.iter_mut().map(oop).collect();
        let mut expected: Vec<(Oop, Oop)> = Vec::new();
        for step in 0..1024 {
            let name = names[(step * 7) % names.len()];
            let value = values[(step / 11) % values.len()];
            left.export(name, value);
            if let Some(entry) = expected.iter_mut().find(|entry| entry.0 == name) {
                entry.1 = value;
            } else {
                expected.push((name, value));
            }
            for &name in &names {
                assert_eq!(
                    left.import(name),
                    expected
                        .iter()
                        .find(|entry| entry.0 == name)
                        .map_or(core::ptr::null_mut(), |entry| entry.1)
                );
                assert!(right.import(name).is_null());
            }
            assert_eq!(
                left.roots(),
                expected
                    .iter()
                    .flat_map(|(name, value)| [*name, *value])
                    .collect::<Vec<_>>()
            );
        }
        right.export(names[0], values[0]);
        assert_eq!(right.count(), 1);
        assert_eq!(right.import(names[0]), values[0]);
        assert_eq!(left.count(), names.len());
    }

    #[test]
    fn large_export_churn_preserves_order_and_current_reverse_authority() {
        let mut environment = Environment::new();
        let count = if cfg!(miri) { 128 } else { 4096 };
        let mut name_owners: Vec<Box<usize>> = (0..count).map(Box::new).collect();
        let mut value_owners: Vec<Box<usize>> =
            (0..count).map(|index| Box::new(index + count)).collect();
        let names: Vec<Oop> = name_owners
            .iter_mut()
            .map(|owner| oop(owner.as_mut()))
            .collect();
        let values: Vec<Oop> = value_owners
            .iter_mut()
            .map(|owner| oop(owner.as_mut()))
            .collect();

        for (name, value) in names.iter().copied().zip(values.iter().copied()) {
            assert_eq!(environment.export(name, value), value);
        }
        assert_eq!(environment.count(), names.len());
        for (index, name) in names.iter().copied().enumerate() {
            assert_eq!(environment.name_at(index), name);
            assert_eq!(environment.import(name), values[index]);
            assert_eq!(environment.exported_name_of(values[index]), name);
        }

        for index in (1..names.len()).step_by(2) {
            assert_eq!(environment.export(names[index], values[0]), values[0]);
            assert_eq!(environment.name_at(index), names[index]);
        }
        assert_eq!(environment.count(), names.len());
        assert_eq!(environment.exported_name_of(values[0]), names[0]);

        let replacement = values[1];
        assert_eq!(environment.export(names[0], replacement), replacement);
        assert_eq!(environment.name_at(0), names[0]);
        assert_eq!(environment.exported_name_of(replacement), names[0]);
        assert_eq!(environment.exported_name_of(values[0]), names[1]);
    }
    #[test]
    #[ignore = "diagnostic scaling probe; host-gated timing acceptance is separate"]
    fn global_lookup_scaling() {
        use std::hint::black_box;
        use std::time::Instant;
        for count in [4, 32, 256, 4096] {
            let mut owners: Vec<_> = (0..=count).map(Box::new).collect();
            let names: Vec<_> = owners.iter_mut().map(|value| oop(value.as_mut())).collect();
            let mut env = Environment::new();
            for &name in &names[..count] {
                env.export(name, name);
            }
            for (kind, key) in [("hit", names[count - 1]), ("miss", names[count])] {
                for indexed in [false, true] {
                    let started = Instant::now();
                    for _ in 0..100_000 {
                        let key = black_box(key);
                        let result = if indexed {
                            env.import(key)
                        } else {
                            env.ordered
                                .iter()
                                .find(|entry| entry.name == key)
                                .map_or(core::ptr::null_mut(), |entry| entry.value)
                        };
                        black_box(result);
                    }
                    eprintln!(
                        "global count={count} kind={kind} indexed={indexed} ns={}",
                        started.elapsed().as_nanos() / 100_000
                    );
                }
            }
        }
    }
}
