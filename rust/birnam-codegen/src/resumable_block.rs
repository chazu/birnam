//! One indivisible BlockBody artifact description.

use birnam_artifact::stackless_selection::{
    Disposition, ImplementationKind, SelectionSpec, portable_implementation_identity,
    source_map_identity,
};

use crate::Block;
use crate::resumable::{LayoutError, LowerError, color_slots, lower_block_with_captures};
use crate::resumable_c;
use crate::resumable_descriptor;
use crate::resumable_registration_c::{self, RegistrationSpec};
use crate::resumable_resolver_c::BlockVtableSpec;

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
    if authority.implementation_kind != ImplementationKind::BlockBody
        || authority.disposition != Disposition::ResumableRequired
    {
        return Err(Error::InvalidSelection);
    }
    let language_arity = u32::try_from(block.params.len()).map_err(|_| Error::QuantityOverflow)?;
    let program = lower_block_with_captures(block, captures).map_err(Error::Logical)?;
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
        &resumable_c::emit_step_method_with_blocks(
            symbol_index,
            &program,
            &layout,
            driver_argument_count,
            block_literals,
        )
        .map_err(Error::C)?,
    );
    c_source.extend_from_slice(
        &resumable_registration_c::emit_registration_v1(RegistrationSpec {
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
    use birnam_artifact::resumable_manifest::{self, ManifestEntrySpec};
    use birnam_artifact::stackless_selection::{
        RootKind, encode_artifact_selection, inspect_artifact_selection,
    };

    use crate::{Expr, Statement};

    use super::*;

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
