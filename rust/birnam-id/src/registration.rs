//! Native registration-envelope v1 ABI shape and pre-context validation.
//!
//! Staged-manifest row lookup and code-lease consumption are deliberately not
//! performed here. This layer validates the complete caller-owned carrier and
//! borrows descriptor bytes only for the synchronous Prepare call.

use core::mem::{align_of, offset_of, size_of};

use birnam_artifact::frame_descriptor::MAX_DESCRIPTOR_BYTES;
use birnam_artifact::materialization_selection::{
    Disposition as PortableDisposition, Row as PortableRow, SubjectKind,
};
use birnam_artifact::stackless_selection::{
    Disposition, ImplementationKind, RootKind, SelectionSpec,
};
use birnam_artifact::{materialization_selection, portable_resumable_manifest, resumable_manifest};
use sha2::{Digest, Sha256};

use crate::abi::{Method, Oop, borrowed_bytes};

pub(crate) const CARRIER_VERSION_V1: u32 = 1;
pub(crate) const CARRIER_VERSION_V2: u32 = 2;
pub(crate) const PREPARE_V1: u32 = 1;
pub(crate) const ABORT_V1: u32 = 2;
#[allow(dead_code)] // Used by the exported shim once staged-unit context lands.
pub(crate) const OK_V1: u32 = 0;
#[allow(dead_code)] // Used by the exported shim once staged-unit context lands.
pub(crate) const REJECTED_V1: u32 = 1;
const NOT_RESUMABLE_V1: u32 = 2;
const MAX_LANGUAGE_ARITY: u32 = 4096;

/// Opaque process-local capability returned to the candidate repository while
/// an overwritten resumable implementation remains a rollback target.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub(crate) struct BirnamStacklessGenerationLeaseV1 {
    token_index: u64,
    token_generation: u64,
}

#[repr(C)]
pub(crate) struct BirnamResumableRegistrationV1 {
    pub(crate) carrier_version: u32,
    pub(crate) struct_size: u32,
    pub(crate) operation: u32,
    pub(crate) flags: u32,
    pub(crate) manifest_entry_index: u32,
    pub(crate) reserved0: u32,
    pub(crate) registration_token: u64,
    pub(crate) descriptor_bytes: *const u8,
    pub(crate) descriptor_length: u64,
    pub(crate) entry_thunk: Option<Method>,
    pub(crate) step_method: Option<Method>,
    pub(crate) implementation_oop: Oop,
    pub(crate) code_generation_identity: u64,
    pub(crate) code_generation_lease: u64,
    pub(crate) portable_implementation_identity: u64,
    pub(crate) language_arity: u32,
    pub(crate) driver_argument_count: u32,
    pub(crate) descriptor_sha256: [u8; 32],
}

/// Append-only registration carrier for portable Blocks. The v1 prefix is
/// repeated field-for-field so generated C can use one flat exact-layout
/// structure and old callers remain entirely unchanged.
#[repr(C)]
pub(crate) struct BirnamResumableRegistrationV2 {
    pub(crate) carrier_version: u32,
    pub(crate) struct_size: u32,
    pub(crate) operation: u32,
    pub(crate) flags: u32,
    pub(crate) manifest_entry_index: u32,
    pub(crate) reserved0: u32,
    pub(crate) registration_token: u64,
    pub(crate) descriptor_bytes: *const u8,
    pub(crate) descriptor_length: u64,
    pub(crate) entry_thunk: Option<Method>,
    pub(crate) step_method: Option<Method>,
    pub(crate) implementation_oop: Oop,
    pub(crate) code_generation_identity: u64,
    pub(crate) code_generation_lease: u64,
    pub(crate) portable_implementation_identity: u64,
    pub(crate) language_arity: u32,
    pub(crate) driver_argument_count: u32,
    pub(crate) descriptor_sha256: [u8; 32],
    pub(crate) receiver_capture_offset_words: u32,
    pub(crate) reserved1: u32,
}

