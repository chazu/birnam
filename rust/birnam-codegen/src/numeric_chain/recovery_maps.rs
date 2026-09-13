//! Physical aliases of virtual numeric results at original V2 boundaries.
//! These maps supply no arithmetic or allocation authority. A consumer must
//! admit current implementations and materialize before any observer can run.
use super::Template;
use crate::resumable::{
    self, ColoredEdge, LogicalProgram, OperationKind, PhysicalLayout, Terminator,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryStep {
    /// At original action publication, before this PC's numeric call executes.
    /// Some(n) names the nth successful numeric result, not its numeric bits.
    /// None leaves the ordinary physical Oop unchanged. Call-window aliases
    /// are included, so a recovered pending send has its original operands.
    pub at_action: Vec<Option<u32>>,
    /// After the original parallel copies, clears and result initialization.
    /// Return and TailCall have no local continuation.
    pub after_transition: Option<Vec<Option<u32>>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryMaps {
    steps: Vec<RecoveryStep>,
    result_count: usize,
}

impl RecoveryMaps {
    /// Owned metadata storage, excluding allocator bookkeeping and an Rc header.
    pub fn storage_bytes(&self) -> usize {
        core::mem::size_of::<Self>()
            + self.steps.capacity() * core::mem::size_of::<RecoveryStep>()
            + self
                .steps
                .iter()
                .map(|step| {
                    (step.at_action.capacity()
                        + step.after_transition.as_ref().map_or(0, Vec::capacity))
                        * core::mem::size_of::<Option<u32>>()
                })
                .sum::<usize>()
    }
    /// Derive separately from Template, so the existing boxed consumer need
    /// not pay preparation or retention costs for a virtual representation.
    pub fn derive(program: &LogicalProgram<'_>, layout: &PhysicalLayout) -> Option<Self> {
        let template = Template::derive(program, layout)?;
        if resumable::color_slots(program).as_ref() != Ok(layout) {
            return None;
        }
        let slot = |id: resumable::ValueId| layout.value_slots[id.0 as usize] as usize;
        let mut origins = vec![None; program.value_count as usize];
        let mut aliases = vec![None; layout.slot_count as usize];
        let mut steps = Vec::with_capacity(program.blocks.len());
        let mut result_count = 0;
        for block in &program.blocks {
            for op in &block.operations {
                let origin = match op.kind {
                    OperationKind::Bind { source, .. } => origins[source.0 as usize],
                    OperationKind::EntryReceiver
                    | OperationKind::EntryImplementation
                    | OperationKind::EntryArgument { .. }
                    | OperationKind::EntryCapture { .. }
                    | OperationKind::Literal(_) => None,
                    _ => return None,
                };
                origins[op.result.0 as usize] = origin;
                aliases[slot(op.result)] = origin;
            }
            let (next, result) = match &block.terminator {
                Terminator::Call {
                    receiver,
                    arguments,
                    result,
                    successor,
                    ..
                } => {
                    stage(
                        &mut aliases,
                        layout,
                        origins[receiver.0 as usize],
                        origins[arguments[0].0 as usize],
                    )?;
                    (Some(*successor), Some(*result))
                }
                Terminator::TailCall {
                    receiver,
                    arguments,
                    ..
                } => {
                    stage(
                        &mut aliases,
                        layout,
                        origins[receiver.0 as usize],
                        origins[arguments[0].0 as usize],
                    )?;
                    (None, None)
                }
                Terminator::Continue(next) => (Some(*next), None),
                Terminator::Return(_) => (None, None),
            };
            let at_action = aliases.clone();
            let after_transition = if let Some(next) = next {
                let edge = layout
                    .edges
                    .iter()
                    .find(|e| e.source == block.pc && e.target == next)?;
                transition(&mut aliases, edge);
                if let Some(result) = result {
                    if edge.result_initialization_slot != Some(slot(result) as u32) {
                        return None;
                    }
                    origins[result.0 as usize] = Some(result_count as u32);
                    aliases[slot(result)] = Some(result_count as u32);
                } else if edge.result_initialization_slot.is_some() {
                    return None;
                }
                Some(aliases.clone())
            } else {
                None
            };
            if matches!(
                block.terminator,
                Terminator::Call { .. } | Terminator::TailCall { .. }
            ) {
                if template.sends().get(result_count)?.pc != block.pc.0 {
                    return None;
                }
                result_count += 1;
            }
            steps.push(RecoveryStep {
                at_action,
                after_transition,
            });
        }
        (result_count == template.sends().len()).then_some(Self {
            steps,
            result_count,
        })
    }

    pub fn steps(&self) -> &[RecoveryStep] {
        &self.steps
    }
    pub fn result_count(&self) -> usize {
        self.result_count
    }
}

fn stage(
    aliases: &mut [Option<u32>],
    layout: &PhysicalLayout,
    receiver: Option<u32>,
    argument: Option<u32>,
) -> Option<()> {
    let start = layout.call_window_start? as usize;
    aliases[start] = receiver;
    aliases[start + 1] = None;
    aliases[start + 2] = argument;
    Some(())
}

fn transition(aliases: &mut [Option<u32>], edge: &ColoredEdge) {
    let before = aliases.to_vec();
    for copy in &edge.copies {
        aliases[copy.destination as usize] = before[copy.source as usize];
    }
    for (slot, alias) in aliases.iter_mut().enumerate() {
        if edge.clear_bitmap_words[slot / 64] & (1 << (slot % 64)) != 0 {
            *alias = None;
        }
    }
    if let Some(slot) = edge.result_initialization_slot {
        aliases[slot as usize] = None;
    }
}

#[cfg(test)]
mod tests;
