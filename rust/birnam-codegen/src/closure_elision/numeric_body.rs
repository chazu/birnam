//! Numeric closure-body descriptions; no constructor or execution authority.
use crate::numeric_chain::{Input, RecoveryMaps, Template};
use crate::resumable::{self, Literal, LogicalProgram, OperationKind, PhysicalLayout, Terminator};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Allocation {
    Driver(u32),
    Integer(i64),
    Result(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Identity {
    Entry(u32),
    Allocation(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Publication {
    pub pc: u32,
    pub action: crate::frame_region::Exit,
    /// Allocation prefix including the publication operand, before arithmetic.
    pub allocation_end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumericBody {
    pub template: Template,
    pub recovery: RecoveryMaps,
    pub allocations: Vec<Allocation>,
    pub publications: Vec<Publication>,
    pub returned: Identity,
}

impl NumericBody {
    /// Canonical multi-PC bodies use one driver argument per PC. This includes
    /// the entry token; the runtime must avoid charging that token twice.
    pub fn derive(program: &LogicalProgram<'_>, layout: &PhysicalLayout) -> Option<Self> {
        let template = Template::derive(program, layout)?;
        let recovery = RecoveryMaps::derive(program, layout)?;
        // RecoveryMaps checks exact canonical coloring, including entry slots.
        let is_input = |slot: u32| {
            (layout.entry_argument_start..layout.entry_argument_start + layout.entry_argument_count)
                .contains(&slot)
                || layout.entry_capture_slots.contains(&slot)
        };
        // A virtual receiver or implementation is not an arithmetic input.
        if template
            .inputs()
            .iter()
            .any(|input| matches!(input, Input::Frame(slot) if !is_input(*slot)))
        {
            return None;
        }
        #[derive(Clone, Copy)]
        enum Origin {
            Exposable(Identity),
            PrivateReceiver,
        }
        let slot = |value: resumable::ValueId| layout.value_slots[value.0 as usize];
        let mut origins = vec![None; program.value_count as usize];
        let mut allocations = Vec::new();
        let mut publications = Vec::new();
        let mut returned = None;
        let mut result = 0;
        for block in &program.blocks {
            allocations.push(Allocation::Driver(block.pc.0));
            for operation in &block.operations {
                let destination = slot(operation.result);
                let origin = match operation.kind {
                    OperationKind::EntryReceiver | OperationKind::EntryImplementation => {
                        Origin::PrivateReceiver
                    }
                    OperationKind::EntryArgument { .. } | OperationKind::EntryCapture { .. } => {
                        if !is_input(destination) {
                            return None;
                        }
                        Origin::Exposable(Identity::Entry(destination))
                    }
                    OperationKind::Bind { source, .. } => {
                        allocations.extend([
                            Allocation::Integer(destination as i64),
                            Allocation::Integer(slot(source) as i64),
                        ]);
                        origins[source.0 as usize]?
                    }
                    OperationKind::Literal(Literal::SmallInt(value)) => {
                        allocations.extend([
                            Allocation::Integer(destination as i64),
                            Allocation::Integer(value),
                        ]);
                        Origin::Exposable(Identity::Allocation((allocations.len() - 1) as u32))
                    }
                    _ => return None,
                };
                origins[operation.result.0 as usize] = Some(origin);
            }
            let action = match &block.terminator {
                Terminator::Call {
                    call_site,
                    receiver,
                    arguments,
                    ..
                }
                | Terminator::TailCall {
                    call_site,
                    receiver,
                    arguments,
                    ..
                } => {
                    // The Template independently requires exactly one argument.
                    let window = layout.call_window_start?;
                    allocations.extend([
                        Allocation::Integer(window as i64),
                        Allocation::Integer(slot(*receiver) as i64),
                        Allocation::Integer((window + 1) as i64),
                        Allocation::Integer((window + 2) as i64),
                        Allocation::Integer(slot(arguments[0]) as i64),
                        Allocation::Integer(call_site.0 as i64),
                    ]);
                    if matches!(block.terminator, Terminator::Call { .. }) {
                        crate::frame_region::Exit::Call(call_site.0)
                    } else {
                        crate::frame_region::Exit::TailCall(call_site.0)
                    }
                }
                Terminator::Continue(next) => {
                    allocations.push(Allocation::Integer(next.0 as i64));
                    crate::frame_region::Exit::Continue(next.0)
                }
                Terminator::Return(value) => {
                    let Origin::Exposable(identity) = origins[value.0 as usize]? else {
                        return None;
                    };
                    returned = Some(identity);
                    allocations.push(Allocation::Integer(slot(*value) as i64));
                    crate::frame_region::Exit::Return(slot(*value))
                }
            };
            publications.push(Publication {
                pc: block.pc.0,
                action,
                allocation_end: allocations.len(),
            });
            match &block.terminator {
                Terminator::Call { result: value, .. } => {
                    origins[value.0 as usize] = Some(Origin::Exposable(Identity::Allocation(
                        allocations.len() as u32,
                    )));
                    allocations.push(Allocation::Result(result));
                    result += 1;
                }
                Terminator::TailCall { .. } => {
                    returned = Some(Identity::Allocation(allocations.len() as u32));
                    allocations.push(Allocation::Result(result));
                    result += 1;
                }
                _ => {}
            }
        }
        if result as usize != template.sends().len()
            || allocations.len() != template.body_boxes() + 1
        {
            return None;
        }
        Some(Self {
            template,
            recovery,
            allocations,
            publications,
            returned: returned?,
        })
    }
}

#[cfg(test)]
mod tests;
