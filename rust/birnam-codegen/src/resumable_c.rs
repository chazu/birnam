//! Deterministic Method-shaped StepMethod emission for colored resumable IR.
//!
//! Generated code accesses Frame state only through the late-bound Machine
//! protocol. It never embeds a Frame pointer or dispatch result. Captures are
//! copied into descriptor-declared entry slots by the Runtime; nested block-
//! object construction remains a separate lowering boundary.

use crate::resumable::{
    Literal, LogicalProgram, OperationKind, PhysicalLayout, Terminator, ValueId,
};
use crate::{Block, Emitter};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidProgram,
    Unsupported,
    QuantityOverflow,
}

#[derive(Clone, Copy, Debug)]
pub struct BlockLiteralSpec<'a> {
    pub block: &'a Block<'a>,
    pub disposition: BlockLiteralDisposition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockLiteralDisposition {
    Resumable {
        manifest_entry_index: u32,
    },
    Synchronous {
        function_index: u32,
        language_arity: u32,
    },
}

/// Emit the unique ordinary Method carrier installed in a vtable.
///
/// Normal dispatch classifies this address and enters the registered
/// resumable without invoking the carrier. A raw Method invocation falls back
/// through ordinary `birnam_send`, where the same resolver classifies it.
pub fn emit_entry_thunk(symbol_index: u32, selector: &[u8]) -> Vec<u8> {
    let mut output = Emitter::with_capacity(256);
    output.push(b"void* __birnam_resumable_thunk_");
    decimal(&mut output, symbol_index);
    output.push(b"(void* receiver, void* const* args, unsigned long n){\nreturn birnam_send(receiver,birnam_intern(\"");
    output.push_c_string_body(selector);
    output.push(b"\"),(void**)args,(long)n);\n}\n");
    output.finish()
}

/// Emit one exact-ABI StepMethod definition.
///
/// The returned function is named `__birnam_resumable_step_<symbol_index>`.
/// Its argument spelling is the accepted Method ABI on S2's sole native
/// target: Oop receiver, borrowed const Oop array, and 64-bit `size_t`.
pub fn emit_step_method(
    symbol_index: u32,
    program: &LogicalProgram<'_>,
    layout: &PhysicalLayout,
    driver_argument_count: u32,
) -> Result<Vec<u8>, Error> {
    emit_step_method_with_blocks(symbol_index, program, layout, driver_argument_count, &[])
}

