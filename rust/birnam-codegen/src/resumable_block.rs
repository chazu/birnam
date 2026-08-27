//! One indivisible BlockBody artifact description.

use birnam_artifact::stackless_selection::{
    Disposition, ImplementationKind, SelectionSpec, portable_implementation_identity,
    source_map_identity,
};
use birnam_artifact::{
    materialization_selection::{Disposition as PortableDisposition, Row, SubjectKind},
    portable_resumable_manifest,
};
use sha2::{Digest, Sha256};

use crate::resumable::{LayoutError, LowerError, color_slots, lower_portable_block};
use crate::resumable_c;
use crate::resumable_descriptor;
use crate::resumable_registration_c::{self, RegistrationSpec};
use crate::resumable_resolver_c::BlockVtableSpec;
use crate::{Block, Expr, Statement};

#[derive(Debug, PartialEq, Eq)]
pub struct CompiledBlockResumableV1 {
    pub descriptor: Vec<u8>,
    /// Function definitions and registration operations. The caller places
    /// these after the ordinary codegen runtime prelude and emits the shared
    /// registration prelude once per translation unit.
    pub c_source: Vec<u8>,
    pub symbol_index: u32,
    pub manifest_entry_index: u32,
    pub selector: Vec<u8>,
    pub portable_implementation_identity: u64,
    pub source_map_identity: u64,
    pub language_arity: u32,
    pub driver_argument_count: u32,
}

impl CompiledBlockResumableV1 {
    pub fn resolver_spec(&self) -> BlockVtableSpec<'_> {
        BlockVtableSpec {
            manifest_entry_index: self.manifest_entry_index,
            symbol_index: self.symbol_index,
            selector: &self.selector,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidSelection,
    QuantityOverflow,
    Logical(LowerError),
    Layout(LayoutError),
    Descriptor(resumable_descriptor::Error),
    C(resumable_c::Error),
    Registration(resumable_registration_c::Error),
    Portable(crate::portable::Error),
}

/// Compile one block body through logical IR, coloring,
/// descriptor-v2 construction, StepMethod/thunk emission, and registration C.
pub fn compile_block_v1<'a>(
    block: &'a Block<'a>,
    captures: &'a [&'a [u8]],
    authority: SelectionSpec<'a>,
    diagnostic_path: &[u8],
    source: &[u8],
    symbol_index: u32,
    manifest_entry_index: u32,
) -> Result<CompiledBlockResumableV1, Error> {
    compile_block_with_literals_v1(
        block,
        captures,
        authority,
        diagnostic_path,
        source,
        symbol_index,
        manifest_entry_index,
        &[],
    )
}

/// Compile one block body while binding every nested block literal to its
/// already-selected unit manifest row.
#[allow(clippy::too_many_arguments)]
pub fn compile_block_with_literals_v1<'a>(
    block: &'a Block<'a>,
    captures: &'a [&'a [u8]],
    authority: SelectionSpec<'a>,
    diagnostic_path: &[u8],
    source: &[u8],
    symbol_index: u32,
    manifest_entry_index: u32,
    block_literals: &[resumable_c::BlockLiteralSpec<'a>],
) -> Result<CompiledBlockResumableV1, Error> {
    let outer_bindings = captures
        .iter()
        .map(|name| name.to_vec())
        .collect::<Vec<_>>();
    let portable_program =
        crate::portable::encode_block(block, &outer_bindings).map_err(Error::Portable)?;
    let portable_block_literals =
        portable_block_literal_specs(block, &portable_program, block_literals)?;
    compile_portable_block_with_literals_v1(
        &portable_program,
        authority,
        diagnostic_path,
        source,
        symbol_index,
        manifest_entry_index,
        &portable_block_literals,
    )
}