pub(crate) trait ResumableRegistrationEnvelope {
    fn operation(&self) -> u32;
    fn manifest_entry_index(&self) -> u32;
    fn registration_token(&self) -> u64;
    fn set_registration_token(&mut self, token: u64);
    fn step_method(&self) -> Option<Method>;
    fn entry_thunk(&self) -> Option<Method>;
    fn implementation_oop(&self) -> Oop;
    fn code_generation_identity(&self) -> u64;
    fn code_generation_lease(&self) -> u64;
    fn portable_implementation_identity(&self) -> u64;
    fn language_arity(&self) -> u32;
    fn driver_argument_count(&self) -> u32;
    fn descriptor_sha256(&self) -> [u8; 32];
    fn receiver_capture_offset_words(&self) -> usize;
    unsafe fn validate_prepare_shape(
        &self,
        implementation_kind: ImplementationKind,
    ) -> Result<&[u8], Error>;
    fn validate_abort_shape(&self) -> Result<(), Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    Header,
    Token,
    Descriptor,
    Function,
    Implementation,
    Generation,
    Arity,
    Digest,
    AbortPayload,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StageError {
    Manifest,
    Selection,
    Context,
    Index,
    Duplicate,
    Carrier(Error),
    Descriptor,
    Agreement,
    Incomplete,
}

#[derive(Clone, Debug)]
struct StagedEntry {
    authority: StagedEntryAuthority,
    implementation_kind: ImplementationKind,
    portable_implementation_identity: u64,
    descriptor_sha256: [u8; 32],
    language_arity: u32,
    capture_count: Option<u32>,
    claimed: bool,
}

#[derive(Clone, Debug)]
enum StagedEntryAuthority {
    Legacy {
        source_revision: [u8; 32],
        root_kind: RootKind,
        root_index: u32,
        path: Vec<u32>,
    },
    Portable {
        kind: SubjectKind,
        subject_index: u32,
        semantic_digest: [u8; 32],
        selector: Vec<u8>,
    },
}

impl StagedEntry {
    fn admission(&self) -> DescriptorAdmission<'_> {
        match &self.authority {
            StagedEntryAuthority::Legacy {
                source_revision,
                root_kind,
                root_index,
                path,
            } => DescriptorAdmission::Legacy(SelectionSpec {
                source_revision: *source_revision,
                root_kind: *root_kind,
                root_index: *root_index,
                path,
                implementation_kind: self.implementation_kind,
                disposition: Disposition::ResumableRequired,
            }),
            StagedEntryAuthority::Portable {
                kind,
                subject_index,
                semantic_digest,
                selector,
            } => DescriptorAdmission::Portable(PortableRow {
                kind: *kind,
                disposition: PortableDisposition::Resumable,
                subject_index: *subject_index,
                language_arity: self.language_arity,
                capture_count: self.capture_count.unwrap_or(0),
                semantic_digest: *semantic_digest,
                selector,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum DescriptorAdmission<'a> {
    Legacy(SelectionSpec<'a>),
    Portable(PortableRow<'a>),
}

/// Runtime-owned context established after mapped manifest admission and
/// before any unit-owned registration code executes.
pub(crate) struct StagedUnit {
    manifest_bytes: Vec<u8>,
    authority: [u8; 32],
    code_generation_identity: u64,
    code_generation_lease: u64,
    entries: Vec<StagedEntry>,
}

impl StagedUnit {
    pub(crate) fn is_portable_entry(&self, index: usize) -> bool {
        self.entries
            .get(index)
            .is_some_and(|entry| matches!(entry.authority, StagedEntryAuthority::Portable { .. }))
    }

    pub(crate) fn new(
        manifest_bytes: &[u8],
        authority: [u8; 32],
        code_generation_identity: u64,
        code_generation_lease: u64,
    ) -> Result<Self, StageError> {
        if code_generation_identity == 0 || code_generation_lease == 0 {
            return Err(StageError::Context);
        }
        let manifest = resumable_manifest::inspect(manifest_bytes, authority)
            .map_err(|_| StageError::Manifest)?;
        let mut entries = Vec::with_capacity(manifest.entry_count());
        for index in 0..manifest.entry_count() {
            let entry = manifest.entry(index).ok_or(StageError::Manifest)?;
            entries.push(StagedEntry {
                authority: StagedEntryAuthority::Legacy {
                    source_revision: entry.source_revision,
                    root_kind: entry.root_kind,
                    root_index: entry.root_index,
                    path: entry.path.iter().collect(),
                },
                implementation_kind: entry.implementation_kind,
                portable_implementation_identity: entry.portable_implementation_identity,
                descriptor_sha256: entry.descriptor_sha256,
                language_arity: entry.language_arity,
                capture_count: None,
                claimed: false,
            });
        }
        Ok(Self {
            manifest_bytes: manifest_bytes.to_vec(),
            authority,
            code_generation_identity,
            code_generation_lease,
            entries,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_portable(
        programs_bytes: &[u8],
        bindings_bytes: &[u8],
        selection_bytes: &[u8],
        manifest_bytes: &[u8],
        authority: [u8; 32],
        code_generation_identity: u64,
        code_generation_lease: u64,
    ) -> Result<Self, StageError> {
        if code_generation_identity == 0 || code_generation_lease == 0 {
            return Err(StageError::Context);
        }
        if materialization_selection::authority(programs_bytes, bindings_bytes, selection_bytes)
            .map_err(|_| StageError::Selection)?
            != authority
        {
            return Err(StageError::Context);
        }
        let selection =
            materialization_selection::inspect(selection_bytes, programs_bytes, bindings_bytes)
                .map_err(|_| StageError::Selection)?;
        let manifest = portable_resumable_manifest::inspect(manifest_bytes, authority, &selection)
            .map_err(|_| StageError::Manifest)?;
        let mut entries = Vec::with_capacity(manifest.entry_count());
        for index in 0..manifest.entry_count() {
            let manifest_entry = manifest.entry(index).ok_or(StageError::Manifest)?;
            let row = selection
                .row(manifest_entry.selection_row_index as usize)
                .ok_or(StageError::Selection)?;
            let implementation_kind = match row.kind {
                SubjectKind::NamedAdapter => ImplementationKind::NamedAdapter,
                SubjectKind::BlockBody => ImplementationKind::BlockBody,
            };
            entries.push(StagedEntry {
                authority: StagedEntryAuthority::Portable {
                    kind: row.kind,
                    subject_index: row.subject_index,
                    semantic_digest: row.semantic_digest,
                    selector: row.selector.to_vec(),
                },
                implementation_kind,
                portable_implementation_identity: manifest_entry.portable_implementation_identity,
                descriptor_sha256: manifest_entry.descriptor_sha256,
                language_arity: row.language_arity,
                capture_count: Some(row.capture_count),
                claimed: false,
            });
        }
        Ok(Self {
            manifest_bytes: manifest_bytes.to_vec(),
            authority,
            code_generation_identity,
            code_generation_lease,
            entries,
        })
    }

    /// Validate one Prepare against its exact manifest row. The row is marked
    /// only after the caller successfully transfers all data into Runtime
    /// ownership through `claim`.
    ///
    /// # Safety
    ///
    /// The envelope's bounded descriptor pointer follows
    /// `validate_prepare_shape`'s synchronous borrow contract.
    pub(crate) unsafe fn validate_prepare<'a, E: ResumableRegistrationEnvelope>(
        &'a self,
        envelope: &'a E,
    ) -> Result<(&'a [u8], DescriptorAdmission<'a>), StageError> {
        let entry = self
            .entries
            .get(envelope.manifest_entry_index() as usize)
            .ok_or(StageError::Index)?;
        if entry.claimed {
            return Err(StageError::Duplicate);
        }
        // SAFETY: forwarded bounded synchronous carrier contract.
        let descriptor = unsafe {
            envelope
                .validate_prepare_shape(entry.implementation_kind)
                .map_err(StageError::Carrier)?
        };
        if envelope.code_generation_identity() != self.code_generation_identity
            || envelope.code_generation_lease() != self.code_generation_lease
        {
            return Err(StageError::Context);
        }
        if envelope.portable_implementation_identity() != entry.portable_implementation_identity
            || envelope.language_arity() != entry.language_arity
            || envelope.descriptor_sha256() != entry.descriptor_sha256
        {
            return Err(StageError::Agreement);
        }
        let admission = entry.admission();
        let parsed = match admission {
            DescriptorAdmission::Legacy(selection) => {
                birnam_artifact::frame_descriptor::inspect(descriptor, selection)
            }
            DescriptorAdmission::Portable(row) => {
                birnam_artifact::frame_descriptor::inspect_portable(descriptor, row)
            }
        }
        .map_err(|_| StageError::Descriptor)?;
        let metadata = parsed.metadata();
        if metadata.portable_implementation_identity != envelope.portable_implementation_identity()
            || metadata.implementation_kind != entry.implementation_kind
            || metadata.entry_argument_count != envelope.language_arity()
            || metadata.driver_argument_count != envelope.driver_argument_count()
            || entry
                .capture_count
                .is_some_and(|count| parsed.capture_count() != count as usize)
        {
            return Err(StageError::Agreement);
        }
        Ok((descriptor, admission))
    }

    pub(crate) fn claim(&mut self, manifest_entry_index: u32) -> Result<(), StageError> {
        let entry = self
            .entries
            .get_mut(manifest_entry_index as usize)
            .ok_or(StageError::Index)?;
        if entry.claimed {
            return Err(StageError::Duplicate);
        }
        entry.claimed = true;
        Ok(())
    }

    pub(crate) fn release(&mut self, manifest_entry_index: u32) -> Result<(), StageError> {
        let entry = self
            .entries
            .get_mut(manifest_entry_index as usize)
            .ok_or(StageError::Index)?;
        if !entry.claimed {
            return Err(StageError::Index);
        }
        entry.claimed = false;
        Ok(())
    }

    pub(crate) fn finish(self) -> Result<(), StageError> {
        if self.entries.iter().any(|entry| !entry.claimed) {
            return Err(StageError::Incomplete);
        }
        Ok(())
    }

    pub(crate) const fn code_generation_identity(&self) -> u64 {
        self.code_generation_identity
    }

    pub(crate) const fn authority(&self) -> [u8; 32] {
        self.authority
    }

    pub(crate) fn manifest_bytes(&self) -> &[u8] {
        &self.manifest_bytes
    }

    pub(crate) fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn implementation_kind(&self, index: usize) -> Option<ImplementationKind> {
        self.entries
            .get(index)
            .map(|entry| entry.implementation_kind)
    }
}

impl BirnamResumableRegistrationV1 {
    fn validate_header(&self, operation: u32) -> Result<(), Error> {
        if self.carrier_version != CARRIER_VERSION_V1
            || self.struct_size as usize != size_of::<Self>()
            || self.operation != operation
            || self.flags != 0
            || self.reserved0 != 0
        {
            return Err(Error::Header);
        }
        Ok(())
    }

    /// Validate a Prepare carrier before consulting staged Runtime context.
    ///
    /// # Safety
    ///
    /// A non-null descriptor pointer must remain readable for the declared
    /// length for this synchronous call.
    pub(crate) unsafe fn validate_prepare_shape(
        &self,
        implementation_kind: ImplementationKind,
    ) -> Result<&[u8], Error> {
        self.validate_header(PREPARE_V1)?;
        if self.registration_token != 0 {
            return Err(Error::Token);
        }
        let length = usize::try_from(self.descriptor_length).map_err(|_| Error::Descriptor)?;
        if length == 0 || length > MAX_DESCRIPTOR_BYTES {
            return Err(Error::Descriptor);
        }
        // SAFETY: forwarded caller contract after bounded length conversion.
        let descriptor =
            unsafe { borrowed_bytes(self.descriptor_bytes, length) }.ok_or(Error::Descriptor)?;
        let (Some(entry_thunk), Some(step_method)) = (self.entry_thunk, self.step_method) else {
            return Err(Error::Function);
        };
        if entry_thunk as usize == step_method as usize {
            return Err(Error::Function);
        }
        match implementation_kind {
            ImplementationKind::NamedAdapter if self.implementation_oop.is_null() => {
                return Err(Error::Implementation);
            }
            ImplementationKind::BlockBody if !self.implementation_oop.is_null() => {
                return Err(Error::Implementation);
            }
            _ => {}
        }
        if self.code_generation_identity == 0 || self.code_generation_lease == 0 {
            return Err(Error::Generation);
        }
        if self.language_arity > MAX_LANGUAGE_ARITY || self.driver_argument_count > 1 {
            return Err(Error::Arity);
        }
        if Sha256::digest(descriptor).as_slice() != self.descriptor_sha256 {
            return Err(Error::Digest);
        }
        Ok(descriptor)
    }

    pub(crate) fn validate_abort_shape(&self) -> Result<(), Error> {
        self.validate_header(ABORT_V1)?;
        if self.registration_token == 0 {
            return Err(Error::Token);
        }
        if !self.descriptor_bytes.is_null()
            || self.descriptor_length != 0
            || self.entry_thunk.is_some()
            || self.step_method.is_some()
            || !self.implementation_oop.is_null()
            || self.code_generation_identity != 0
            || self.code_generation_lease != 0
            || self.portable_implementation_identity != 0
            || self.language_arity != 0
            || self.driver_argument_count != 0
            || self.descriptor_sha256 != [0; 32]
        {
            return Err(Error::AbortPayload);
        }
        Ok(())
    }
}

impl ResumableRegistrationEnvelope for BirnamResumableRegistrationV1 {
    fn operation(&self) -> u32 {
        self.operation
    }
    fn manifest_entry_index(&self) -> u32 {
        self.manifest_entry_index
    }
    fn registration_token(&self) -> u64 {
        self.registration_token
    }
    fn set_registration_token(&mut self, token: u64) {
        self.registration_token = token;
    }
    fn step_method(&self) -> Option<Method> {
        self.step_method
    }
    fn entry_thunk(&self) -> Option<Method> {
        self.entry_thunk
    }
    fn implementation_oop(&self) -> Oop {
        self.implementation_oop
    }
    fn code_generation_identity(&self) -> u64 {
        self.code_generation_identity
    }
    fn code_generation_lease(&self) -> u64 {
        self.code_generation_lease
    }
    fn portable_implementation_identity(&self) -> u64 {
        self.portable_implementation_identity
    }
    fn language_arity(&self) -> u32 {
        self.language_arity
    }
    fn driver_argument_count(&self) -> u32 {
        self.driver_argument_count
    }
    fn descriptor_sha256(&self) -> [u8; 32] {
        self.descriptor_sha256
    }
    fn receiver_capture_offset_words(&self) -> usize {
        0
    }
    unsafe fn validate_prepare_shape(
        &self,
        implementation_kind: ImplementationKind,
    ) -> Result<&[u8], Error> {
        // SAFETY: forwarded trait contract.
        unsafe { BirnamResumableRegistrationV1::validate_prepare_shape(self, implementation_kind) }
    }
    fn validate_abort_shape(&self) -> Result<(), Error> {
        BirnamResumableRegistrationV1::validate_abort_shape(self)
    }
}

impl BirnamResumableRegistrationV2 {
    fn validate_header(&self, operation: u32) -> Result<(), Error> {
        if self.carrier_version != CARRIER_VERSION_V2
            || self.struct_size as usize != size_of::<Self>()
            || self.operation != operation
            || self.flags != 0
            || self.reserved0 != 0
            || self.reserved1 != 0
        {
            return Err(Error::Header);
        }
        Ok(())
    }

    unsafe fn validate_prepare_shape_v2(
        &self,
        implementation_kind: ImplementationKind,
    ) -> Result<&[u8], Error> {
        self.validate_header(PREPARE_V1)?;
        if self.registration_token != 0 {
            return Err(Error::Token);
        }
        let length = usize::try_from(self.descriptor_length).map_err(|_| Error::Descriptor)?;
        if length == 0 || length > MAX_DESCRIPTOR_BYTES {
            return Err(Error::Descriptor);
        }
        // SAFETY: forwarded caller contract after bounded length conversion.
        let descriptor =
            unsafe { borrowed_bytes(self.descriptor_bytes, length) }.ok_or(Error::Descriptor)?;
        let (Some(entry_thunk), Some(step_method)) = (self.entry_thunk, self.step_method) else {
            return Err(Error::Function);
        };
        if entry_thunk as usize == step_method as usize {
            return Err(Error::Function);
        }
        match implementation_kind {
            ImplementationKind::NamedAdapter
                if self.implementation_oop.is_null() || self.receiver_capture_offset_words != 0 =>
            {
                return Err(Error::Implementation);
            }
            ImplementationKind::BlockBody
                if !self.implementation_oop.is_null()
                    || self.receiver_capture_offset_words != 1 =>
            {
                return Err(Error::Implementation);
            }
            _ => {}
        }
        if self.code_generation_identity == 0 || self.code_generation_lease == 0 {
            return Err(Error::Generation);
        }
        if self.language_arity > MAX_LANGUAGE_ARITY || self.driver_argument_count > 1 {
            return Err(Error::Arity);
        }
        if Sha256::digest(descriptor).as_slice() != self.descriptor_sha256 {
            return Err(Error::Digest);
        }
        Ok(descriptor)
    }

    fn validate_abort_shape_v2(&self) -> Result<(), Error> {
        self.validate_header(ABORT_V1)?;
        if self.registration_token == 0 {
            return Err(Error::Token);
        }
        if !self.descriptor_bytes.is_null()
            || self.descriptor_length != 0
            || self.entry_thunk.is_some()
            || self.step_method.is_some()
            || !self.implementation_oop.is_null()
            || self.code_generation_identity != 0
            || self.code_generation_lease != 0
            || self.portable_implementation_identity != 0
            || self.language_arity != 0
            || self.driver_argument_count != 0
            || self.descriptor_sha256 != [0; 32]
            || self.receiver_capture_offset_words != 0
        {
            return Err(Error::AbortPayload);
        }
        Ok(())
    }
}

impl ResumableRegistrationEnvelope for BirnamResumableRegistrationV2 {
    fn operation(&self) -> u32 {
        self.operation
    }
    fn manifest_entry_index(&self) -> u32 {
        self.manifest_entry_index
    }
    fn registration_token(&self) -> u64 {
        self.registration_token
    }
    fn set_registration_token(&mut self, token: u64) {
        self.registration_token = token;
    }
    fn step_method(&self) -> Option<Method> {
        self.step_method
    }
    fn entry_thunk(&self) -> Option<Method> {
        self.entry_thunk
    }
    fn implementation_oop(&self) -> Oop {
        self.implementation_oop
    }
    fn code_generation_identity(&self) -> u64 {
        self.code_generation_identity
    }
    fn code_generation_lease(&self) -> u64 {
        self.code_generation_lease
    }
    fn portable_implementation_identity(&self) -> u64 {
        self.portable_implementation_identity
    }
    fn language_arity(&self) -> u32 {
        self.language_arity
    }
    fn driver_argument_count(&self) -> u32 {
        self.driver_argument_count
    }
    fn descriptor_sha256(&self) -> [u8; 32] {
        self.descriptor_sha256
    }
    fn receiver_capture_offset_words(&self) -> usize {
        self.receiver_capture_offset_words as usize
    }
    unsafe fn validate_prepare_shape(
        &self,
        implementation_kind: ImplementationKind,
    ) -> Result<&[u8], Error> {
        // SAFETY: forwarded trait contract.
        unsafe { self.validate_prepare_shape_v2(implementation_kind) }
    }
    fn validate_abort_shape(&self) -> Result<(), Error> {
        self.validate_abort_shape_v2()
    }
}

/// Register or abort one manifest-indexed resumable entry in the active
/// Runtime's staged unit context.
///
/// # Safety
///
/// `envelope` must be non-null, aligned, exclusively writable for one complete
/// carrier, and satisfy the operation-specific pointer lifetime contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_register_resumable_v1(
    envelope: *mut BirnamResumableRegistrationV1,
) -> u32 {
    let Some(envelope) = (unsafe { envelope.as_mut() }) else {
        return REJECTED_V1;
    };
    crate::runtime::with_active_if_present(|runtime| {
        // SAFETY: the active Runtime consumes the same bounded synchronous
        // envelope borrow established by this exported boundary.
        unsafe { runtime.register_resumable_envelope(envelope) }
    })
    .unwrap_or(REJECTED_V1)
}

/// Append-only portable-Block registration boundary. V1 remains frozen for
/// already-built artifacts; V2 makes the receiver's physical capture offset
/// explicit and checked before it enters Runtime-owned state.
///
/// # Safety
///
/// The carrier and descriptor pointer follow the same synchronous contracts as
/// `birnam_register_resumable_v1`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_register_resumable_v2(
    envelope: *mut BirnamResumableRegistrationV2,
) -> u32 {
    let Some(envelope) = (unsafe { envelope.as_mut() }) else {
        return REJECTED_V1;
    };
    crate::runtime::with_active_if_present(|runtime| {
        // SAFETY: the active Runtime consumes this bounded synchronous borrow.
        unsafe { runtime.register_resumable_envelope(envelope) }
    })
    .unwrap_or(REJECTED_V1)
}

/// Host-to-floor staging seam. This symbol is not imported by generated code
/// and therefore is not part of Runtime ABI v3; it brackets the sole generated
/// publication symbol with already-admitted mapped-unit context.
///
/// # Safety
///
/// Both pointers must remain readable for their declared lengths for this
/// synchronous call. `authority` names exactly 32 bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_id_begin_resumable_unit_v1(
    manifest: *const u8,
    manifest_length: usize,
    authority: *const u8,
    code_generation_identity: u64,
    code_generation_lease: u64,
) -> u32 {
    if manifest_length > resumable_manifest::MAX_MANIFEST_BYTES {
        return REJECTED_V1;
    }
    // SAFETY: forwarded synchronous caller contracts after the public bound.
    let Some(manifest) = (unsafe { borrowed_bytes(manifest, manifest_length) }) else {
        return REJECTED_V1;
    };
    // SAFETY: authority's fixed readable extent is part of this function's
    // native contract.
    let Some(authority) = (unsafe { borrowed_bytes(authority, 32) }) else {
        return REJECTED_V1;
    };
    let Ok(authority) = authority.try_into() else {
        return REJECTED_V1;
    };
    crate::runtime::with_active_if_present(|runtime| {
        u32::from(!runtime.begin_staged_resumable_unit(
            manifest,
            authority,
            code_generation_identity,
            code_generation_lease,
        ))
    })
    .unwrap_or(REJECTED_V1)
}

/// Host-to-floor staging seam for the source-independent portable native
/// profile. Every byte input is synchronously copied after complete 60L and
/// portable-manifest admission.
///
/// # Safety
///
/// Every pointer must remain readable for its declared length for this call;
/// `authority` names exactly 32 bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_id_begin_portable_resumable_unit_v1(
    programs: *const u8,
    programs_length: usize,
    bindings: *const u8,
    bindings_length: usize,
    selection: *const u8,
    selection_length: usize,
    manifest: *const u8,
    manifest_length: usize,
    authority: *const u8,
    code_generation_identity: u64,
    code_generation_lease: u64,
) -> u32 {
    if programs_length > 64 << 20
        || bindings_length > 64 << 20
        || selection_length > 64 << 20
        || manifest_length > portable_resumable_manifest::MAX_MANIFEST_BYTES
    {
        return REJECTED_V1;
    }
    let (Some(programs), Some(bindings), Some(selection), Some(manifest), Some(authority)) = (unsafe {
        (
            borrowed_bytes(programs, programs_length),
            borrowed_bytes(bindings, bindings_length),
            borrowed_bytes(selection, selection_length),
            borrowed_bytes(manifest, manifest_length),
            borrowed_bytes(authority, 32),
        )
    }) else {
        return REJECTED_V1;
    };
    let Ok(authority) = authority.try_into() else {
        return REJECTED_V1;
    };
    crate::runtime::with_active_if_present(|runtime| {
        u32::from(!runtime.begin_staged_portable_resumable_unit(
            programs,
            bindings,
            selection,
            manifest,
            authority,
            code_generation_identity,
            code_generation_lease,
        ))
    })
    .unwrap_or(REJECTED_V1)
}

/// Close the active mapped-unit staging context after every manifest row has
/// been claimed.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_id_finish_resumable_unit_v1() -> u32 {
    crate::runtime::with_active_if_present(|runtime| {
        u32::from(!runtime.finish_staged_resumable_unit())
    })
    .unwrap_or(REJECTED_V1)
}

/// Roll back the active mapped-unit staging context regardless of whether all
/// manifest rows have already been claimed. This loader-private compensation
/// seam is deliberately outside IdApi and Runtime ABI v3.
#[unsafe(no_mangle)]
pub extern "C" fn birnam_id_cancel_resumable_unit_v1() -> u32 {
    crate::runtime::with_active_if_present(|runtime| {
        u32::from(!runtime.cancel_staged_resumable_unit())
    })
    .unwrap_or(REJECTED_V1)
}

/// Acquire a process-local generation lease when `method` names a registered
/// resumable entry. Ordinary synchronous methods answer `NOT_RESUMABLE_V1` and
/// require no lease. This candidate-repository seam remains outside IdApi and
/// Runtime ABI v3.
///
/// # Safety
///
/// `lease` must be non-null, aligned, and exclusively writable for one complete
/// carrier for this synchronous call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_id_try_acquire_stackless_generation_lease_v1(
    method: Oop,
    lease: *mut BirnamStacklessGenerationLeaseV1,
) -> u32 {
    if method.is_null() {
        return REJECTED_V1;
    }
    let Some(lease) = (unsafe { lease.as_mut() }) else {
        return REJECTED_V1;
    };
    *lease = BirnamStacklessGenerationLeaseV1::default();
    crate::runtime::with_active_if_present(|runtime| {
        // SAFETY: the non-null carrier came from a method dictionary entry.
        let method = unsafe { crate::raw::MethodAddress::from_oop(method) }
            .expect("non-null method carrier");
        match runtime.try_acquire_stackless_generation_lease(method) {
            Ok(Some(capability)) => {
                let (index, generation) = capability.into_raw_parts();
                let Ok(index) = u64::try_from(index) else {
                    runtime
                        .release_stackless_generation_lease(capability)
                        .unwrap_or_else(|_| {
                            crate::boundary::fatal_invariant("generation lease compensation failed")
                        });
                    return REJECTED_V1;
                };
                lease.token_index = index;
                lease.token_generation = generation;
                OK_V1
            }
            Ok(None) => NOT_RESUMABLE_V1,
            Err(_) => REJECTED_V1,
        }
    })
    .unwrap_or(REJECTED_V1)
}

/// Release one candidate-repository generation lease. Capabilities are
/// one-shot and generation checked by the owning Runtime.
///
/// # Safety
///
/// `lease` must be non-null, aligned, and readable for one complete carrier for
/// this synchronous call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_id_release_stackless_generation_lease_v1(
    lease: *const BirnamStacklessGenerationLeaseV1,
) -> u32 {
    let Some(lease) = (unsafe { lease.as_ref() }) else {
        return REJECTED_V1;
    };
    let Ok(index) = usize::try_from(lease.token_index) else {
        return REJECTED_V1;
    };
    if lease.token_generation == 0 {
        return REJECTED_V1;
    }
    crate::runtime::with_active_if_present(|runtime| {
        u32::from(
            runtime
                .release_stackless_generation_lease(
                    crate::execution::GenerationLeaseCapability::from_raw_parts(
                        index,
                        lease.token_generation,
                    ),
                )
                .is_err(),
        )
    })
    .unwrap_or(REJECTED_V1)
}

/// Retire one quiescent Runtime-local code generation after its repository has
/// removed every externally visible method carrier. This loader-private seam is
/// outside frozen IdApi v1; success releases committed manifest ownership and
/// makes every thunk from the generation unrecognizable before `dlclose`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_id_retire_stackless_code_generation_v1(
    code_generation_identity: u64,
) -> u32 {
    crate::runtime::with_active_if_present(|runtime| {
        runtime
            .request_stackless_code_generation_retirement(code_generation_identity)
            .abi_code()
    })
    .unwrap_or(0)
}

