//! A bounded no-escape proof for discarded allocation results.
//! Method meaning and allocation/observation permissions are runtime obligations.
use crate::resumable::{LogicalProgram, OperationKind, PhysicalLayout, Terminator, ValueId};

pub const MAX_CLONES: usize = 32;
pub const MAX_SLOTS: usize = 128;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Site {
    pub pc: usize,
    pub source_slot: usize,
    pub selector: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    sites: Vec<Site>,
    integer_boxes: usize,
}
#[derive(Clone, Copy)]
enum Origin {
    Entry(usize),
    Discarded,
}
impl Plan {
    pub fn sites(&self) -> &[Site] {
        &self.sites
    }
    /// All original integer boxes after the already-created first PC token.
    pub fn integer_boxes(&self) -> usize {
        self.integer_boxes
    }
    pub fn derive(program: &LogicalProgram<'_>, layout: &PhysicalLayout) -> Option<Self> {
        if program.entry.0 != 0
            || !(2..=MAX_CLONES + 1).contains(&program.blocks.len())
            || layout.slot_count as usize > MAX_SLOTS
            || program.value_count > 4096
            || layout.value_slots.len() != program.value_count as usize
        {
            return None;
        }
        let slot = |value: ValueId| {
            layout
                .value_slots
                .get(value.0 as usize)
                .copied()
                .filter(|value| *value < layout.slot_count)
                .map(|value| value as usize)
        };
        let mut origins = vec![None; program.value_count as usize];
        let mut sites = Vec::new();
        let mut boxes = program.blocks.len() - 1; // later PC tokens
        for (pc, block) in program.blocks.iter().enumerate() {
            if block.pc.0 as usize != pc || block.operations.len() > 64 {
                return None;
            }
            for operation in &block.operations {
                let origin = match operation.kind {
                    OperationKind::EntryReceiver
                    | OperationKind::EntryImplementation
                    | OperationKind::EntryArgument { .. }
                    | OperationKind::EntryCapture { .. }
                        if pc == 0 =>
                    {
                        Origin::Entry(slot(operation.result)?)
                    }
                    OperationKind::Bind { source, .. } => {
                        boxes += 2;
                        origins.get(source.0 as usize).copied().flatten()?
                    }
                    _ => return None,
                };
                slot(operation.result)?;
                *origins.get_mut(operation.result.0 as usize)? = Some(origin);
            }
            match &block.terminator {
                Terminator::Call {
                    receiver,
                    selector,
                    arguments,
                    result,
                    successor,
                    ..
                } if arguments.is_empty()
                    && successor.0 as usize == pc + 1
                    && pc + 1 < program.blocks.len()
                    && selector.len() <= 4096 =>
                {
                    let Origin::Entry(source_slot) =
                        origins.get(receiver.0 as usize).copied().flatten()?
                    else {
                        return None;
                    };
                    slot(*result)?;
                    *origins.get_mut(result.0 as usize)? = Some(Origin::Discarded);
                    sites.push(Site {
                        pc,
                        source_slot,
                        selector: selector.to_vec(),
                    });
                    boxes += 4; // receiver/source/selector indices and call publication
                }
                Terminator::Return(result) if pc + 1 == program.blocks.len() => {
                    if !matches!(
                        origins.get(result.0 as usize).copied().flatten()?,
                        Origin::Entry(_)
                    ) {
                        return None;
                    }
                    boxes += 1;
                }
                _ => return None,
            }
        }
        (sites.len() + 1 == program.blocks.len()).then_some(Self {
            sites,
            integer_boxes: boxes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Block, Expr, Statement, portable, resumable};
    use std::borrow::Cow;
    #[test]
    fn discarded_clone_proof_rejects_escaping_and_observing_uses() {
        for escape in [false, true] {
            let bytes = portable::encode_block(
                &Block {
                    params: vec![b"prototype", b"answer"],
                    body: vec![
                        Statement::Let {
                            name: b"copy",
                            value: Expr::Send {
                                receiver: Box::new(Expr::Global(b"prototype")),
                                selector: Cow::Borrowed(b"arbitraryName"),
                                args: vec![],
                                source: b"",
                            },
                        },
                        Statement::Let {
                            name: b"alias",
                            value: Expr::Global(b"copy"),
                        },
                        Statement::Expr(Expr::Global(if escape { b"alias" } else { b"answer" })),
                    ],
                },
                &[],
            )
            .unwrap();
            let logical = resumable::lower_portable_block(&bytes).unwrap();
            let layout = resumable::color_slots(&logical).unwrap();
            let plan = Plan::derive(&logical, &layout);
            assert_eq!(plan.is_none(), escape);
            if let Some(plan) = plan {
                assert_eq!(plan.sites[0].selector, b"arbitraryName");
                assert_eq!(plan.integer_boxes, 10);
            }
            let mut bad = logical.clone();
            bad.blocks[0].operations.push(crate::resumable::Operation {
                result: ValueId(0),
                kind: OperationKind::GlobalLookup(b"Observer"),
            });
            assert!(Plan::derive(&bad, &layout).is_none());
        }
    }
}