pub fn emit_step_method_with_blocks<'a>(
    symbol_index: u32,
    program: &LogicalProgram<'a>,
    layout: &PhysicalLayout,
    driver_argument_count: u32,
    block_literals: &[BlockLiteralSpec<'a>],
) -> Result<Vec<u8>, Error> {
    validate(program, layout, driver_argument_count, block_literals)?;

    let mut output = Emitter::with_capacity(4096);
    output.push(b"void* __birnam_resumable_step_");
    decimal(&mut output, symbol_index);
    output.push(b"(void* machine, void* const* args, unsigned long n){\n");
    output.push(b"if(n!=");
    decimal(&mut output, driver_argument_count);
    output.push(b"UL)return birnam_nil();\n");
    if driver_argument_count == 1 {
        output.push(b"long pc=birnam_smallIntValue(args[0]);\n");
        output.push(b"switch(pc){\n");
    } else {
        output.push(b"(void)args;\n");
    }

    for block in &program.blocks {
        if driver_argument_count == 1 {
            output.push(b"case ");
            decimal(&mut output, block.pc.0);
            output.push(b"L:{\n");
        } else {
            output.push(b"{\n");
        }

        for operation in &block.operations {
            match &operation.kind {
                OperationKind::EntryReceiver => {}
                OperationKind::EntryImplementation => {}
                OperationKind::EntryArgument { .. } => {}
                OperationKind::EntryCapture { .. } => {}
                OperationKind::Literal(literal) => {
                    let destination = value_slot(layout, operation.result)?;
                    emit_store_prefix(&mut output, operation.result.0, destination);
                    emit_literal(&mut output, literal);
                    emit_store_suffix(&mut output, operation.result.0);
                }
                OperationKind::GlobalLookup(name) => {
                    let destination = value_slot(layout, operation.result)?;
                    emit_store_prefix(&mut output, operation.result.0, destination);
                    output.emit_global(name);
                    emit_store_suffix(&mut output, operation.result.0);
                }
                OperationKind::BlockLiteral { block, captures } => {
                    let spec = block_literals
                        .iter()
                        .find(|spec| core::ptr::eq(spec.block, *block))
                        .ok_or(Error::Unsupported)?;
                    emit_block_literal(
                        &mut output,
                        layout,
                        operation.result,
                        captures,
                        spec.disposition,
                    )?;
                }
                OperationKind::Bind { source, .. } => {
                    let destination = value_slot(layout, operation.result)?;
                    emit_store_prefix(&mut output, operation.result.0, destination);
                    emit_get(&mut output, value_slot(layout, *source)?);
                    emit_store_suffix(&mut output, operation.result.0);
                }
            }
        }

        match &block.terminator {
            Terminator::Call {
                call_site,
                receiver,
                selector,
                arguments,
                ..
            } => {
                emit_call_staging(
                    &mut output,
                    layout,
                    *receiver,
                    selector,
                    arguments,
                    program.value_count,
                )?;
                emit_publish(&mut output, b"callSite:", call_site.0);
            }
            Terminator::TailCall {
                call_site,
                receiver,
                selector,
                arguments,
                ..
            } => {
                emit_call_staging(
                    &mut output,
                    layout,
                    *receiver,
                    selector,
                    arguments,
                    program.value_count,
                )?;
                emit_publish(&mut output, b"tailCallSite:", call_site.0);
            }
            Terminator::Return(value) => {
                emit_publish(&mut output, b"returnAt:", value_slot(layout, *value)?);
            }
            Terminator::Continue(successor) => {
                emit_publish(&mut output, b"continueAt:", successor.0);
            }
        }
        output.push(b"}\n");
    }

    if driver_argument_count == 1 {
        output.push(b"default:return birnam_nil();\n}\n");
    }
    // Every admitted terminator returns. This is only a C compiler fallback
    // for a corrupt control value at the trusted Runtime boundary.
    output.push(b"return birnam_nil();\n}\n");
    Ok(output.finish())
}

