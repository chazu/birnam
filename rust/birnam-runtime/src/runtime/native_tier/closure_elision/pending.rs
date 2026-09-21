//! Owned, bounded compiler inputs; these bytes confer no execution authority.
use super::*;
use birnam_artifact::portable_program::{self, NodeKind};

const MAX_INPUT_BYTES: usize = 256 << 10;
const MAX_PENDING_BYTES: usize = 4 << 20;

#[derive(Clone, Default)]
pub(in crate::runtime::native_tier) struct Budget(Rc<Cell<usize>>);

impl Budget {
    #[cfg(feature = "ffi-test-hooks")]
    pub(in crate::runtime::native_tier) fn bytes(&self) -> usize {
        self.0.get()
    }

    fn reserve(&self, bytes: usize) -> Option<Reservation> {
        let total = self.0.get().checked_add(bytes)?;
        if total > MAX_PENDING_BYTES {
            return None;
        }
        self.0.set(total);
        Some(Reservation {
            budget: self.clone(),
            bytes,
        })
    }
}

struct Reservation {
    budget: Budget,
    bytes: usize,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        self.budget.0.set(
            self.budget
                .0
                .get()
                .checked_sub(self.bytes)
                .expect("owned pending reservation"),
        );
    }
}

pub(in crate::runtime::native_tier) struct Pending {
    bytes: Box<[u8]>,
    selector: Box<[u8]>,
    row: selection::Row<'static>,
    reservation: Reservation,
}

impl Pending {
    pub(in crate::runtime::native_tier) fn new(
        bytes: &[u8],
        row: selection::Row<'_>,
        budget: &Budget,
    ) -> Option<Self> {
        if bytes.len() > MAX_INPUT_BYTES || row.selector.len() > MAX_INPUT_BYTES {
            return None;
        }
        let program = portable_program::inspect(bytes).ok()?;
        if !(0..program.node_count as usize).any(|index| {
            program
                .node(index)
                .is_some_and(|node| node.kind == NodeKind::BlockLiteral as u32)
        }) {
            return None;
        }
        let reservation = budget.reserve(bytes.len().checked_add(row.selector.len())?)?;
        Some(Self {
            bytes: bytes.into(),
            selector: row.selector.into(),
            row: selection::Row {
                selector: &[],
                ..row
            },
            reservation,
        })
    }

    pub(in crate::runtime::native_tier) fn prepare(&self) -> Option<Prepared> {
        Prepared::new(
            &self.bytes,
            selection::Row {
                selector: &self.selector,
                ..self.row
            },
        )
    }

    pub(super) fn expected(&self, bytes: &[u8]) -> Option<Expected> {
        if bytes.len() > MAX_INPUT_BYTES {
            return None;
        }
        let reservation = self.reservation.budget.reserve(bytes.len())?;
        Some(Expected {
            bytes: bytes.into(),
            _reservation: reservation,
        })
    }

    #[cfg(feature = "ffi-test-hooks")]
    pub(super) fn payload_bytes(&self) -> usize {
        self.bytes.len() + self.selector.len()
    }
}

pub(super) struct Expected {
    pub bytes: Box<[u8]>,
    _reservation: Reservation,
}

#[cfg(test)]
mod tests {
    use super::*;
    use birnam_artifact::code_binding_inventory;
    use birnam_codegen::{Block, Expr, Statement, portable};
    use std::borrow::Cow;

    fn fixture() -> (Vec<u8>, selection::Row<'static>, Vec<u8>) {
        let bytes = portable::encode_block(
            &Block {
                params: vec![b"input"],
                body: vec![
                    Statement::Let {
                        name: b"closure",
                        value: Expr::Block(Box::new(Block {
                            params: vec![],
                            body: vec![Statement::Expr(Expr::Global(b"input"))],
                        })),
                    },
                    Statement::Let {
                        name: b"result",
                        value: Expr::Send {
                            receiver: Box::new(Expr::Global(b"closure")),
                            selector: Cow::Borrowed(b"value"),
                            args: vec![],
                            source: b"",
                        },
                    },
                    Statement::Expr(Expr::Global(b"closure")),
                ],
            },
            &[],
        )
        .unwrap();
        let programs = portable_inventory::encode_closure(&[&bytes]).unwrap();
        let inventory = portable_inventory::inspect(&programs).unwrap();
        let bindings = code_binding_inventory::encode(&[]).unwrap();
        let selected = selection::encode(
            &programs,
            &bindings,
            &[],
            &vec![selection::Disposition::Resumable; inventory.program_count as usize],
        )
        .unwrap();
        let selected = selection::inspect(&selected, &programs, &bindings).unwrap();
        let row = selected.row(inventory.program_count as usize - 1).unwrap();
        assert_eq!(row.selector, b"value:");
        let row = selection::Row {
            selector: b"value:",
            ..row
        };
        let bytes = inventory
            .program(row.subject_index as usize)
            .unwrap()
            .to_vec();
        let logical = resumable::lower_portable_block(&bytes).unwrap();
        let layout = resumable::color_slots(&logical).unwrap();
        let descriptor =
            resumable_descriptor::compile_portable_descriptor_v2(&logical, &layout, row, 1)
                .unwrap();
        (bytes, row, descriptor)
    }

