//! Canonical straight-line lowering supplies dataflow and allocation duties;
//! actual native implementations supply operators only at runtime admission.
use super::*;
use crate::resumable::{Literal, LogicalProgram, OperationKind, PhysicalLayout, Terminator};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Frame(u32),
    /// A distinct source occurrence, whose ordinary box is still allocated by
    /// the baseline step. Equal literals are not merged into one identity.
    Literal {
        value: i64,
        occurrence: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Send {
    pub pc: u32,
    pub selector: Vec<u8>,
    pub left: Value,
    pub right: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Template {
    inputs: Vec<Input>,
    sends: Vec<Send>,
    slot_count: u32,
    /// Includes driver PC token, body index/literal boxes and exit operand.
    step_boxes: Vec<usize>,
}

#[derive(Clone, Copy)]
enum Source {
    Input(Input),
    Result(u32),
}

impl Template {
    pub fn derive(program: &LogicalProgram<'_>, layout: &PhysicalLayout) -> Option<Self> {
        if program.entry.0 != 0
            || program.blocks.len() > MAX_OPERATIONS + 1
            || program.value_count > 4096
            || layout.slot_count == 0
            || layout.slot_count > 128
            || layout.value_slots.len() != program.value_count as usize
        {
            return None;
        }
        let mut sources = vec![None; program.value_count as usize];
        let mut inputs = Vec::new();
        let mut sends = Vec::new();
        let mut step_boxes = Vec::new();
        let slot = |value: u32| {
            layout
                .value_slots
                .get(value as usize)
                .copied()
                .filter(|&s| s < layout.slot_count)
        };
        for (pc, block) in program.blocks.iter().enumerate() {
            if block.pc.0 as usize != pc || block.operations.len() > 64 {
                return None;
            }
            let mut boxes = 2; // driver token and publication operand
            for operation in &block.operations {
                let destination = operation.result.0 as usize;
                if sources.get(destination)?.is_some() {
                    return None;
                }
                let source = match &operation.kind {
                    OperationKind::EntryReceiver
                    | OperationKind::EntryImplementation
                    | OperationKind::EntryArgument { .. }
                    | OperationKind::EntryCapture { .. }
                        if pc == 0 =>
                    {
                        Source::Input(Input::Frame(slot(operation.result.0)?))
                    }
                    OperationKind::Bind { source, .. } => {
                        boxes += 2;
                        sources.get(source.0 as usize).copied().flatten()?
                    }
                    OperationKind::Literal(Literal::SmallInt(value)) => {
                        boxes += 2;
                        Source::Input(Input::Literal {
                            value: *value,
                            occurrence: operation.result.0,
                        })
                    }
                    _ => return None,
                };
                sources[destination] = Some(source);
            }
            let mut operand = |value: crate::resumable::ValueId| -> Option<Value> {
                match sources.get(value.0 as usize).copied().flatten()? {
                    Source::Result(index) => Some(Value::Result(index)),
                    Source::Input(input) => {
                        let index = inputs
                            .iter()
                            .position(|&existing| existing == input)
                            .unwrap_or_else(|| {
                                inputs.push(input);
                                inputs.len() - 1
                            });
                        (inputs.len() <= MAX_INPUTS).then_some(Value::Input(index as u32))
                    }
                }
            };
            match &block.terminator {
                Terminator::Call {
                    receiver,
                    selector,
                    arguments,
                    result,
                    successor,
                    ..
                } => {
                    if arguments.len() != 1
                        || successor.0 as usize != pc + 1
                        || pc + 1 == program.blocks.len()
                    {
                        return None;
                    }
                    let send = Send {
                        pc: pc as u32,
                        selector: selector.to_vec(),
                        left: operand(*receiver)?,
                        right: operand(arguments[0])?,
                    };
                    if sources.get(result.0 as usize)?.is_some() {
                        return None;
                    }
                    sources[result.0 as usize] = Some(Source::Result(sends.len() as u32));
                    sends.push(send);
                    boxes += 5; // receiver, selector and one argument staging
                }
                Terminator::TailCall {
                    receiver,
                    selector,
                    arguments,
                    ..
                } => {
                    if arguments.len() != 1 || pc + 1 != program.blocks.len() {
                        return None;
                    }
                    sends.push(Send {
                        pc: pc as u32,
                        selector: selector.to_vec(),
                        left: operand(*receiver)?,
                        right: operand(arguments[0])?,
                    });
                    boxes += 5;
                }
                Terminator::Return(value) => {
                    if pc + 1 != program.blocks.len() || sources.get(value.0 as usize)?.is_none() {
                        return None;
                    }
                }
                Terminator::Continue(next)
                    if next.0 as usize == pc + 1 && pc + 1 < program.blocks.len() => {}
                _ => return None,
            }
            step_boxes.push(boxes);
        }
        if sends.len() < 2
            || sends.len() > MAX_OPERATIONS
            || sends.first()?.pc != 0
            || sends.iter().any(|s| s.selector.len() > 4096)
        {
            return None;
        }
        Some(Self {
            inputs,
            sends,
            slot_count: layout.slot_count,
            step_boxes,
        })
    }

    pub fn instantiate(&self, operators: &[Operator]) -> Option<Plan> {
        if operators.len() != self.sends.len() {
            return None;
        }
        Plan::new(
            self.inputs.len(),
            self.sends
                .iter()
                .zip(operators)
                .map(|(send, &operator)| Operation {
                    operator,
                    left: send.left,
                    right: send.right,
                })
                .collect(),
        )
    }

    pub fn inputs(&self) -> &[Input] {
        &self.inputs
    }
    pub fn sends(&self) -> &[Send] {
        &self.sends
    }
    pub fn slot_count(&self) -> u32 {
        self.slot_count
    }

    /// Entry is after PC zero's publication, immediately before its first
    /// arithmetic result. Include even the final Return step conservatively.
    pub fn remaining_boxes(&self) -> usize {
        self.step_boxes.iter().skip(1).sum::<usize>() + self.sends.len()
    }

    /// Whole-body admission follows allocation of the entry PC token only.
    pub fn body_boxes(&self) -> usize {
        self.step_boxes.iter().sum::<usize>() + self.sends.len() - 1
    }

    pub fn step_count(&self) -> usize {
        self.step_boxes.len()
    }
}