/// Compile one admitted canonical Block program through the same resumable
/// backend used by source-authored Blocks. Source bytes are diagnostic
/// provenance only and are never consulted for lexical or literal meaning.
#[allow(clippy::too_many_arguments)]
pub fn compile_portable_block_with_literals_v1(
    portable_program: &[u8],
    authority: SelectionSpec<'_>,
    diagnostic_path: &[u8],
    source: &[u8],
    symbol_index: u32,
    manifest_entry_index: u32,
    block_literals: &[resumable_c::PortableBlockLiteralSpec],
) -> Result<CompiledBlockResumableV1, Error> {
    if authority.implementation_kind != ImplementationKind::BlockBody
        || authority.disposition != Disposition::ResumableRequired
    {
        return Err(Error::InvalidSelection);
    }
    let inspected = birnam_artifact::portable_program::inspect(portable_program)
        .map_err(|_| Error::Logical(LowerError::InvalidProgram))?;
    let language_arity = inspected.parameter_count;
    let program = lower_portable_block(portable_program).map_err(Error::Logical)?;
    let layout = color_slots(&program).map_err(Error::Layout)?;
    let driver_argument_count = u32::from(program.blocks.len() > 1);
    let descriptor = resumable_descriptor::compile_block_descriptor_v2(
        &program,
        &layout,
        authority,
        diagnostic_path,
        source,
        driver_argument_count,
    )
    .map_err(Error::Descriptor)?;
    let portable_implementation_identity =
        portable_implementation_identity(authority, 1).map_err(|_| Error::InvalidSelection)?;
    let source_map_identity =
        source_map_identity(authority, 1).map_err(|_| Error::InvalidSelection)?;
    let selector = block_selector(language_arity)?;

    let mut c_source = resumable_c::emit_entry_thunk(symbol_index, &selector);
    c_source.extend_from_slice(
        &resumable_c::emit_step_method_with_portable_blocks(
            symbol_index,
            &program,
            &layout,
            driver_argument_count,
            block_literals,
        )
        .map_err(Error::C)?,
    );
    c_source.extend_from_slice(
        &resumable_registration_c::emit_registration_v2(RegistrationSpec {
            symbol_index,
            manifest_entry_index,
            implementation_kind: ImplementationKind::BlockBody,
            descriptor: &descriptor,
            portable_implementation_identity,
            language_arity,
            driver_argument_count,
        })
        .map_err(Error::Registration)?,
    );

    Ok(CompiledBlockResumableV1 {
        descriptor,
        c_source,
        symbol_index,
        manifest_entry_index,
        selector,
        portable_implementation_identity,
        source_map_identity,
        language_arity,
        driver_argument_count,
    })
}

/// Compile one admitted 60L BlockBody row without constructing legacy source
/// selection identity. The generated entry and registration ABI are otherwise
/// identical to the source-authored resumable backend.
#[allow(clippy::too_many_arguments)]
pub fn compile_portable_materialization_block_v1(
    portable_program: &[u8],
    row: Row<'_>,
    symbol_index: u32,
    manifest_entry_index: u32,
    block_literals: &[resumable_c::PortableBlockLiteralSpec],
) -> Result<CompiledBlockResumableV1, Error> {
    if row.kind != SubjectKind::BlockBody
        || row.disposition != PortableDisposition::Resumable
        || row.semantic_digest != <[u8; 32]>::from(Sha256::digest(portable_program))
    {
        return Err(Error::InvalidSelection);
    }
    let inspected = birnam_artifact::portable_program::inspect(portable_program)
        .map_err(|_| Error::Logical(LowerError::InvalidProgram))?;
    if inspected.parameter_count != row.language_arity
        || inspected.capture_count != row.capture_count
    {
        return Err(Error::InvalidSelection);
    }
    let language_arity = inspected.parameter_count;
    let program = lower_portable_block(portable_program).map_err(Error::Logical)?;
    let layout = color_slots(&program).map_err(Error::Layout)?;
    let driver_argument_count = u32::from(program.blocks.len() > 1);
    let descriptor = resumable_descriptor::compile_portable_descriptor_v2(
        &program,
        &layout,
        row,
        driver_argument_count,
    )
    .map_err(Error::Descriptor)?;
    let portable_implementation_identity =
        portable_resumable_manifest::portable_implementation_identity(
            row,
            resumable_descriptor::LOWERING_VERSION_V1,
        )
        .map_err(|_| Error::InvalidSelection)?;
    let source_map_identity = 0;
    let selector = block_selector(language_arity)?;

    let mut c_source = resumable_c::emit_entry_thunk(symbol_index, &selector);
    c_source.extend_from_slice(
        &resumable_c::emit_step_method_with_portable_blocks(
            symbol_index,
            &program,
            &layout,
            driver_argument_count,
            block_literals,
        )
        .map_err(Error::C)?,
    );
    c_source.extend_from_slice(
        &resumable_registration_c::emit_registration_v2(RegistrationSpec {
            symbol_index,
            manifest_entry_index,
            implementation_kind: ImplementationKind::BlockBody,
            descriptor: &descriptor,
            portable_implementation_identity,
            language_arity,
            driver_argument_count,
        })
        .map_err(Error::Registration)?,
    );

    Ok(CompiledBlockResumableV1 {
        descriptor,
        c_source,
        symbol_index,
        manifest_entry_index,
        selector,
        portable_implementation_identity,
        source_map_identity,
        language_arity,
        driver_argument_count,
    })
}

