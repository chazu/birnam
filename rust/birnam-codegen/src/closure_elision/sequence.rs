//! Acyclic alias/call continuations of one source-created virtual identity.
use super::*;

pub const MAX_CALLS: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    pub pc: u32,
    pub call_site: u32,
    pub selector: Vec<u8>,
    pub argument_sources: Vec<u32>,
    pub return_source: Option<u32>,
    pub stores: Vec<(u32, Value)>,
    /// Baseline caller indices, excluding this PC's driver token.
    pub boxed_indices: Vec<u32>,
    pub invocation_aliases: Vec<bool>,
    pub continuation: ColoredEdge,
    pub continuation_aliases: Vec<bool>,
}

pub(super) fn derive(
    program: &LogicalProgram<'_>,
    layout: &PhysicalLayout,
    first: &Plan,
    entry_values: &[Option<Value>],
    callee_layout: &PhysicalLayout,
) -> Vec<Invocation> {
    let mut sequence = Vec::new();
    let Some(first_edge) = &first.continuation else {
        return sequence;
    };
    let Terminator::Call { result, .. } = program.blocks[0].terminator else {
        return sequence;
    };
    let mut values = entry_values.to_vec();
    values[result.0 as usize] = Some(
        first
            .return_source
            .map(Value::Input)
            .unwrap_or(Value::Result(0)),
    );
    let mut aliases = first.continuation_aliases.clone().unwrap();
    let mut pc = first_edge.target.0 as usize;
    let mut visited = vec![false; program.blocks.len()];
    visited[0] = true;
    while sequence.len() + 1 < MAX_CALLS {
        let Some(block) = program.blocks.get(pc) else {
            break;
        };
        if visited[pc] || block.pc.0 as usize != pc || block.operations.len() > 128 {
            break;
        }
        visited[pc] = true;
        // Refusing an extension cannot change the last admitted continuation.
        let mut next_values = values.clone();
        let Some(invocation) = extend(
            block,
            layout,
            first,
            callee_layout,
            &mut next_values,
            &aliases,
            sequence.len() as u32 + 1,
        ) else {
            break;
        };
        pc = invocation.continuation.target.0 as usize;
        aliases = invocation.continuation_aliases.clone();
        values = next_values;
        sequence.push(invocation);
    }
    sequence
}

fn extend(
    block: &resumable::BasicBlock<'_>,
    layout: &PhysicalLayout,
    first: &Plan,
    callee_layout: &PhysicalLayout,
    values: &mut [Option<Value>],
    previous_aliases: &[bool],
    ordinal: u32,
) -> Option<Invocation> {
    let mut aliases = previous_aliases.to_vec();
    let mut stores = Vec::new();
    let mut boxes = Vec::new();
    let slot = |v: resumable::ValueId| layout.value_slots.get(v.0 as usize).copied();
    for operation in &block.operations {
        let OperationKind::Bind { source, .. } = operation.kind else {
            return None;
        };
        let value = values.get(source.0 as usize).copied().flatten()?;
        let destination = slot(operation.result)?;
        if values
            .get_mut(operation.result.0 as usize)?
            .replace(value)
            .is_some()
        {
            return None;
        }
        stores.push((destination, value));
        aliases[destination as usize] = value == Value::Closure;
        boxes.extend([destination, slot(source)?]);
    }
    let Terminator::Call {
        receiver,
        selector,
        arguments,
        result,
        successor,
        call_site,
        ..
    } = &block.terminator
    else {
        return None;
    };
    if values.get(receiver.0 as usize).copied().flatten()? != Value::Closure
        || arguments.len() != callee_layout.entry_argument_count as usize
        || selector.len() > 4096
    {
        return None;
    }
    let argument_sources: Vec<_> = arguments
        .iter()
        .map(
            |argument| match values.get(argument.0 as usize).copied().flatten()? {
                Value::Input(source) => Some(source),
                Value::Closure | Value::Result(_) => None,
            },
        )
        .collect::<Option<_>>()?;
    let return_source = if let Some(returned) = first.callee.return_entry_slot() {
        let returned = returned as u32;
        Some(
            if let Some(index) = callee_layout
                .entry_capture_slots
                .iter()
                .position(|s| *s == returned)
            {
                first.capture_sources[index]
            } else {
                *argument_sources
                    .get(returned.checked_sub(callee_layout.entry_argument_start)? as usize)?
            },
        )
    } else {
        None
    };
    if values
        .get_mut(result.0 as usize)?
        .replace(
            return_source
                .map(Value::Input)
                .unwrap_or(Value::Result(ordinal)),
        )
        .is_some()
    {
        return None;
    }
    let window = first.call_window_start;
    boxes.extend([window, slot(*receiver)?, window + 1]);
    aliases[window as usize] = true;
    aliases[window as usize + 1] = false;
    for (index, argument) in arguments.iter().enumerate() {
        let destination = window + 2 + index as u32;
        boxes.extend([destination, slot(*argument)?]);
        aliases[destination as usize] = false;
    }
    boxes.push(call_site.0);
    let edge = layout
        .edges
        .iter()
        .find(|e| e.source == block.pc && e.target == *successor)?
        .clone();
    let mut next = aliases.clone();
    for copy in &edge.copies {
        next[copy.destination as usize] = aliases[copy.source as usize];
    }
    next[edge.result_initialization_slot? as usize] = false;
    for (slot, live) in next.iter_mut().enumerate() {
        if edge
            .clear_bitmap_words
            .get(slot / 64)
            .is_some_and(|bits| bits & (1 << (slot % 64)) != 0)
        {
            *live = false;
        }
    }
    Some(Invocation {
        pc: block.pc.0,
        call_site: call_site.0,
        selector: selector.to_vec(),
        argument_sources,
        return_source,
        stores,
        boxed_indices: boxes,
        invocation_aliases: aliases,
        continuation: edge,
        continuation_aliases: next,
    })
}

#[cfg(test)]
mod tests;