/// Resolve one committed block-vtable identity for image writing.
///
/// This is a host-to-floor image seam, not an import used by generated code,
/// and therefore is deliberately outside Runtime ABI v3. The selected image
/// rung discovers it from the exact RTLD_LOCAL floor that owns `IdApi`.
///
/// # Safety
///
/// `authority` must remain readable for exactly `authority_length` bytes for
/// this synchronous call. `manifest_entry_index` must be non-null, aligned,
/// and writable for one `u32`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_id_stackless_block_vtable_image_reference_v1(
    authority: *const u8,
    authority_length: usize,
    vtable: Oop,
    manifest_entry_index: *mut u32,
) -> u32 {
    if authority_length != 32 || vtable.is_null() {
        return 0;
    }
    // SAFETY: the fixed bound has been checked and the caller provides the
    // synchronous readable extent.
    let Some(authority) = (unsafe { borrowed_bytes(authority, authority_length) }) else {
        return 0;
    };
    let Ok(authority) = authority.try_into() else {
        return 0;
    };
    let Some(output) = (unsafe { manifest_entry_index.as_mut() }) else {
        return 0;
    };
    crate::runtime::with_active_if_present(|runtime| {
        let Some(index) = runtime.stackless_block_vtable_image_reference(vtable, authority) else {
            return 0;
        };
        *output = index;
        1
    })
    .unwrap_or(0)
}