    #[test]
    fn pending_budget_tracks_shared_owners_and_recovers_after_exhaustion() {
        let (bytes, row, descriptor) = fixture();
        let budget = Budget::default();
        let full = budget.reserve(MAX_PENDING_BYTES).unwrap();
        assert!(Pending::new(&bytes, row, &budget).is_none());
        drop(full);
        let input = Rc::new(Pending::new(&bytes, row, &budget).unwrap());
        let shared = input.clone();
        assert_eq!(budget.0.get(), bytes.len() + row.selector.len());
        let expected = input.expected(&descriptor).unwrap();
        assert_eq!(
            budget.0.get(),
            bytes.len() + row.selector.len() + descriptor.len()
        );
        assert!(input.expected(&vec![0; MAX_INPUT_BYTES + 1]).is_none());
        assert!(Pending::new(&vec![0; MAX_INPUT_BYTES + 1], row, &budget).is_none());
        drop(input);
        assert_eq!(
            budget.0.get(),
            bytes.len() + row.selector.len() + descriptor.len()
        );
        drop(shared);
        assert_eq!(budget.0.get(), descriptor.len());
        drop(expected);
        assert_eq!(budget.0.get(), 0);
    }

    #[test]
    fn closure_plan_derivation_waits_for_hot_visit_and_releases_inputs() {
        let (bytes, row, expected) = fixture();
        let budget = Budget::default();
        let input = Rc::new(Pending::new(&bytes, row, &budget).unwrap());
        let weak = Rc::downgrade(&input);
        // A descriptor owner suffices here: this test does no Frame execution.
        let descriptor = FrameDescriptor::validate_v2(crate::execution::probe_spec(2, 1)).unwrap();
        let mut record = Record::new(input, &descriptor, &expected).unwrap();
        let before = DERIVATIONS.get();
        for _ in 1..HOT_ENTRIES {
            assert!(matches!(record.visit(), Visit::Cold));
            assert_eq!(DERIVATIONS.get(), before);
            assert!(budget.0.get() > 0);
        }
        let Visit::Ready(prepared) = record.visit() else {
            panic!("hot validated plan");
        };
        assert_eq!(prepared.descriptor, expected);
        assert_eq!(DERIVATIONS.get(), before + 1);
        assert_eq!(budget.0.get(), 0);
        assert!(weak.upgrade().is_none());
        let Visit::Ready(next) = record.visit() else {
            panic!("cached plan");
        };
        assert!(Rc::ptr_eq(&prepared, &next));
        assert_eq!(DERIVATIONS.get(), before + 1);
    }

    #[test]
    fn hot_descriptor_mismatch_refuses_once_and_drops_pending_bytes() {
        let (bytes, row, mut expected) = fixture();
        expected[0] ^= 1;
        let budget = Budget::default();
        let input = Rc::new(Pending::new(&bytes, row, &budget).unwrap());
        let descriptor = FrameDescriptor::validate_v2(crate::execution::probe_spec(2, 1)).unwrap();
        let mut record = Record::new(input, &descriptor, &expected).unwrap();
        let before = DERIVATIONS.get();
        for _ in 1..HOT_ENTRIES {
            assert!(matches!(record.visit(), Visit::Cold));
        }
        assert!(matches!(record.visit(), Visit::Refused));
        assert_eq!(DERIVATIONS.get(), before + 1);
        assert_eq!(budget.0.get(), 0);
        assert!(matches!(record.visit(), Visit::Refused));
        assert_eq!(DERIVATIONS.get(), before + 1);
    }

    #[test]
    fn retirement_releases_cold_inputs_without_derivation() {
        let (bytes, row, expected) = fixture();
        let budget = Budget::default();
        let input = Rc::new(Pending::new(&bytes, row, &budget).unwrap());
        let descriptor = FrameDescriptor::validate_v2(crate::execution::probe_spec(2, 1)).unwrap();
        let generation = descriptor.identity().generation;
        let record = Record::new(input, &descriptor, &expected).unwrap();
        let mut registry = Registry::default();
        registry
            .closures
            .insert(Arc::as_ptr(&descriptor) as usize, record);
        let before = DERIVATIONS.get();
        drop(descriptor);
        assert!(registry.closures.values().all(Record::expired_pending));
        let retired = registry.retire_generation(generation);
        assert!(registry.closures.is_empty());
        assert!(budget.0.get() > 0);
        drop(retired);
        assert_eq!(budget.0.get(), 0);
        assert_eq!(DERIVATIONS.get(), before);
    }
}
