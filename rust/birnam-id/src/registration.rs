//! Native registration-envelope v1 ABI shape and pre-context validation.
//!
//! Staged-manifest row lookup and code-lease consumption are deliberately not
//! performed here. This layer validates the complete caller-owned carrier and
//! borrows descriptor bytes only for the synchronous Prepare call.

use core::mem::{align_of, offset_of, size_of};

use birnam_artifact::frame_descriptor::MAX_DESCRIPTOR_BYTES;
use birnam_artifact::resumable_manifest;
use birnam_artifact::stackless_selection::{
    Disposition, ImplementationKind, RootKind, SelectionSpec,
};
use sha2::{Digest, Sha256};

use crate::abi::{Method, Oop, borrowed_bytes};

pub(crate) const CARRIER_VERSION_V1: u32 = 1;
pub(crate) const PREPARE_V1: u32 = 1;
pub(crate) const ABORT_V1: u32 = 2;
#[allow(dead_code)] // Used by the exported shim once staged-unit context lands.
pub(crate) const OK_V1: u32 = 0;
#[allow(dead_code)] // Used by the exported shim once staged-unit context lands.
pub(crate) const REJECTED_V1: u32 = 1;
const MAX_LANGUAGE_ARITY: u32 = 4096;

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
    source_revision: [u8; 32],
    root_kind: RootKind,
    root_index: u32,
    path: Vec<u32>,
    implementation_kind: ImplementationKind,
    portable_implementation_identity: u64,
    descriptor_sha256: [u8; 32],
    language_arity: u32,
    claimed: bool,
}

impl StagedEntry {
    fn selection(&self) -> SelectionSpec<'_> {
        SelectionSpec {
            source_revision: self.source_revision,
            root_kind: self.root_kind,
            root_index: self.root_index,
            path: &self.path,
            implementation_kind: self.implementation_kind,
            disposition: Disposition::ResumableRequired,
        }
    }
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
                source_revision: entry.source_revision,
                root_kind: entry.root_kind,
                root_index: entry.root_index,
                path: entry.path.iter().collect(),
                implementation_kind: entry.implementation_kind,
                portable_implementation_identity: entry.portable_implementation_identity,
                descriptor_sha256: entry.descriptor_sha256,
                language_arity: entry.language_arity,
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
    pub(crate) unsafe fn validate_prepare<'a>(
        &'a self,
        envelope: &'a BirnamResumableRegistrationV1,
    ) -> Result<(&'a [u8], SelectionSpec<'a>), StageError> {
        let entry = self
            .entries
            .get(envelope.manifest_entry_index as usize)
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
        if envelope.code_generation_identity != self.code_generation_identity
            || envelope.code_generation_lease != self.code_generation_lease
        {
            return Err(StageError::Context);
        }
        if envelope.portable_implementation_identity != entry.portable_implementation_identity
            || envelope.language_arity != entry.language_arity
            || envelope.descriptor_sha256 != entry.descriptor_sha256
        {
            return Err(StageError::Agreement);
        }
        let selection = entry.selection();
        let parsed = birnam_artifact::frame_descriptor::inspect(descriptor, selection)
            .map_err(|_| StageError::Descriptor)?;
        let metadata = parsed.metadata();
        if metadata.portable_implementation_identity != envelope.portable_implementation_identity
            || metadata.implementation_kind != entry.implementation_kind
            || metadata.entry_argument_count != envelope.language_arity
            || metadata.driver_argument_count != envelope.driver_argument_count
        {
            return Err(StageError::Agreement);
        }
        Ok((descriptor, selection))
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
    fn frozen_native_layout_is_exact() {
        assert_eq!(size_of::<BirnamResumableRegistrationV1>(), 136);
        assert_eq!(align_of::<BirnamResumableRegistrationV1>(), 8);
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
