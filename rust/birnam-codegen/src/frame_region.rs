//! Private native kernels over existing, fully observable baseline Frames.
//! Runtime admission proves no observation/collection inside the whole kernel.
use crate::resumable::{LogicalProgram, OperationKind, PhysicalLayout, Terminator, ValueId};

pub const MAX_OPERATIONS: usize = 64;
pub const MAX_SLOTS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    Return(u32),
    Continue(u32),
    Call(u32),
    TailCall(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Limit,
    ObservingOperation,
    ExitShape,
    InvalidLayout,
    NoWork,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub pc: u32,
    pub slot_count: u32,
    pub copies: Vec<(u32, u32)>, // destination, source
    pub boxed_indices: Vec<u32>, // body only; exit operand is prepared after kernel
    pub exit: Exit,
    pub selector_store: Option<(usize, u32, Vec<u8>)>,
}

impl Plan {
    /// Straight repeated-input stores were not profitable in the isolated
    /// runtime matrix. Admit propagation when it can remove a dependency on a
    /// prior copy or an overwritten destination, rather than just reuse reads.
    pub fn benefits_from_propagation(&self) -> bool {
        let mut written = [false; MAX_SLOTS];
        for (index, &(destination, source)) in self.copies.iter().enumerate() {
            if let Some((after, slot, _)) = &self.selector_store
                && *after == index
            {
                written[*slot as usize] = true;
            }
            if written[source as usize] || written[destination as usize] {
                return true;
            }
            written[destination as usize] = true;
        }
        false
    }

    /// Copy propagation and dead-store elimination over physical identities.
    /// Preloading the surviving inputs makes parallel final stores safe even
    /// when the original sequence reuses slots or contains copy cycles.
    pub fn emit_propagated(&self) -> (Vec<u8>, usize) {
        let symbol = self.slot_count;
        let mut origin: Vec<u32> = (0..self.slot_count).collect();
        for (index, &(destination, source)) in self.copies.iter().enumerate() {
            if let Some((after, slot, _)) = self.selector_store.as_ref()
                && *after == index
            {
                origin[*slot as usize] = symbol;
            }
            origin[destination as usize] = origin[source as usize];
        }
        if let Some((after, slot, _)) = self.selector_store.as_ref()
            && *after == self.copies.len()
        {
            origin[*slot as usize] = symbol;
        }
        let mut code = format!(
            "extern void birnam_region_fill(unsigned long*,unsigned long,unsigned long);\nvoid region_{}(unsigned long *words,unsigned long symbol){{\n",
            self.pc
        );
        let writes: Vec<_> = origin
            .iter()
            .copied()
            .enumerate()
            .filter(|&(slot, value)| slot as u32 != value)
            .collect();
        let inputs: std::collections::BTreeSet<_> = writes
            .iter()
            .map(|&(_, value)| value)
            .filter(|&value| value != symbol)
            .collect();
        for &input in &inputs {
            code.push_str(&format!("unsigned long v{input}=words[{input}];\n"));
        }
        let mut i = 0;
        while i < writes.len() {
            let (slot, value) = writes[i];
            let mut end = i + 1;
            while end < writes.len() && writes[end] == (slot + end - i, value) {
                end += 1;
            }
            let name = if value == symbol {
                "symbol".to_owned()
            } else {
                format!("v{value}")
            };
            if end - i >= 4 {
                code.push_str(&format!(
                    "birnam_region_fill(words+{slot},{},{name});\n",
                    end - i
                ));
            } else {
                for &(destination, _) in &writes[i..end] {
                    code.push_str(&format!("words[{destination}]={name};\n"));
                }
            }
            i = end;
        }
        code.push_str("}\n");
        (code.into_bytes(), inputs.len())
    }

    /// Code is process-local and uses no public floor entry or serialized field.
    /// Every write remains; read reuse dies on a write to the same physical slot.
    pub fn emit(&self, reuse_reads: bool) -> (Vec<u8>, usize) {
        let mut code = format!(
            "void region_{}(unsigned long *words,unsigned long symbol){{\n",
            self.pc
        );
        let mut available = vec![None; self.slot_count as usize];
        let mut loads = 0;
        for (index, &(destination, source)) in self.copies.iter().enumerate() {
            if let Some((after, slot, _)) = &self.selector_store
                && *after == index
            {
                code.push_str(&format!("words[{slot}]=symbol;\n"));
                available[*slot as usize] = None;
            }
            let local = if reuse_reads {
                available[source as usize]
            } else {
                None
            };
            let local = local.unwrap_or_else(|| {
                code.push_str(&format!("unsigned long v{index}=words[{source}];\n"));
                loads += 1;
                available[source as usize] = Some(index);
                index
            });
            code.push_str(&format!("words[{destination}]=v{local};\n"));
            available[destination as usize] = None;
        }
        if let Some((after, slot, _)) = &self.selector_store
            && *after == self.copies.len()
        {
            code.push_str(&format!("words[{slot}]=symbol;\n"));
        }
        code.push_str("}\n");
        (code.into_bytes(), loads)
    }
}

