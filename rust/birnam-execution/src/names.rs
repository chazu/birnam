//! Direct access to frequent, already-interned names. The inventory remains the
//! authority and root enumeration; this stores no namespace values or methods.
use super::*;

const HOT: [&[u8]; 8] = [
    b"at:",
    b"Integer",
    b"behaviorEpoch",
    b"nativeBindingsMatch:",
    b"usesDefaultAllocation",
    b"usesDefaultLookup:",
    b"bind:to:",
    b"canOmitIndexing",
];
pub(super) struct Names {
    inventory: HashMap<&'static [u8], Oop>,
    hot: [Oop; HOT.len()],
}
impl Names {
    pub(super) fn new(inventory: HashMap<&'static [u8], Oop>) -> Self {
        let hot = HOT.map(|name| inventory[name]);
        Self { inventory, hot }
    }
    #[inline]
    pub(super) fn get(&self, name: &[u8]) -> Option<&Oop> {
        let index = match name {
            b"at:" => 0,
            b"Integer" => 1,
            b"behaviorEpoch" => 2,
            b"nativeBindingsMatch:" => 3,
            b"usesDefaultAllocation" => 4,
            b"usesDefaultLookup:" => 5,
            b"bind:to:" => 6,
            b"canOmitIndexing" => 7,
            _ => return self.inventory.get(name),
        };
        Some(&self.hot[index])
    }
    pub(super) fn values(&self) -> impl Iterator<Item = &Oop> {
        self.inventory.values()
    }
}
impl std::ops::Index<&[u8]> for Names {
    type Output = Oop;
    #[inline]
    fn index(&self, name: &[u8]) -> &Oop {
        self.get(name).expect("owned Execution name")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owned_names_preserve_full_inventory_and_byte_equality() {
        let map: HashMap<_, _> = super::super::NAMES
            .iter()
            .chain(machine::NAMES)
            .enumerate()
            .map(|(i, &name)| (name, ptr::without_provenance_mut(i + 1)))
            .collect();
        let original_order: Vec<_> = map.values().copied().collect();
        let expected = map.clone();
        let names = Names::new(map);
        for (&name, &symbol) in &expected {
            let copied = name.to_vec();
            assert_eq!(names.get(&copied), Some(&symbol));
        }
        assert_eq!(names.get(b"unknown-local-name"), None);
        assert_eq!(names.values().copied().collect::<Vec<_>>(), original_order);
    }
}