fn portable_block_literal_specs<'a>(
    block: &'a Block<'a>,
    portable_program: &[u8],
    source_specs: &[resumable_c::BlockLiteralSpec<'a>],
) -> Result<Vec<resumable_c::PortableBlockLiteralSpec>, Error> {
    let inspected = birnam_artifact::portable_program::inspect(portable_program)
        .map_err(|_| Error::C(resumable_c::Error::InvalidProgram))?;
    let mut source_occurrences = Vec::new();
    for statement in &block.body {
        let expression = match statement {
            Statement::Let { value, .. } | Statement::Expr(value) => value,
        };
        collect_direct_block_literals(expression, &mut source_occurrences);
    }
    let portable_occurrences = (0..inspected.node_count)
        .filter_map(|node_index| {
            inspected
                .node(node_index as usize)
                .filter(|node| {
                    node.semantic_kind()
                        == Some(birnam_artifact::portable_program::NodeKind::BlockLiteral)
                })
                .map(|node| (node_index, node.data))
        })
        .collect::<Vec<_>>();
    if source_occurrences.len() != portable_occurrences.len() {
        return Err(Error::C(resumable_c::Error::InvalidProgram));
    }
    source_occurrences
        .into_iter()
        .zip(portable_occurrences)
        .map(|(source_block, (node_index, nested_program))| {
            let mut matching = source_specs
                .iter()
                .filter(|spec| core::ptr::eq(spec.block, source_block));
            let spec = matching
                .next()
                .ok_or(Error::C(resumable_c::Error::Unsupported))?;
            if matching.next().is_some() || spec.portable_program.as_slice() != nested_program {
                return Err(Error::C(resumable_c::Error::InvalidProgram));
            }
            Ok(resumable_c::PortableBlockLiteralSpec {
                node_index,
                disposition: spec.disposition,
                portable_program: spec.portable_program.clone(),
                retain_materialization_owner: false,
            })
        })
        .collect()
}

fn collect_direct_block_literals<'a>(expression: &'a Expr<'a>, output: &mut Vec<&'a Block<'a>>) {
    match expression {
        Expr::Block(block) => output.push(block),
        Expr::Send { receiver, args, .. } => {
            collect_direct_block_literals(receiver, output);
            for argument in args {
                collect_direct_block_literals(argument, output);
            }
        }
        _ => {}
    }
}

/// Compatibility wrapper for capture-free block bodies.
pub fn compile_capture_free_block_v1(
    block: &Block<'_>,
    authority: SelectionSpec<'_>,
    diagnostic_path: &[u8],
    source: &[u8],
    symbol_index: u32,
    manifest_entry_index: u32,
) -> Result<CompiledBlockResumableV1, Error> {
    compile_block_v1(
        block,
        &[],
        authority,
        diagnostic_path,
        source,
        symbol_index,
        manifest_entry_index,
    )
}

fn block_selector(language_arity: u32) -> Result<Vec<u8>, Error> {
    if language_arity == 0 {
        return Ok(b"value".to_vec());
    }
    let count = usize::try_from(language_arity).map_err(|_| Error::QuantityOverflow)?;
    let capacity = count.checked_mul(6).ok_or(Error::QuantityOverflow)?;
    let mut selector = Vec::with_capacity(capacity);
    for _ in 0..count {
        selector.extend_from_slice(b"value:");
    }
    Ok(selector)
}

#[cfg(test)]
mod tests {
    use birnam_artifact::frame_descriptor;
    use birnam_artifact::portable_program::{NodeSpec, ProgramSpec, StatementSpec, encode};
    use birnam_artifact::resumable_manifest::{self, ManifestEntrySpec};
    use birnam_artifact::stackless_selection::{
        RootKind, encode_artifact_selection, inspect_artifact_selection,
    };

    use crate::{Expr, Statement};

    use super::*;

    #[test]
    fn source_less_portable_program_compiles_without_source_reconstruction() {
        let bytes = encode(ProgramSpec {
            parameters: &[b"value"],
            captures: &[],
            lets: &[],
            statements: &[StatementSpec::Expression { root_node: 0 }],
            nodes: &[NodeSpec::FloatBits(0x7ff8_0000_0000_1234)],
        })
        .unwrap();
        let authority = SelectionSpec {
            source_revision: [0x71; 32],
            root_kind: RootKind::NonMethodProgram,
            root_index: 0,
            path: &[0],
            implementation_kind: ImplementationKind::BlockBody,
            disposition: Disposition::ResumableRequired,
        };

        let compiled =
            compile_portable_block_with_literals_v1(&bytes, authority, b"", b"", 7, 0, &[])
                .unwrap();
        assert_eq!(compiled.language_arity, 1);
        assert!(
            compiled
                .c_source
                .windows(b"0x7ff8000000001234ULL".len())
                .any(|window| window == b"0x7ff8000000001234ULL")
        );
    }

