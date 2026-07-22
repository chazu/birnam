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
}