/// Verify that an exact mapped-unit resolver result agrees with the committed
/// Runtime catalog for one authority and manifest row.
///
/// This loader-private seam is outside Runtime ABI v3 for the same reason as
/// `birnam_id_begin_resumable_unit_v1`.
///
/// # Safety
///
/// `authority` must remain readable for exactly `authority_length` bytes for
/// this synchronous call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_id_stackless_block_vtable_matches_v1(
    authority: *const u8,
    authority_length: usize,
    manifest_entry_index: u32,
    vtable: Oop,
) -> u32 {
    if authority_length != 32 || vtable.is_null() {
        return 0;
    }
    // SAFETY: the fixed bound has been checked and the caller provides the
    // synchronous readable extent.
    let Some(authority) = (unsafe { borrowed_bytes(authority, authority_length) }) else {
        return 0;
    };
    let Ok(authority) = authority.try_into() else {
        return 0;
    };
    crate::runtime::with_active_if_present(|runtime| {
        u32::from(
            runtime.stackless_block_vtable_for_manifest_index(authority, manifest_entry_index)
                == Some(vtable),
        )
    })
    .unwrap_or(0)
}

/// Resolve one BlockBody vtable belonging to the active portable staging
/// transaction. This image-loader seam is private and deliberately remains
/// outside frozen IdApi and Runtime ABI v3.
///
/// # Safety
///
/// `authority` must remain readable for exactly `authority_length` bytes for
/// this synchronous call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_id_staged_portable_block_vtable_v1(
    authority: *const u8,
    authority_length: usize,
    manifest_entry_index: u32,
) -> Oop {
    if authority_length != 32 {
        return core::ptr::null_mut();
    }
    let Some(authority) = (unsafe { borrowed_bytes(authority, authority_length) }) else {
        return core::ptr::null_mut();
    };
    let Ok(authority) = authority.try_into() else {
        return core::ptr::null_mut();
    };
    crate::runtime::with_active_if_present(|runtime| {
        runtime.staged_portable_block_vtable_for_manifest_index(authority, manifest_entry_index)
    })
    .flatten()
    .unwrap_or(core::ptr::null_mut())
}