fn validate(
    program: &LogicalProgram<'_>,
    layout: &PhysicalLayout,
    driver_argument_count: u32,
    block_literals: &[BlockLiteralSpec<'_>],
) -> Result<(), Error> {
    if program.blocks.is_empty()
        || program.blocks.len() != layout.pc_value_locations.len()
        || program.value_count as usize != layout.value_slots.len()
        || program.entry.0 != 0
        || layout.entry_argument_count != program.argument_count
        || layout.entry_capture_slots.len() != program.capture_count as usize
        || driver_argument_count > 1
        || (driver_argument_count == 0 && program.blocks.len() != 1)
    {
        return Err(Error::InvalidProgram);
    }
    let mut expected_call_site = 0u32;
    for (index, block) in program.blocks.iter().enumerate() {
        if block.pc.0 as usize != index {
            return Err(Error::InvalidProgram);
        }
        for operation in &block.operations {
            let slot = value_slot(layout, operation.result)?;
            if slot >= layout.slot_count {
                return Err(Error::InvalidProgram);
            }
            match operation.kind {
                OperationKind::EntryReceiver
                    if block.pc.0 != 0 || slot != layout.entry_receiver_slot =>
                {
                    return Err(Error::InvalidProgram);
                }
                OperationKind::EntryImplementation
                    if block.pc.0 != 0 || Some(slot) != layout.implementation_slot =>
                {
                    return Err(Error::InvalidProgram);
                }
                OperationKind::EntryArgument { index, .. }
                    if block.pc.0 != 0
                        || index >= program.argument_count
                        || slot
                            != layout
                                .entry_argument_start
                                .checked_add(index)
                                .ok_or(Error::QuantityOverflow)? =>
                {
                    return Err(Error::InvalidProgram);
                }
                OperationKind::EntryCapture { index, .. }
                    if block.pc.0 != 0
                        || index >= program.capture_count
                        || layout.entry_capture_slots.get(index as usize).copied()
                            != Some(slot) =>
                {
                    return Err(Error::InvalidProgram);
                }
                OperationKind::BlockLiteral {
                    block,
                    ref captures,
                } => {
                    if block_literals
                        .iter()
                        .filter(|spec| core::ptr::eq(spec.block, block))
                        .count()
                        != 1
                    {
                        return Err(Error::Unsupported);
                    }
                    for capture in captures {
                        value_slot(layout, *capture)?;
                    }
                }
                OperationKind::Bind { source, .. } => {
                    value_slot(layout, source)?;
                }
                _ => {}
            }
        }
        match &block.terminator {
            Terminator::Call {
                call_site,
                receiver,
                arguments,
                result,
                successor,
                ..
            } => {
                if call_site.0 != expected_call_site || successor.0 as usize >= program.blocks.len()
                {
                    return Err(Error::InvalidProgram);
                }
                expected_call_site = expected_call_site
                    .checked_add(1)
                    .ok_or(Error::QuantityOverflow)?;
                validate_call_values(layout, *receiver, arguments)?;
                value_slot(layout, *result)?;
            }
            Terminator::TailCall {
                call_site,
                receiver,
                arguments,
                ..
            } => {
                if call_site.0 != expected_call_site {
                    return Err(Error::InvalidProgram);
                }
                expected_call_site = expected_call_site
                    .checked_add(1)
                    .ok_or(Error::QuantityOverflow)?;
                validate_call_values(layout, *receiver, arguments)?;
            }
            Terminator::Return(value) => {
                value_slot(layout, *value)?;
            }
            Terminator::Continue(successor) => {
                if successor.0 as usize >= program.blocks.len() {
                    return Err(Error::InvalidProgram);
                }
            }
        }
    }
    if expected_call_site != program.call_site_count {
        return Err(Error::InvalidProgram);
    }
    Ok(())
}

fn emit_block_literal(
    output: &mut Emitter,
    layout: &PhysicalLayout,
    result: ValueId,
    captures: &[ValueId],
    disposition: BlockLiteralDisposition,
) -> Result<(), Error> {
    let destination = value_slot(layout, result)?;
    emit_store_prefix(output, result.0, destination);
    output.push(b"({void* __birnam_block_vt_");
    decimal(output, result.0);
    output.push(b"=");
    match disposition {
        BlockLiteralDisposition::Resumable {
            manifest_entry_index,
        } => {
            output.push(b"__birnam_resolve_block_vtable_v1(");
            decimal(output, manifest_entry_index);
            output.push(b"U)");
        }
        BlockLiteralDisposition::Synchronous {
            function_index,
            language_arity,
        } => {
            output.push(b"({extern void* __blk");
            decimal(output, function_index);
            output.push(b"(void*,void**,long);static void* v=0;if(!v){v=birnam_proto(birnam_blockVt());if(v){birnam_method(v,birnam_intern(\"");
            if language_arity == 0 {
                output.push(b"value");
            } else {
                for _ in 0..language_arity {
                    output.push(b"value:");
                }
            }
            output.push(b"\"),(void*)&__blk");
            decimal(output, function_index);
            output.push(b");birnam_layoutAllOops(v);}}v;})");
        }
    }
    output.push(b";if(!__birnam_block_vt_");
    decimal(output, result.0);
    output.push(b")return birnam_nil();void* __birnam_block_oop_");
    decimal(output, result.0);
    output.push(b"=birnam_alloc(__birnam_block_vt_");
    decimal(output, result.0);
    output.push(b",");
    output.push(captures.len().to_string().as_bytes());
    output.push(b"L*sizeof(void*));if(!__birnam_block_oop_");
    decimal(output, result.0);
    output.push(b"||__birnam_block_oop_");
    decimal(output, result.0);
    output.push(b"==birnam_nil())return birnam_nil();");
    for (index, capture) in captures.iter().enumerate() {
        output.push(b"((void**)__birnam_block_oop_");
        decimal(output, result.0);
        output.push(b")[");
        output.push(index.to_string().as_bytes());
        output.push(b"]=");
        emit_get(output, value_slot(layout, *capture)?);
        output.push(b";");
    }
    output.push(b"__birnam_block_oop_");
    decimal(output, result.0);
    output.push(b";})");
    emit_store_suffix(output, result.0);
    Ok(())
}

fn validate_call_values(
    layout: &PhysicalLayout,
    receiver: ValueId,
    arguments: &[ValueId],
) -> Result<(), Error> {
    value_slot(layout, receiver)?;
    for argument in arguments {
        value_slot(layout, *argument)?;
    }
    let capacity = usize::try_from(layout.call_window_argument_capacity)
        .map_err(|_| Error::QuantityOverflow)?;
    if layout.call_window_start.is_none() || arguments.len() > capacity {
        return Err(Error::InvalidProgram);
    }
    Ok(())
}

fn value_slot(layout: &PhysicalLayout, value: ValueId) -> Result<u32, Error> {
    layout
        .value_slots
        .get(value.0 as usize)
        .copied()
        .filter(|slot| *slot < layout.slot_count)
        .ok_or(Error::InvalidProgram)
}

fn emit_literal(output: &mut Emitter, literal: &Literal<'_>) {
    match literal {
        Literal::SmallInt(value) => output.emit_small_int(*value),
        Literal::SmallIntSource(source) => output.emit_small_int_literal(source),
        Literal::Float(source) => output.emit_float(source),
        Literal::String(bytes) => output.emit_string(bytes),
        Literal::StringSource(source) => output.emit_string_literal(source),
        Literal::Symbol(name) => output.emit_symbol(name),
        Literal::Nil => output.emit_nil(),
        Literal::True => output.emit_true(),
        Literal::False => output.emit_false(),
    }
}

/// Prepare every potentially allocating store operand before evaluating the
/// new value. The only operation after value production is the rooted send.
fn emit_store_prefix(output: &mut Emitter, id: u32, slot: u32) {
    output.push(b"void* __birnam_put_selector_");
    decimal(output, id);
    output.push(b"=birnam_intern(\"oopAt:put:\");\nvoid* __birnam_put_slot_");
    decimal(output, id);
    output.push(b"=MKV(");
    decimal(output, slot);
    output.push(b"L);\nvoid* __birnam_value_");
    decimal(output, id);
    output.push(b"=");
}

fn emit_store_suffix(output: &mut Emitter, id: u32) {
    output.push(b";\nvoid* __birnam_put_args_");
    decimal(output, id);
    output.push(b"[]={__birnam_put_slot_");
    decimal(output, id);
    output.push(b",__birnam_value_");
    decimal(output, id);
    output.push(b"};\n(void)birnam_send(machine,__birnam_put_selector_");
    decimal(output, id);
    output.push(b",__birnam_put_args_");
    decimal(output, id);
    output.push(b",2L);\n");
}

fn emit_get(output: &mut Emitter, slot: u32) {
    output.push(b"birnam_send(machine,birnam_intern(\"oopAt:\"),(void*[]){MKV(");
    decimal(output, slot);
    output.push(b"L)},1L)");
}

fn emit_call_staging(
    output: &mut Emitter,
    layout: &PhysicalLayout,
    receiver: ValueId,
    selector: &[u8],
    arguments: &[ValueId],
    first_staging_id: u32,
) -> Result<(), Error> {
    let window = layout.call_window_start.ok_or(Error::InvalidProgram)?;
    emit_store_prefix(output, first_staging_id, window);
    emit_get(output, value_slot(layout, receiver)?);
    emit_store_suffix(output, first_staging_id);

    let selector_id = first_staging_id
        .checked_add(1)
        .ok_or(Error::QuantityOverflow)?;
    let selector_slot = window.checked_add(1).ok_or(Error::QuantityOverflow)?;
    emit_store_prefix(output, selector_id, selector_slot);
    output.emit_symbol(selector);
    emit_store_suffix(output, selector_id);

    for (index, argument) in arguments.iter().enumerate() {
        let index = u32::try_from(index).map_err(|_| Error::QuantityOverflow)?;
        let id = first_staging_id
            .checked_add(2)
            .and_then(|id| id.checked_add(index))
            .ok_or(Error::QuantityOverflow)?;
        let slot = window
            .checked_add(2)
            .and_then(|slot| slot.checked_add(index))
            .ok_or(Error::QuantityOverflow)?;
        emit_store_prefix(output, id, slot);
        emit_get(output, value_slot(layout, *argument)?);
        emit_store_suffix(output, id);
    }
    Ok(())
}

fn emit_publish(output: &mut Emitter, selector: &[u8], operand: u32) {
    output.push(b"return birnam_send(machine,birnam_intern(\"");
    output.push_c_string_body(selector);
    output.push(b"\"),(void*[]){MKV(");
    decimal(output, operand);
    output.push(b"L)},1L);\n");
}

fn decimal(output: &mut Emitter, value: u32) {
    output.push(value.to_string().as_bytes());
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use crate::resumable::{color_slots, lower_block};
    use crate::{Block, Expr, Statement};

    use super::*;

    #[test]
    fn emits_machine_only_stepmethod_with_ordered_call_staging() {
        let block = Block {
            params: vec![b"self", b"x"],
            body: vec![Statement::Expr(Expr::Send {
                receiver: Box::new(Expr::Global(b"x")),
                selector: Cow::Borrowed(b"foo:"),
                args: vec![Expr::SmallInt(7)],
                source: b"[x foo: 7]",
            })],
        };
        let program = lower_block(&block).unwrap();
        let layout = color_slots(&program).unwrap();
        let source = String::from_utf8(emit_step_method(9, &program, &layout, 1).unwrap()).unwrap();

        assert!(source.starts_with(
            "void* __birnam_resumable_step_9(void* machine, void* const* args, unsigned long n)"
        ));
        assert!(source.contains("long pc=birnam_smallIntValue(args[0]);"));
        let first_stage = program.value_count;
        let receiver = source
            .find(&format!("__birnam_put_selector_{first_stage}="))
            .unwrap();
        let selector = source
            .find(&format!("__birnam_put_selector_{}=", first_stage + 1))
            .unwrap();
        let argument = source
            .find(&format!("__birnam_put_selector_{}=", first_stage + 2))
            .unwrap();
        let publish = source.find("birnam_intern(\"tailCallSite:\")").unwrap();
        assert!(receiver < selector && selector < argument && argument < publish);
        assert!(!source.contains("birnam_send(x"));
    }

    #[test]
    fn emits_single_pc_zero_driver_argument_method() {
        let block = Block {
            params: vec![b"self"],
            body: vec![Statement::Expr(Expr::SmallInt(7))],
        };
        let program = lower_block(&block).unwrap();
        let layout = color_slots(&program).unwrap();
        let source = String::from_utf8(emit_step_method(1, &program, &layout, 0).unwrap()).unwrap();
        assert!(source.contains("if(n!=0UL)return birnam_nil();"));
        assert!(source.contains("(void)args;"));
        assert!(!source.contains("switch(pc)"));
        assert!(source.contains("birnam_intern(\"returnAt:\")"));
    }

    #[test]
    fn entry_thunk_reenters_through_the_same_late_bound_selector() {
        assert_eq!(
            String::from_utf8(emit_entry_thunk(12, b"value:value:")).unwrap(),
            "void* __birnam_resumable_thunk_12(void* receiver, void* const* args, unsigned long n){\nreturn birnam_send(receiver,birnam_intern(\"value:value:\"),(void**)args,(long)n);\n}\n"
        );
    }

    #[test]
    fn nested_block_construction_requires_and_uses_an_exact_manifest_row() {
        let nested = Block {
            params: vec![],
            body: vec![Statement::Expr(Expr::Global(b"self"))],
        };
        let block = Block {
            params: vec![b"self"],
            body: vec![Statement::Expr(Expr::Block(Box::new(nested)))],
        };
        let program = lower_block(&block).unwrap();
        let layout = color_slots(&program).unwrap();
        assert_eq!(
            emit_step_method(0, &program, &layout, 0),
            Err(Error::Unsupported)
        );
        let emitted = String::from_utf8(
            emit_step_method_with_blocks(
                0,
                &program,
                &layout,
                0,
                &[BlockLiteralSpec {
                    block: match &block.body[0] {
                        Statement::Expr(Expr::Block(block)) => block,
                        _ => unreachable!(),
                    },
                    disposition: BlockLiteralDisposition::Resumable {
                        manifest_entry_index: 7,
                    },
                }],
            )
            .unwrap(),
        )
        .unwrap();
        assert!(emitted.contains("__birnam_resolve_block_vtable_v1(7U)"));
        assert!(emitted.contains("birnam_alloc(__birnam_block_vt_"));
        assert!(emitted.contains("((void**)__birnam_block_oop_"));
        assert!(emitted.contains("birnam_intern(\"oopAt:\")"));
    }
}