    #[test]
    fn materialized_nested_block_uses_the_traced_unit_owner_carrier() {
        use birnam_artifact::{
            code_binding_inventory, materialization_selection, portable_inventory,
        };

        let nested = encode(ProgramSpec {
            parameters: &[],
            captures: &[b"held"],
            lets: &[],
            statements: &[StatementSpec::Expression { root_node: 0 }],
            nodes: &[NodeSpec::LexicalRead(0)],
        })
        .unwrap();
        let outer = encode(ProgramSpec {
            parameters: &[b"held"],
            captures: &[],
            lets: &[],
            statements: &[StatementSpec::Expression { root_node: 0 }],
            nodes: &[NodeSpec::BlockLiteral {
                program: &nested,
                captures: &[0],
            }],
        })
        .unwrap();
        let programs = portable_inventory::encode_closure(&[&outer]).unwrap();
        let inventory = portable_inventory::inspect(&programs).unwrap();
        let outer_index = (0..inventory.program_count as usize)
            .find(|index| inventory.program(*index) == Some(outer.as_slice()))
            .unwrap();
        let nested_index = (0..inventory.program_count as usize)
            .find(|index| inventory.program(*index) == Some(nested.as_slice()))
            .unwrap();
        let bindings = code_binding_inventory::encode(&[]).unwrap();
        let selection_bytes = materialization_selection::encode(
            &programs,
            &bindings,
            &[],
            &vec![
                materialization_selection::Disposition::Resumable;
                inventory.program_count as usize
            ],
        )
        .unwrap();
        let selection =
            materialization_selection::inspect(&selection_bytes, &programs, &bindings).unwrap();
        let compiled = compile_portable_materialization_block_v1(
            &outer,
            selection.row(outer_index).unwrap(),
            outer_index as u32,
            outer_index as u32,
            &[resumable_c::PortableBlockLiteralSpec {
                node_index: 0,
                disposition: resumable_c::BlockLiteralDisposition::Resumable {
                    manifest_entry_index: nested_index as u32,
                },
                portable_program: nested,
                retain_materialization_owner: true,
            }],
        )
        .unwrap();
        let c = String::from_utf8(compiled.c_source).unwrap();
        assert!(c.contains("birnam_portable_materialized_block_new_v2"));
        assert!(c.contains("__birnam_materialization_owner"));
        assert!(!c.contains("=birnam_portable_block_new_v1"));
        assert!(
            frame_descriptor::inspect_portable(
                &compiled.descriptor,
                selection.row(outer_index).unwrap()
            )
            .is_ok()
        );
    }

    #[test]
    fn canonical_node_occurrences_disambiguate_byte_equal_nested_blocks() {
        let nested = || Block {
            params: vec![],
            body: vec![Statement::Expr(Expr::SmallInt(7))],
        };
        let block = Block {
            params: vec![],
            body: vec![
                Statement::Expr(Expr::Block(Box::new(nested()))),
                Statement::Expr(Expr::Block(Box::new(nested()))),
            ],
        };
        let first = match &block.body[0] {
            Statement::Expr(Expr::Block(block)) => block.as_ref(),
            _ => unreachable!(),
        };
        let second = match &block.body[1] {
            Statement::Expr(Expr::Block(block)) => block.as_ref(),
            _ => unreachable!(),
        };
        let nested_program = crate::portable::encode_block(first, &[]).unwrap();
        assert_eq!(
            nested_program,
            crate::portable::encode_block(second, &[]).unwrap()
        );
        let literals = [
            resumable_c::BlockLiteralSpec {
                block: first,
                disposition: resumable_c::BlockLiteralDisposition::Resumable {
                    manifest_entry_index: 11,
                },
                portable_program: nested_program.clone(),
            },
            resumable_c::BlockLiteralSpec {
                block: second,
                disposition: resumable_c::BlockLiteralDisposition::Synchronous {
                    function_index: 4,
                    language_arity: 0,
                },
                portable_program: nested_program,
            },
        ];
        let authority = SelectionSpec {
            source_revision: [0x72; 32],
            root_kind: RootKind::NonMethodProgram,
            root_index: 0,
            path: &[0],
            implementation_kind: ImplementationKind::BlockBody,
            disposition: Disposition::ResumableRequired,
        };
        let compiled =
            compile_block_with_literals_v1(&block, &[], authority, b"", b"", 9, 0, &literals)
                .unwrap();
        let c = String::from_utf8(compiled.c_source).unwrap();
        assert!(c.contains("__birnam_resolve_block_vtable_v1(11U)"));
        assert!(c.contains("(void*)&__blk4"));
    }