/// Compare mapped image provenance with the exact manifest bytes retained by
/// the committed Runtime unit.
///
/// This loader-private seam is outside Runtime ABI v3. It gives the image rung
/// a boolean authority check without exposing Runtime-owned bytes or catalog
/// storage across the native boundary.
///
/// # Safety
///
/// Both pointers must remain readable for their declared lengths for this
/// synchronous call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_id_stackless_resumable_manifest_matches_v1(
    authority: *const u8,
    authority_length: usize,
    code_generation_identity: u64,
    manifest: *const u8,
    manifest_length: usize,
) -> u32 {
    if authority_length != 32
        || code_generation_identity == 0
        || manifest_length > resumable_manifest::MAX_MANIFEST_BYTES
    {
        return 0;
    }
    // SAFETY: both synchronous caller contracts are forwarded after their
    // public bounds have been checked.
    let Some(authority) = (unsafe { borrowed_bytes(authority, authority_length) }) else {
        return 0;
    };
    // Empty manifests have a non-empty canonical envelope, but accepting an
    // empty slice here keeps this pointer boundary independently well-defined.
    let Some(manifest) = (unsafe { borrowed_bytes(manifest, manifest_length) }) else {
        return 0;
    };
    let Ok(authority) = authority.try_into() else {
        return 0;
    };
    crate::runtime::with_active_if_present(|runtime| {
        u32::from(runtime.stackless_resumable_manifest_matches(
            authority,
            code_generation_identity,
            manifest,
        ))
    })
    .unwrap_or(0)
}

