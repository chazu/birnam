//! Bounded one-callee plans derived from canonical lowering and slot coloring.
//! No runtime Oops, addresses, ABI fields or serialized artifact state.
use crate::resumable::{LogicalProgram, OperationKind, PhysicalLayout, Terminator};

pub const MAX_OPERATIONS: usize = 32;
pub const MAX_SLOTS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    RegionShape,
    Limit,
    ObservingOperation,
    InvalidLayout,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    return_entry_slot: u32,
    return_slot: u32,
    // Destination-indexed entry sources. None means explicit canonical nil.
    recovery_sources: Vec<Option<u32>>,
    // Exactly the canonical emitter's allocation order, excluding driver PC.
    boxed_indices: Vec<u32>,
}

impl Plan {
    pub fn return_entry_slot(&self) -> usize {
        self.return_entry_slot as usize
    }
    pub fn return_slot(&self) -> usize {
        self.return_slot as usize
    }
    pub fn recovery_pc(&self) -> usize {
        0
    }
    pub fn recovery_sources(&self) -> &[Option<u32>] {
        &self.recovery_sources
    }
    pub fn boxed_indices(&self) -> &[u32] {
        &self.boxed_indices
    }
    pub fn alias_count(&self) -> usize {
        (self.boxed_indices.len() - 1) / 2
    }
}

/// `layout` is the same canonical coloring used for baseline descriptor/emission.
/// The runtime additionally compares that descriptor's bytes at registration.
pub fn compile(program: &LogicalProgram<'_>, layout: &PhysicalLayout) -> Result<Plan, Refusal> {
    if program.entry.0 != 0 || program.blocks.len() != 1 || program.blocks[0].pc.0 != 0 {
        return Err(Refusal::RegionShape);
    }
    let block = &program.blocks[0];
    let Terminator::Return(returned) = block.terminator else {
        return Err(Refusal::RegionShape);
    };
    if block.operations.len() > MAX_OPERATIONS
        || layout.slot_count as usize > MAX_SLOTS
        || program.value_count as usize > MAX_OPERATIONS
    {
        return Err(Refusal::Limit);
    }
    if layout.value_slots.len() != program.value_count as usize || layout.slot_count == 0 {
        return Err(Refusal::InvalidLayout);
    }
    let slot = |value: crate::resumable::ValueId| {
        layout
            .value_slots
            .get(value.0 as usize)
            .copied()
            .filter(|s| *s < layout.slot_count)
            .ok_or(Refusal::InvalidLayout)
    };
    let mut origins = vec![None; program.value_count as usize];
    let mut recovery_sources = vec![None; layout.slot_count as usize];
    let mut boxed_indices = Vec::new();
    for operation in &block.operations {
        let destination = slot(operation.result)?;
        let origin = match operation.kind {
            OperationKind::EntryReceiver
            | OperationKind::EntryArgument { .. }
            | OperationKind::EntryCapture { .. } => {
                if recovery_sources[destination as usize]
                    .replace(destination)
                    .is_some()
                {
                    return Err(Refusal::InvalidLayout);
                }
                destination
            }
            OperationKind::Bind { source, .. } => {
                let origin = origins
                    .get(source.0 as usize)
                    .copied()
                    .flatten()
                    .ok_or(Refusal::InvalidLayout)?;
                // emit_store_prefix allocates destination before emit_get's source.
                boxed_indices.extend([destination, slot(source)?]);
                origin
            }
            _ => return Err(Refusal::ObservingOperation),
        };
        if origins[operation.result.0 as usize]
            .replace(origin)
            .is_some()
        {
            return Err(Refusal::InvalidLayout);
        }
    }
    let return_slot = slot(returned)?;
    let return_entry_slot = origins
        .get(returned.0 as usize)
        .copied()
        .flatten()
        .ok_or(Refusal::InvalidLayout)?;
    boxed_indices.push(return_slot);
    Ok(Plan {
        return_entry_slot,
        return_slot,
        recovery_sources,
        boxed_indices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Block, Expr, Statement, portable, resumable};

    #[test]
    fn small_callee_plans_preserve_entry_provenance_and_refuse_observers() {
        for captured in [false, true] {
            let bytes = portable::encode_block(
                &Block {
                    params: vec![b"input"],
                    body: vec![
                        Statement::Let {
                            name: b"alias",
                            value: Expr::Global(if captured { b"saved" } else { b"input" }),
                        },
                        Statement::Let {
                            name: b"again",
                            value: Expr::Global(b"alias"),
                        },
                        Statement::Expr(Expr::Global(b"again")),
                    ],
                },
                &if captured {
                    vec![b"saved".to_vec()]
                } else {
                    vec![]
                },
            )
            .unwrap();
            let program = resumable::lower_portable_block(&bytes).unwrap();
            let layout = resumable::color_slots(&program).unwrap();
            let plan = compile(&program, &layout).unwrap();
            let entry = if captured {
                layout.entry_capture_slots[0]
            } else {
                layout.entry_argument_start
            };
            assert_eq!(plan.return_entry_slot(), entry as usize);
            assert_ne!(
                plan.return_slot(),
                plan.return_entry_slot(),
                "alias return must not read its uninitialized colored slot"
            );
            assert_eq!(plan.alias_count(), 2);
            assert_eq!(plan.recovery_sources()[plan.return_slot()], None);
            assert_eq!(plan.recovery_sources()[entry as usize], Some(entry));
            let mut bad = layout.clone();
            bad.value_slots[0] = layout.slot_count;
            assert_eq!(compile(&program, &bad), Err(Refusal::InvalidLayout));
        }
        for expression in [
            Expr::SmallInt(1),
            Expr::Global(b"Observer"),
            Expr::Block(Box::new(Block {
                params: vec![],
                body: vec![Statement::Expr(Expr::Nil)],
            })),
        ] {
            let bytes = portable::encode_block(
                &Block {
                    params: vec![],
                    body: vec![Statement::Expr(expression)],
                },
                &[],
            )
            .unwrap();
            let program = resumable::lower_portable_block(&bytes).unwrap();
            let layout = resumable::color_slots(&program).unwrap();
            assert_eq!(compile(&program, &layout), Err(Refusal::ObservingOperation));
        }
    }

    #[test]
    fn small_callee_limits_and_call_boundaries_decline() {
        let bytes = portable::encode_block(
            &Block {
                params: vec![b"input"],
                body: vec![Statement::Expr(Expr::Send {
                    receiver: Box::new(Expr::Global(b"input")),
                    selector: std::borrow::Cow::Borrowed(b"observe"),
                    args: vec![],
                    source: b"",
                })],
            },
            &[],
        )
        .unwrap();
        let program = resumable::lower_portable_block(&bytes).unwrap();
        assert_eq!(
            compile(&program, &resumable::color_slots(&program).unwrap()),
            Err(Refusal::RegionShape)
        );
        let names: Vec<_> = (0..MAX_OPERATIONS)
            .map(|i| format!("alias{i}").into_bytes())
            .collect();
        let mut body: Vec<_> = names
            .iter()
            .map(|name| Statement::Let {
                name: name.as_slice(),
                value: Expr::Global(b"input"),
            })
            .collect();
        body.push(Statement::Expr(Expr::Global(names.last().unwrap())));
        let bytes = portable::encode_block(
            &Block {
                params: vec![b"input"],
                body,
            },
            &[],
        )
        .unwrap();
        let program = resumable::lower_portable_block(&bytes).unwrap();
        assert_eq!(
            compile(&program, &resumable::color_slots(&program).unwrap()),
            Err(Refusal::Limit)
        );
    }
}
