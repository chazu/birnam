//! One indivisible named-method adapter artifact description.

use birnam_artifact::stackless_selection::{
    Disposition, ImplementationKind, SelectionSpec, portable_implementation_identity,
    source_map_identity,
};

use crate::resumable::{LayoutError, LowerError, color_slots, lower_named_adapter};
use crate::resumable_c;
use crate::resumable_descriptor;
use crate::resumable_registration_c::{self, RegistrationSpec};

#[derive(Debug, PartialEq, Eq)]
pub struct CompiledNamedResumableV1 {
    pub descriptor: Vec<u8>,
    pub c_source: Vec<u8>,
    pub selector: Vec<u8>,
    pub implementation_selector: Vec<u8>,
    pub portable_implementation_identity: u64,
    pub source_map_identity: u64,
    pub language_arity: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidSelection,
    InvalidSelector,
    QuantityOverflow,
    Logical(LowerError),
    Layout(LayoutError),
    Descriptor(resumable_descriptor::Error),
    C(resumable_c::Error),
    Registration(resumable_registration_c::Error),
}

/// Compile the retained-implementation adapter for one selected named method.
pub fn compile_named_adapter_v1(
    selector: &[u8],
    authority: SelectionSpec<'_>,
    diagnostic_path: &[u8],
    source: &[u8],
    symbol_index: u32,
    manifest_entry_index: u32,
) -> Result<CompiledNamedResumableV1, Error> {
    if authority.implementation_kind != ImplementationKind::NamedAdapter
        || authority.disposition != Disposition::ResumableRequired
    {
        return Err(Error::InvalidSelection);
    }
    let language_arity = selector_arity(selector)?;
    let implementation_selector = implementation_selector(language_arity)?;
    let program =
        lower_named_adapter(language_arity, &implementation_selector).map_err(Error::Logical)?;
    let layout = color_slots(&program).map_err(Error::Layout)?;
    let descriptor = resumable_descriptor::compile_descriptor_v2(
        &program,
        &layout,
        authority,
        diagnostic_path,
        source,
        0,
    )
    .map_err(Error::Descriptor)?;
    let portable_implementation_identity =
        portable_implementation_identity(authority, 1).map_err(|_| Error::InvalidSelection)?;
    let source_map_identity =
        source_map_identity(authority, 1).map_err(|_| Error::InvalidSelection)?;

    let mut c_source = resumable_c::emit_entry_thunk(symbol_index, selector);
    c_source.extend_from_slice(
        &resumable_c::emit_step_method(symbol_index, &program, &layout, 0).map_err(Error::C)?,
    );
    c_source.extend_from_slice(
        &resumable_registration_c::emit_registration_v1(RegistrationSpec {
            symbol_index,
            manifest_entry_index,
            implementation_kind: ImplementationKind::NamedAdapter,
            descriptor: &descriptor,
            portable_implementation_identity,
            language_arity,
            driver_argument_count: 0,
        })
        .map_err(Error::Registration)?,
    );

    Ok(CompiledNamedResumableV1 {
        descriptor,
        c_source,
        selector: selector.to_vec(),
        implementation_selector,
        portable_implementation_identity,
        source_map_identity,
        language_arity,
    })
}

fn selector_arity(selector: &[u8]) -> Result<u32, Error> {
    if selector.is_empty() {
        return Err(Error::InvalidSelector);
    }
    let colons = selector.iter().filter(|byte| **byte == b':').count();
    let arity = if colons != 0 {
        colons
    } else if selector[0].is_ascii_alphabetic() || selector[0] == b'_' {
        0
    } else {
        1
    };
    u32::try_from(arity).map_err(|_| Error::QuantityOverflow)
}

fn implementation_selector(language_arity: u32) -> Result<Vec<u8>, Error> {
    let argument_count = language_arity
        .checked_add(1)
        .ok_or(Error::QuantityOverflow)?;
    let count = usize::try_from(argument_count).map_err(|_| Error::QuantityOverflow)?;
    let capacity = count.checked_mul(6).ok_or(Error::QuantityOverflow)?;
    let mut selector = Vec::with_capacity(capacity);
    for _ in 0..count {
        selector.extend_from_slice(b"value:");
    }
    Ok(selector)
}

#[cfg(test)]
mod tests {
    use birnam_artifact::frame_descriptor::{self, CallKind};
    use birnam_artifact::stackless_selection::RootKind;

    use super::*;

    #[test]
    fn named_adapter_retains_second_lookup_and_distinct_entry_carrier() {
        let authority = SelectionSpec {
            source_revision: [0x31; 32],
            root_kind: RootKind::MethodOperation,
            root_index: 4,
            path: &[],
            implementation_kind: ImplementationKind::NamedAdapter,
            disposition: Disposition::ResumableRequired,
        };
        let compiled = compile_named_adapter_v1(
            b"at:put:",
            authority,
            b"methods.birnam",
            b"[Reflect defineMethod: #at:put: on: Map as: impl]",
            7,
            2,
        )
        .unwrap();
        assert_eq!(compiled.language_arity, 2);
        assert_eq!(compiled.implementation_selector, b"value:value:value:");
        let descriptor = frame_descriptor::inspect(&compiled.descriptor, authority).unwrap();
        assert!(descriptor.metadata().implementation_slot.is_some());
        assert_eq!(descriptor.metadata().entry_argument_count, 2);
        let call = descriptor.call_site(0).unwrap();
        assert_eq!(call.kind, CallKind::TailCall);
        assert_eq!(call.argument_count, 3);
        assert_eq!(call.selector, b"value:value:value:");

        let c = String::from_utf8(compiled.c_source).unwrap();
        assert!(c.contains("birnam_intern(\"at:put:\")"));
        assert!(c.contains("birnam_intern(\"value:value:value:\")"));
        assert!(c.contains("e.implementation_oop=implementation"));
        assert!(c.contains("e.manifest_entry_index=2U"));
    }
}