const _: () = {
    assert!(size_of::<Oop>() == 8);
    assert!(size_of::<Method>() == 8);
    assert!(size_of::<Option<Method>>() == 8);
    assert!(align_of::<BirnamResumableRegistrationV1>() == 8);
    assert!(size_of::<BirnamResumableRegistrationV1>() == 136);
    assert!(offset_of!(BirnamResumableRegistrationV1, carrier_version) == 0x00);
    assert!(offset_of!(BirnamResumableRegistrationV1, struct_size) == 0x04);
    assert!(offset_of!(BirnamResumableRegistrationV1, operation) == 0x08);
    assert!(offset_of!(BirnamResumableRegistrationV1, flags) == 0x0c);
    assert!(offset_of!(BirnamResumableRegistrationV1, manifest_entry_index) == 0x10);
    assert!(offset_of!(BirnamResumableRegistrationV1, reserved0) == 0x14);
    assert!(offset_of!(BirnamResumableRegistrationV1, registration_token) == 0x18);
    assert!(offset_of!(BirnamResumableRegistrationV1, descriptor_bytes) == 0x20);
    assert!(offset_of!(BirnamResumableRegistrationV1, descriptor_length) == 0x28);
    assert!(offset_of!(BirnamResumableRegistrationV1, entry_thunk) == 0x30);
    assert!(offset_of!(BirnamResumableRegistrationV1, step_method) == 0x38);
    assert!(offset_of!(BirnamResumableRegistrationV1, implementation_oop) == 0x40);
    assert!(offset_of!(BirnamResumableRegistrationV1, code_generation_identity) == 0x48);
    assert!(offset_of!(BirnamResumableRegistrationV1, code_generation_lease) == 0x50);
    assert!(
        offset_of!(
            BirnamResumableRegistrationV1,
            portable_implementation_identity
        ) == 0x58
    );
    assert!(offset_of!(BirnamResumableRegistrationV1, language_arity) == 0x60);
    assert!(offset_of!(BirnamResumableRegistrationV1, driver_argument_count) == 0x64);
    assert!(offset_of!(BirnamResumableRegistrationV1, descriptor_sha256) == 0x68);
    assert!(align_of::<BirnamResumableRegistrationV2>() == 8);
    assert!(size_of::<BirnamResumableRegistrationV2>() == 144);
    assert!(offset_of!(BirnamResumableRegistrationV2, carrier_version) == 0x00);
    assert!(offset_of!(BirnamResumableRegistrationV2, struct_size) == 0x04);
    assert!(offset_of!(BirnamResumableRegistrationV2, operation) == 0x08);
    assert!(offset_of!(BirnamResumableRegistrationV2, flags) == 0x0c);
    assert!(offset_of!(BirnamResumableRegistrationV2, manifest_entry_index) == 0x10);
    assert!(offset_of!(BirnamResumableRegistrationV2, reserved0) == 0x14);
    assert!(offset_of!(BirnamResumableRegistrationV2, registration_token) == 0x18);
    assert!(offset_of!(BirnamResumableRegistrationV2, descriptor_bytes) == 0x20);
    assert!(offset_of!(BirnamResumableRegistrationV2, descriptor_length) == 0x28);
    assert!(offset_of!(BirnamResumableRegistrationV2, entry_thunk) == 0x30);
    assert!(offset_of!(BirnamResumableRegistrationV2, step_method) == 0x38);
    assert!(offset_of!(BirnamResumableRegistrationV2, implementation_oop) == 0x40);
    assert!(offset_of!(BirnamResumableRegistrationV2, code_generation_identity) == 0x48);
    assert!(offset_of!(BirnamResumableRegistrationV2, code_generation_lease) == 0x50);
    assert!(
        offset_of!(
            BirnamResumableRegistrationV2,
            portable_implementation_identity
        ) == 0x58
    );
    assert!(offset_of!(BirnamResumableRegistrationV2, language_arity) == 0x60);
    assert!(offset_of!(BirnamResumableRegistrationV2, driver_argument_count) == 0x64);
    assert!(offset_of!(BirnamResumableRegistrationV2, descriptor_sha256) == 0x68);
    assert!(offset_of!(BirnamResumableRegistrationV2, receiver_capture_offset_words) == 0x88);
    assert!(offset_of!(BirnamResumableRegistrationV2, reserved1) == 0x8c);
};

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" fn entry(_: Oop, _: *const Oop, _: usize) -> Oop {
        core::ptr::null_mut()
    }

    unsafe extern "C" fn step(_: Oop, _: *const Oop, _: usize) -> Oop {
        core::ptr::null_mut()
    }

    fn prepare(bytes: &[u8], kind: ImplementationKind) -> BirnamResumableRegistrationV1 {
        let implementation_oop = match kind {
            ImplementationKind::NamedAdapter => {
                core::ptr::NonNull::<u8>::dangling().as_ptr().cast()
            }
            ImplementationKind::BlockBody => core::ptr::null_mut(),
        };
        BirnamResumableRegistrationV1 {
            carrier_version: CARRIER_VERSION_V1,
            struct_size: size_of::<BirnamResumableRegistrationV1>() as u32,
            operation: PREPARE_V1,
            flags: 0,
            manifest_entry_index: 7,
            reserved0: 0,
            registration_token: 0,
            descriptor_bytes: bytes.as_ptr(),
            descriptor_length: bytes.len() as u64,
            entry_thunk: Some(entry),
            step_method: Some(step),
            implementation_oop,
            code_generation_identity: 11,
            code_generation_lease: 13,
            portable_implementation_identity: 0,
            language_arity: 2,
            driver_argument_count: 1,
            descriptor_sha256: Sha256::digest(bytes).into(),
        }
    }

    fn prepare_v2(bytes: &[u8], kind: ImplementationKind) -> BirnamResumableRegistrationV2 {
        let v1 = prepare(bytes, kind);
        BirnamResumableRegistrationV2 {
            carrier_version: CARRIER_VERSION_V2,
            struct_size: size_of::<BirnamResumableRegistrationV2>() as u32,
            operation: v1.operation,
            flags: v1.flags,
            manifest_entry_index: v1.manifest_entry_index,
            reserved0: v1.reserved0,
            registration_token: v1.registration_token,
            descriptor_bytes: v1.descriptor_bytes,
            descriptor_length: v1.descriptor_length,
            entry_thunk: v1.entry_thunk,
            step_method: v1.step_method,
            implementation_oop: v1.implementation_oop,
            code_generation_identity: v1.code_generation_identity,
            code_generation_lease: v1.code_generation_lease,
            portable_implementation_identity: v1.portable_implementation_identity,
            language_arity: v1.language_arity,
            driver_argument_count: v1.driver_argument_count,
            descriptor_sha256: v1.descriptor_sha256,
            receiver_capture_offset_words: u32::from(kind == ImplementationKind::BlockBody),
            reserved1: 0,
        }
    }

    #[test]
    fn prepare_shape_checks_digest_nullability_and_function_distinction() {
        let bytes = b"descriptor";
        let named = prepare(bytes, ImplementationKind::NamedAdapter);
        // SAFETY: bytes remains readable for this call.
        assert_eq!(
            unsafe { named.validate_prepare_shape(ImplementationKind::NamedAdapter) }.unwrap(),
            bytes
        );
        let block = prepare(bytes, ImplementationKind::BlockBody);
        // SAFETY: bytes remains readable for this call.
        assert!(unsafe { block.validate_prepare_shape(ImplementationKind::BlockBody) }.is_ok());

        let mut wrong = prepare(bytes, ImplementationKind::BlockBody);
        wrong.descriptor_sha256[0] ^= 1;
        // SAFETY: bytes remains readable for this call.
        assert_eq!(
            unsafe { wrong.validate_prepare_shape(ImplementationKind::BlockBody) },
            Err(Error::Digest)
        );
        let mut same = prepare(bytes, ImplementationKind::BlockBody);
        same.step_method = same.entry_thunk;
        // SAFETY: bytes remains readable for this call.
        assert_eq!(
            unsafe { same.validate_prepare_shape(ImplementationKind::BlockBody) },
            Err(Error::Function)
        );
    }

    #[test]
    fn abort_requires_a_token_and_zero_prepare_payload() {
        let mut abort = prepare(b"descriptor", ImplementationKind::BlockBody);
        abort.operation = ABORT_V1;
        abort.registration_token = 23;
        abort.descriptor_bytes = core::ptr::null();
        abort.descriptor_length = 0;
        abort.entry_thunk = None;
        abort.step_method = None;
        abort.implementation_oop = core::ptr::null_mut();
        abort.code_generation_identity = 0;
        abort.code_generation_lease = 0;
        abort.portable_implementation_identity = 0;
        abort.language_arity = 0;
        abort.driver_argument_count = 0;
        abort.descriptor_sha256 = [0; 32];
        assert_eq!(abort.validate_abort_shape(), Ok(()));
        abort.descriptor_length = 1;
        assert_eq!(abort.validate_abort_shape(), Err(Error::AbortPayload));
    }

    #[test]
    fn v2_requires_the_kind_specific_capture_offset() {
        let bytes = b"descriptor";
        let block = prepare_v2(bytes, ImplementationKind::BlockBody);
        // SAFETY: bytes remains readable for this call.
        assert!(unsafe { block.validate_prepare_shape_v2(ImplementationKind::BlockBody) }.is_ok());
        let mut wrong_block = prepare_v2(bytes, ImplementationKind::BlockBody);
        wrong_block.receiver_capture_offset_words = 0;
        // SAFETY: bytes remains readable for this call.
        assert_eq!(
            unsafe { wrong_block.validate_prepare_shape_v2(ImplementationKind::BlockBody) },
            Err(Error::Implementation)
        );

        let named = prepare_v2(bytes, ImplementationKind::NamedAdapter);
        // SAFETY: bytes remains readable for this call.
        assert!(
            unsafe { named.validate_prepare_shape_v2(ImplementationKind::NamedAdapter) }.is_ok()
        );
        let mut wrong_named = prepare_v2(bytes, ImplementationKind::NamedAdapter);
        wrong_named.receiver_capture_offset_words = 1;
        // SAFETY: bytes remains readable for this call.
        assert_eq!(
            unsafe { wrong_named.validate_prepare_shape_v2(ImplementationKind::NamedAdapter) },
            Err(Error::Implementation)
        );
    }

    #[test]
    fn v2_abort_requires_zero_appended_words() {
        let mut abort = prepare_v2(b"descriptor", ImplementationKind::BlockBody);
        abort.operation = ABORT_V1;
        abort.registration_token = 23;
        abort.descriptor_bytes = core::ptr::null();
        abort.descriptor_length = 0;
        abort.entry_thunk = None;
        abort.step_method = None;
        abort.implementation_oop = core::ptr::null_mut();
        abort.code_generation_identity = 0;
        abort.code_generation_lease = 0;
        abort.portable_implementation_identity = 0;
        abort.language_arity = 0;
        abort.driver_argument_count = 0;
        abort.descriptor_sha256 = [0; 32];
        abort.receiver_capture_offset_words = 0;
        assert_eq!(abort.validate_abort_shape_v2(), Ok(()));
        abort.receiver_capture_offset_words = 1;
        assert_eq!(abort.validate_abort_shape_v2(), Err(Error::AbortPayload));
    }

    #[test]
    fn frozen_native_layout_is_exact() {
        assert_eq!(size_of::<BirnamResumableRegistrationV1>(), 136);
        assert_eq!(align_of::<BirnamResumableRegistrationV1>(), 8);
        assert_eq!(size_of::<BirnamResumableRegistrationV2>(), 144);
        assert_eq!(align_of::<BirnamResumableRegistrationV2>(), 8);
        assert_eq!(
            offset_of!(BirnamResumableRegistrationV2, receiver_capture_offset_words),
            0x88
        );
        assert_eq!(offset_of!(BirnamResumableRegistrationV2, reserved1), 0x8c);
    }

    #[test]
    fn staged_manifest_context_admits_exact_row_once_and_requires_total_claims() {
        let fixture = |name: &str| {
            std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../abi/fixtures/stackless-s2")
                    .join(name),
            )
            .unwrap()
        };
        let manifest_bytes = fixture("resumable-manifest-block-only.bin");
        let authority: [u8; 32] = manifest_bytes[0xc0..0xe0].try_into().unwrap();
        let descriptor = fixture("frame-descriptor-source-map-block.bin");
        let manifest = resumable_manifest::inspect(&manifest_bytes, authority).unwrap();
        let entry = manifest.entry(0).unwrap();
        let mut envelope = prepare(&descriptor, ImplementationKind::BlockBody);
        envelope.manifest_entry_index = 0;
        envelope.portable_implementation_identity = entry.portable_implementation_identity;
        envelope.language_arity = entry.language_arity;
        envelope.driver_argument_count = 0;
        envelope.descriptor_sha256 = entry.descriptor_sha256;

        let staged = StagedUnit::new(&manifest_bytes, authority, 11, 13).unwrap();
        {
            // SAFETY: descriptor owns readable bytes for this call.
            let (admitted, selection) = unsafe { staged.validate_prepare(&envelope) }.unwrap();
            assert_eq!(admitted, descriptor);
            let DescriptorAdmission::Legacy(selection) = selection else {
                panic!("legacy fixture must retain legacy descriptor admission")
            };
            assert_eq!(selection.path, &[2, 1]);
        }
        assert_eq!(staged.finish(), Err(StageError::Incomplete));

        let mut staged = StagedUnit::new(&manifest_bytes, authority, 11, 13).unwrap();
        // SAFETY: descriptor owns readable bytes for this call.
        unsafe { staged.validate_prepare(&envelope) }.unwrap();
        staged.claim(0).unwrap();
        // SAFETY: descriptor remains readable; duplicate fails before ownership.
        assert_eq!(
            unsafe { staged.validate_prepare(&envelope) }.unwrap_err(),
            StageError::Duplicate
        );
        assert!(staged.finish().is_ok());
    }
}