pub fn compile(
    program: &LogicalProgram<'_>,
    layout: &PhysicalLayout,
    pc: u32,
) -> Result<Plan, Refusal> {
    if layout.slot_count == 0 || layout.value_slots.len() != program.value_count as usize {
        return Err(Refusal::InvalidLayout);
    }
    let block = program
        .blocks
        .get(pc as usize)
        .filter(|b| b.pc.0 == pc)
        .ok_or(Refusal::InvalidLayout)?;
    if block.operations.len() > MAX_OPERATIONS || layout.slot_count as usize > MAX_SLOTS {
        return Err(Refusal::Limit);
    }
    let slot = |v: ValueId| {
        layout
            .value_slots
            .get(v.0 as usize)
            .copied()
            .filter(|s| *s < layout.slot_count)
            .ok_or(Refusal::InvalidLayout)
    };
    let mut copies = Vec::new();
    let mut boxed_indices = Vec::new();
    for op in &block.operations {
        let destination = slot(op.result)?;
        match op.kind {
            OperationKind::EntryReceiver
            | OperationKind::EntryImplementation
            | OperationKind::EntryArgument { .. }
            | OperationKind::EntryCapture { .. } => {}
            OperationKind::Bind { source, .. } => {
                let source = slot(source)?;
                copies.push((destination, source));
                boxed_indices.extend([destination, source]);
            }
            _ => return Err(Refusal::ObservingOperation),
        }
    }
    let mut selector_store = None;
    let exit = match &block.terminator {
        Terminator::Return(v) => Exit::Return(slot(*v)?),
        Terminator::Continue(next) if (next.0 as usize) < program.blocks.len() => {
            Exit::Continue(next.0)
        }
        Terminator::Call {
            call_site,
            receiver,
            selector,
            arguments,
            ..
        }
        | Terminator::TailCall {
            call_site,
            receiver,
            selector,
            arguments,
            ..
        } => {
            if arguments.len() > MAX_OPERATIONS || selector.len() > 4096 {
                return Err(Refusal::Limit);
            }
            let window = layout.call_window_start.ok_or(Refusal::InvalidLayout)?;
            let end = window
                .checked_add(2)
                .and_then(|n| n.checked_add(arguments.len() as u32))
                .ok_or(Refusal::InvalidLayout)?;
            if end > layout.slot_count
                || arguments.len() > layout.call_window_argument_capacity as usize
            {
                return Err(Refusal::InvalidLayout);
            }
            let source = slot(*receiver)?;
            copies.push((window, source));
            boxed_indices.extend([window, source]);
            selector_store = Some((copies.len(), window + 1, selector.to_vec()));
            boxed_indices.push(window + 1);
            for (index, argument) in arguments.iter().enumerate() {
                let source = slot(*argument)?;
                let destination = window + 2 + index as u32;
                copies.push((destination, source));
                boxed_indices.extend([destination, source]);
            }
            if matches!(block.terminator, Terminator::Call { .. }) {
                Exit::Call(call_site.0)
            } else {
                Exit::TailCall(call_site.0)
            }
        }
        _ => return Err(Refusal::ExitShape),
    };
    let operations = copies.len() + usize::from(selector_store.is_some());
    if operations > MAX_OPERATIONS {
        return Err(Refusal::Limit);
    }
    if operations < 4 {
        return Err(Refusal::NoWork);
    }
    Ok(Plan {
        pc,
        slot_count: layout.slot_count,
        copies,
        boxed_indices,
        exit,
        selector_store,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Block, Expr, Statement, portable, resumable};
    #[test]
    fn native_region_read_availability_and_refusal_are_physical() {
        let names: Vec<_> = (0..100).map(|i| format!("alias{i}").into_bytes()).collect();
        for chained in [false, true] {
            let mut body: Vec<_> = names
                .iter()
                .enumerate()
                .map(|(i, name)| Statement::Let {
                    name,
                    value: Expr::Global(if chained && i > 0 {
                        &names[i - 1]
                    } else {
                        b"input"
                    }),
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
            let logical = resumable::lower_portable_block(&bytes).unwrap();
            let layout = resumable::color_slots(&logical).unwrap();
            assert_eq!(logical.blocks.len(), 2);
            for pc in 0..2 {
                let plan = compile(&logical, &layout, pc).unwrap();
                assert_eq!(plan.benefits_from_propagation(), chained);
                assert_eq!(plan.emit(false).1, plan.copies.len());
                assert_eq!(
                    plan.emit(true).1,
                    if chained { plan.copies.len() } else { 1 }
                );
                assert_eq!(
                    plan.boxed_indices,
                    plan.copies
                        .iter()
                        .flat_map(|&(d, s)| [d, s])
                        .collect::<Vec<_>>()
                );
            }
            let mut bad = layout.clone();
            bad.value_slots[0] = bad.slot_count;
            assert_eq!(compile(&logical, &bad, 0), Err(Refusal::InvalidLayout));
        }
        let bytes = portable::encode_block(
            &Block {
                params: vec![],
                body: vec![Statement::Expr(Expr::SmallInt(7))],
            },
            &[],
        )
        .unwrap();
        let logical = resumable::lower_portable_block(&bytes).unwrap();
        assert_eq!(
            compile(&logical, &resumable::color_slots(&logical).unwrap(), 0),
            Err(Refusal::ObservingOperation)
        );
    }
}
