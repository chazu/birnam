//! Ordered import/export namespace and reverse-name lookup.

use crate::abi::Oop;

#[derive(Clone, Copy)]
struct Export {
    name: Oop,
    value: Oop,
}

pub(crate) struct Environment {
    ordered: Vec<Export>,
}

impl Environment {
    pub(crate) fn new() -> Self {
        Self {
            ordered: Vec::new(),
        }
    }

    pub(crate) fn export(&mut self, name: Oop, value: Oop) -> Oop {
        if let Some(existing) = self.ordered.iter_mut().find(|entry| entry.name == name) {
            existing.value = value;
            return value;
        }
        self.ordered.push(Export { name, value });
        value
    }

    pub(crate) fn import(&self, name: Oop) -> Oop {
        self.ordered
            .iter()
            .find(|entry| entry.name == name)
            .map_or(core::ptr::null_mut(), |entry| entry.value)
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

    /// Snapshot values that the process export namespace keeps live. Export
    /// names are interned permanent Symbols and are rooted by the interner.
    pub(crate) fn root_values(&self) -> Vec<Oop> {
        self.ordered.iter().map(|entry| entry.value).collect()
    }

    #[cfg(test)]
    pub(crate) fn count(&self) -> usize {
        self.ordered.len()
    }

    #[cfg(test)]
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
}
