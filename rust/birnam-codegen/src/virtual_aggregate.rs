//! Symbolic values and complete physical-slot recovery for a bounded call prefix.
//! Native operation meaning is established separately by runtime admission.
use crate::resumable::{LogicalProgram, OperationKind, PhysicalLayout, Terminator, ValueId};

pub const MAX_CALLS: usize = 32;
pub const MAX_SLOTS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    Input(usize),
    Result(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Exit {
    Call {
        receiver: Value,
        selector: Vec<u8>,
        arguments: Vec<Value>,
        result_slot: usize,
    },
    Return(Value),
    Boundary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub pc: usize,
    /// Every physical slot still containing an earlier call's result, including
    /// aliases and staging scratch. None means ordinary physical storage.
    pub before: Vec<Option<usize>>,
    pub exit: Exit,
    /// Original boxes for operations/publication plus this PC's resume token;
    /// PC zero's token was allocated before native admission.
    pub integer_boxes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub steps: Vec<Step>,
    pub slot_count: usize,
}

impl Plan {
    pub fn derive(program: &LogicalProgram<'_>, layout: &PhysicalLayout) -> Option<Self> {
        if program.entry.0 != 0
            || program.blocks.len() < 2
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
                .filter(|s| *s < layout.slot_count)
                .map(|s| s as usize)
        };
        let mut values = vec![None; program.value_count as usize];
        let mut slots = vec![None; layout.slot_count as usize];
        let mut steps = Vec::new();
        for (pc, block) in program.blocks.iter().take(MAX_CALLS + 1).enumerate() {
            if block.pc.0 as usize != pc {
                return None;
            }
            let before = slots.clone();
            let mut boxes = usize::from(pc != 0);
            let mut supported = block.operations.len() <= 64;
            if supported {
                for operation in &block.operations {
                    let destination = slot(operation.result)?;
                    let value = match operation.kind {
                        OperationKind::EntryReceiver
                        | OperationKind::EntryImplementation
                        | OperationKind::EntryArgument { .. }
                        | OperationKind::EntryCapture { .. }
                            if pc == 0 =>
                        {
                            Value::Input(destination)
                        }
                        OperationKind::Bind { source, .. } => {
                            boxes += 2;
                            values.get(source.0 as usize).copied().flatten()?
                        }
                        _ => {
                            supported = false;
                            break;
                        }
                    };
                    slots[destination] = match value {
                        Value::Input(_) => None,
                        Value::Result(index) => Some(index),
                    };
                    *values.get_mut(operation.result.0 as usize)? = Some(value);
                }
            }
            let exit = if !supported || pc == MAX_CALLS {
                Exit::Boundary
            } else {
                match &block.terminator {
                    Terminator::Call {
                        receiver,
                        selector,
                        arguments,
                        result,
                        successor,
                        ..
                    } if successor.0 as usize == pc + 1
                        && pc + 1 < program.blocks.len()
                        && selector.len() <= 4096
                        && arguments.len() <= 32 =>
                    {
                        let receiver = values.get(receiver.0 as usize).copied().flatten()?;
                        let arguments = arguments
                            .iter()
                            .map(|v| values.get(v.0 as usize).copied().flatten())
                            .collect::<Option<Vec<_>>>()?;
                        let window = layout.call_window_start? as usize;
                        if window.checked_add(2 + arguments.len())? > slots.len() {
                            return None;
                        }
                        let tag = |v| match v {
                            Value::Input(_) => None,
                            Value::Result(i) => Some(i),
                        };
                        slots[window] = tag(receiver);
                        slots[window + 1] = None;
                        for (index, &argument) in arguments.iter().enumerate() {
                            slots[window + 2 + index] = tag(argument);
                        }
                        boxes += 4 + 2 * arguments.len();
                        let edge = layout.edges.iter().find(|edge| {
                            edge.source.0 as usize == pc && edge.target == *successor
                        })?;
                        let copies: Vec<_> = edge
                            .copies
                            .iter()
                            .map(|copy| slots.get(copy.source as usize).copied())
                            .collect::<Option<_>>()?;
                        for (copy, value) in edge.copies.iter().zip(copies) {
                            *slots.get_mut(copy.destination as usize)? = value;
                        }
                        let result_slot = edge.result_initialization_slot? as usize;
                        *slots.get_mut(result_slot)? = Some(pc);
                        for (word, bits) in edge.clear_bitmap_words.iter().copied().enumerate() {
                            for bit in 0..64 {
                                if bits & (1 << bit) != 0 {
                                    *slots.get_mut(word * 64 + bit)? = None;
                                }
                            }
                        }
                        *values.get_mut(result.0 as usize)? = Some(Value::Result(pc));
                        Exit::Call {
                            receiver,
                            selector: selector.to_vec(),
                            arguments,
                            result_slot,
                        }
                    }
                    Terminator::Return(value) => {
                        boxes += 1;
                        Exit::Return(values.get(value.0 as usize).copied().flatten()?)
                    }
                    _ => Exit::Boundary,
                }
            };
            let stop = !matches!(exit, Exit::Call { .. });
            steps.push(Step {
                pc,
                before,
                exit,
                integer_boxes: boxes,
            });
            if stop {
                break;
            }
        }
        (steps.len() >= 2).then_some(Self {
            steps,
            slot_count: layout.slot_count as usize,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Block, Expr, Statement, portable, resumable};
    use std::borrow::Cow;

    #[test]
    fn virtual_aliases_follow_actual_v2_copies_and_clears_before_observation() {
        let send = |receiver, selector| Expr::Send {
            receiver: Box::new(Expr::Global(receiver)),
            selector: Cow::Borrowed(selector),
            args: vec![],
            source: b"",
        };
        let bytes = portable::encode_block(
            &Block {
                params: vec![b"prototype"],
                body: vec![
                    Statement::Let {
                        name: b"copy",
                        value: send(b"prototype", b"clone"),
                    },
                    Statement::Let {
                        name: b"alias",
                        value: Expr::Global(b"copy"),
                    },
                    Statement::Let {
                        name: b"field",
                        value: send(b"alias", b"name"),
                    },
                    Statement::Expr(Expr::Global(b"alias")),
                ],
            },
            &[],
        )
        .unwrap();
        let logical = resumable::lower_portable_block(&bytes).unwrap();
        let layout = resumable::color_slots(&logical).unwrap();
        let plan = Plan::derive(&logical, &layout).unwrap();
        assert_eq!(plan.steps.len(), 3);
        assert!(matches!(
            plan.steps[1].exit,
            Exit::Call {
                receiver: Value::Result(0),
                ..
            }
        ));
        assert_eq!(plan.steps[2].exit, Exit::Return(Value::Result(0)));
        assert!(plan.steps[2].before.contains(&Some(0)));
        // An ordinary global read can observe execution. Stop before any of
        // that PC's writes and retain the pre-PC physical reconstruction map.
        let mut observed = logical.clone();
        observed.blocks[1].operations[0].kind = OperationKind::GlobalLookup(b"Observer");
        let boundary = Plan::derive(&observed, &layout).unwrap();
        assert_eq!(boundary.steps[1].exit, Exit::Boundary);
        assert_eq!(boundary.steps[1].before, plan.steps[1].before);
    }
}
