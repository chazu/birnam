//! Experimental constructor-boundary continuation over baseline C emission.
use crate::{
    closure_elision::Plan,
    resumable::{LogicalProgram, PhysicalLayout},
    resumable_c::{self, Error, PortableBlockLiteralSpec},
};

pub fn emit_selected(
    symbol: u32,
    program: &LogicalProgram<'_>,
    layout: &PhysicalLayout,
    argc: u32,
    literals: &[PortableBlockLiteralSpec],
) -> Result<Vec<u8>, Error> {
    emit(
        symbol,
        program,
        layout,
        argc,
        literals,
        std::env::var_os("BIRNAM_DEV_CLOSURE_ELISION").is_some_and(|v| v == "1"),
    )
}

pub fn emit(
    symbol: u32,
    program: &LogicalProgram<'_>,
    layout: &PhysicalLayout,
    argc: u32,
    literals: &[PortableBlockLiteralSpec],
    enabled: bool,
) -> Result<Vec<u8>, Error> {
    let baseline = resumable_c::emit_step_method_with_portable_blocks(
        symbol, program, layout, argc, literals,
    )?;
    if !enabled {
        return Ok(baseline);
    }
    let Ok(plan) = Plan::derive(program, layout) else {
        return Ok(baseline);
    };
    if plan.continuation.is_none() {
        return Ok(baseline);
    }
    let spec = literals
        .iter()
        .find(|s| s.node_index == plan.node_index)
        .ok_or(Error::InvalidProgram)?;
    let id = plan.literal_value;
    let owner = if spec.retain_materialization_owner {
        "__birnam_materialization_owner"
    } else {
        "(void*)0"
    };
    let captures = if plan.capture_sources.is_empty() {
        "(void**)0".to_string()
    } else {
        format!("__birnam_block_captures_{id}")
    };
    let count = plan.capture_sources.len();
    let constructor = if spec.retain_materialization_owner {
        "birnam_portable_materialized_block_new_v2"
    } else {
        "birnam_portable_block_new_v1"
    };
    let original_owner = if spec.retain_materialization_owner {
        ",__birnam_materialization_owner"
    } else {
        ""
    };
    let original = format!(
        "void* __birnam_block_oop_{id}={constructor}(__birnam_block_vt_{id},__birnam_block_code_{id},{captures},{count}UL{original_owner});"
    );
    let replacement = format!(
        "unsigned int __birnam_closure_inlined_{id}=0U;void* __birnam_block_oop_{id}=birnam_portable_block_or_inline_v1(machine,(void*)&__birnam_resumable_step_{symbol},{}U,__birnam_block_vt_{id},__birnam_block_code_{id},{captures},{count}UL,{owner},{}U,&__birnam_closure_inlined_{id});if(__birnam_closure_inlined_{id})return __birnam_block_oop_{id};",
        plan.node_index,
        u32::from(spec.retain_materialization_owner)
    );
    let baseline = String::from_utf8(baseline).map_err(|_| Error::InvalidProgram)?;
    if baseline.matches(&original).count() != 1 {
        return Err(Error::InvalidProgram);
    }
    let mut output = b"extern void* birnam_portable_block_or_inline_v1(void*,void*,unsigned int,void*,void*,void**,unsigned long,void*,unsigned int,unsigned int*);\n".to_vec();
    output.extend_from_slice(baseline.replacen(&original, &replacement, 1).as_bytes());
    Ok(output)
}