    #[test]
    fn block_artifact_is_coherent_across_descriptor_manifest_and_c() {
        let revision = [0x41; 32];
        let path = [2, 1];
        let authority = SelectionSpec {
            source_revision: revision,
            root_kind: RootKind::NonMethodProgram,
            root_index: 0,
            path: &path,
            implementation_kind: ImplementationKind::BlockBody,
            disposition: Disposition::ResumableRequired,
        };
        let block = Block {
            params: vec![b"x"],
            body: vec![Statement::Expr(Expr::Global(b"x"))],
        };
        let compiled =
            compile_capture_free_block_v1(&block, authority, b"module.birnam", b"[ :x | x ]", 3, 0)
                .unwrap();
        let descriptor = frame_descriptor::inspect(&compiled.descriptor, authority).unwrap();
        assert_eq!(
            descriptor.metadata().portable_implementation_identity,
            compiled.portable_implementation_identity
        );
        assert_eq!(compiled.selector, b"value:");

        let selection_bytes = encode_artifact_selection(&[revision], &[authority]).unwrap();
        let selection = inspect_artifact_selection(&selection_bytes).unwrap();
        let entries = [ManifestEntrySpec {
            selection: authority,
            descriptor: &compiled.descriptor,
            selector: &compiled.selector,
        }];
        let manifest_bytes =
            resumable_manifest::materialize([0x52; 32], selection, &entries).unwrap();
        let manifest = resumable_manifest::inspect(&manifest_bytes, [0x52; 32]).unwrap();
        assert!(resumable_manifest::require_artifact(manifest, selection, &entries).is_ok());

        let c = String::from_utf8(compiled.c_source.clone()).unwrap();
        assert!(c.contains("__birnam_resumable_thunk_3"));
        assert!(c.contains("__birnam_resumable_step_3"));
        assert!(c.contains("__birnam_resumable_prepare_3"));
        assert!(c.contains("e.manifest_entry_index=0U"));
        let resolver =
            crate::resumable_resolver_c::emit_block_vtable_resolver_v1(&[compiled.resolver_spec()])
                .unwrap();
        assert!(
            resolver
                .windows(b"case 0U:".len())
                .any(|window| window == b"case 0U:")
        );
    }

    #[test]
    fn synchronous_selection_cannot_silently_compile_as_resumable() {
        let authority = SelectionSpec {
            source_revision: [0x11; 32],
            root_kind: RootKind::NonMethodProgram,
            root_index: 0,
            path: &[0],
            implementation_kind: ImplementationKind::BlockBody,
            disposition: Disposition::Synchronous,
        };
        let block = Block {
            params: vec![],
            body: vec![Statement::Expr(Expr::Nil)],
        };
        assert_eq!(
            compile_capture_free_block_v1(&block, authority, b"m", b"[]", 0, 0),
            Err(Error::InvalidSelection)
        );
    }

    #[test]
    fn captured_values_receive_canonical_entry_slots_and_feed_step_code() {
        let revision = [0x61; 32];
        let path = [0];
        let authority = SelectionSpec {
            source_revision: revision,
            root_kind: RootKind::MethodOperation,
            root_index: 2,
            path: &path,
            implementation_kind: ImplementationKind::BlockBody,
            disposition: Disposition::ResumableRequired,
        };
        let block = Block {
            params: vec![b"x"],
            body: vec![Statement::Expr(Expr::Global(b"captured"))],
        };
        let compiled = compile_block_v1(
            &block,
            &[b"captured"],
            authority,
            b"capture.birnam",
            b"[:x | captured]",
            8,
            0,
        )
        .unwrap();
        let descriptor = frame_descriptor::inspect(&compiled.descriptor, authority).unwrap();
        assert_eq!(descriptor.capture_count(), 1);
        // receiver=0, argument=1, first capture=2
        assert_eq!(descriptor.capture_slot(0), Some(2));
        assert_eq!(descriptor.metadata().slot_count, 3);
        let c = String::from_utf8(compiled.c_source).unwrap();
        assert!(c.contains("birnam_intern(\"returnAt:\")"));
        assert!(c.contains("MKV(2L)"));
    }
}
