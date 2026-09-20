//! Portable Birnam image-v8 support and runtime provider.

#![allow(clippy::missing_safety_doc)]

mod retained_facts;
pub mod v8;

/// Version-neutral support used by the image-v8 codec and runtime provider.
pub mod support {

    pub const AUTHORITY_DIGEST_LEN: usize = 64;

    pub const IMAGE_ARCH_TAG: u32 = if cfg!(target_arch = "aarch64") {
        1
    } else if cfg!(target_arch = "x86_64") {
        2
    } else {
        0
    };
    const RUNTIME_ABI_VERSION: u32 = 4;
    const RUNTIME_SHIM_NAMES: &[&[u8]] = &[
        b"birnam_send",
        b"birnam_intern",
        b"birnam_mkSmallInt",
        b"birnam_smallIntValue",
        b"birnam_mkFloat",
        b"birnam_floatValue",
        b"birnam_mkWord",
        b"birnam_wordValue",
        b"birnam_uintValue",
        b"birnam_mkForeignAddress",
        b"birnam_foreignAddressValue",
        b"birnam_isForeignAddress",
        b"birnam_mkString",
        b"birnam_nil",
        b"birnam_true",
        b"birnam_false",
        b"birnam_objectVt",
        b"birnam_proto",
        b"birnam_method",
        b"birnam_alloc",
        b"birnam_cstr",
        b"birnam_free_cstr",
        b"birnam_layoutAllOops",
        b"birnam_global",
        b"birnam_candidate_validate_binding",
    ];
    const REGISTRATION_SHIM_NAME: &[u8] = b"birnam_register_resumable_v1";
    const REGISTRATION_ABI_FIELDS_V1: &[(&[u8], u32, u32, u32, &[u8], &[u8])] = &[
        (b"carrier_version", 0x00, 4, 4, b"u32", b"not-applicable"),
        (b"struct_size", 0x04, 4, 4, b"u32", b"not-applicable"),
        (b"operation", 0x08, 4, 4, b"u32", b"not-applicable"),
        (b"flags", 0x0c, 4, 4, b"u32", b"not-applicable"),
        (
            b"manifest_entry_index",
            0x10,
            4,
            4,
            b"u32",
            b"not-applicable",
        ),
        (b"reserved0", 0x14, 4, 4, b"u32", b"not-applicable"),
        (b"registration_token", 0x18, 8, 8, b"u64", b"not-applicable"),
        (
            b"descriptor_bytes",
            0x20,
            8,
            8,
            b"pointer<const-u8>",
            b"prepare-nonnull-abort-null",
        ),
        (b"descriptor_length", 0x28, 8, 8, b"u64", b"not-applicable"),
        (
            b"entry_thunk",
            0x30,
            8,
            8,
            b"function-pointer<BirnamMethodV1>",
            b"prepare-nonnull-abort-null",
        ),
        (
            b"step_method",
            0x38,
            8,
            8,
            b"function-pointer<BirnamStepMethodV1>",
            b"prepare-nonnull-abort-null",
        ),
        (
            b"implementation_oop",
            0x40,
            8,
            8,
            b"BirnamOopV1",
            b"prepare-named-nonnull-block-null-abort-null",
        ),
        (
            b"code_generation_identity",
            0x48,
            8,
            8,
            b"u64",
            b"not-applicable",
        ),
        (
            b"code_generation_lease",
            0x50,
            8,
            8,
            b"u64",
            b"not-applicable",
        ),
        (
            b"portable_implementation_identity",
            0x58,
            8,
            8,
            b"u64",
            b"not-applicable",
        ),
        (b"language_arity", 0x60, 4, 4, b"u32", b"not-applicable"),
        (
            b"driver_argument_count",
            0x64,
            4,
            4,
            b"u32",
            b"not-applicable",
        ),
        (
            b"descriptor_sha256",
            0x68,
            32,
            1,
            b"bytes[32]",
            b"not-applicable",
        ),
    ];
    const REGISTRATION_ABI_CONSTANTS_V1: &[(&[u8], u32)] = &[
        (b"BIRNAM_REGISTRATION_CARRIER_VERSION_V1", 1),
        (b"BIRNAM_REGISTRATION_PREPARE_V1", 1),
        (b"BIRNAM_REGISTRATION_ABORT_V1", 2),
        (b"BIRNAM_REGISTRATION_OK_V1", 0),
        (b"BIRNAM_REGISTRATION_REJECTED_V1", 1),
    ];

    pub(crate) fn hash_registration_abi_tuple_v1(mut hash: u64) -> (u64, usize) {
        fn bytes(hash: &mut u64, count: &mut usize, value: &[u8]) {
            for byte in value {
                *hash ^= u64::from(*byte);
                *hash = hash.wrapping_mul(0x100000001b3);
            }
            *count += value.len();
        }

        fn u32_le(hash: &mut u64, count: &mut usize, value: u32) {
            bytes(hash, count, &value.to_le_bytes());
        }

        fn string(hash: &mut u64, count: &mut usize, value: &[u8]) {
            u32_le(hash, count, value.len() as u32);
            bytes(hash, count, value);
        }

        let mut count = 0;
        bytes(&mut hash, &mut count, b"birnam.registration-envelope.v1\0");
        string(
            &mut hash,
            &mut count,
            b"aarch64-apple-darwin|Mach-O|little-endian",
        );
        string(&mut hash, &mut count, b"birnam_register_resumable_v1");
        string(
            &mut hash,
            &mut count,
            b"u32(BirnamResumableRegistrationV1 *)",
        );
        for value in [136, 8, REGISTRATION_ABI_FIELDS_V1.len() as u32] {
            u32_le(&mut hash, &mut count, value);
        }
        for &(name, offset, width, alignment, abi_kind, nullability) in REGISTRATION_ABI_FIELDS_V1 {
            string(&mut hash, &mut count, name);
            for value in [offset, width, alignment] {
                u32_le(&mut hash, &mut count, value);
            }
            string(&mut hash, &mut count, abi_kind);
            string(&mut hash, &mut count, nullability);
        }
        u32_le(
            &mut hash,
            &mut count,
            REGISTRATION_ABI_CONSTANTS_V1.len() as u32,
        );
        for &(name, value) in REGISTRATION_ABI_CONSTANTS_V1 {
            string(&mut hash, &mut count, name);
            u32_le(&mut hash, &mut count, value);
        }
        (hash, count)
    }

    pub fn runtime_abi_hash() -> u64 {
        let mut hash = 0xcbf29ce484222325;
        for name in RUNTIME_SHIM_NAMES {
            for byte in *name {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
        for byte in REGISTRATION_SHIM_NAME {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash = hash_registration_abi_tuple_v1(hash).0;
        let mut version = RUNTIME_ABI_VERSION;
        for _ in 0..4 {
            hash ^= u64::from((version & 0xff) as u8);
            hash = hash.wrapping_mul(0x100000001b3);
            version >>= 8;
        }
        hash
    }

    #[cfg(any(feature = "module", test))]
    pub(crate) fn authority_digest_from_bytes(bytes: &[u8]) -> Option<[u8; AUTHORITY_DIGEST_LEN]> {
        bytes.try_into().ok()
    }

    use std::fs;
    use std::io::{self, Write};
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum EncodeError {
        PayloadTooLarge,
    }

    /// Encoder for image-v8 canonical Oop references.
    #[derive(Default)]
    pub struct FieldEncoder {
        bytes: Vec<u8>,
    }

    impl FieldEncoder {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn nil(&mut self) {
            self.bytes.push(0);
        }

        pub fn true_value(&mut self) {
            self.bytes.push(1);
        }

        pub fn false_value(&mut self) {
            self.bytes.push(2);
        }

        pub fn heap_ref(&mut self, id: u32) {
            self.bytes.push(7);
            self.bytes.extend_from_slice(&id.to_le_bytes());
        }

        pub fn null_oop(&mut self) {
            self.bytes.push(8);
        }

        pub fn symbol(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
            self.length_delimited(5, bytes)
        }

        pub fn export_ref(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
            self.length_delimited(9, bytes)
        }

        #[cfg(feature = "module")]
        pub(crate) fn portable_block_vtable(&mut self, index: u32) {
            self.bytes.push(12);
            self.bytes.extend_from_slice(&index.to_le_bytes());
        }

        pub fn finish(self) -> Vec<u8> {
            self.bytes
        }

        pub fn as_bytes(&self) -> &[u8] {
            &self.bytes
        }

        fn length_delimited(&mut self, tag: u8, bytes: &[u8]) -> Result<(), EncodeError> {
            let length = u32::try_from(bytes.len()).map_err(|_| EncodeError::PayloadTooLarge)?;
            self.bytes.push(tag);
            self.bytes.extend_from_slice(&length.to_le_bytes());
            self.bytes.extend_from_slice(bytes);
            Ok(())
        }
    }

    pub(crate) static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Publish complete image bytes through a same-directory temporary file and
    /// rename. The destination is never opened for truncation; failed writes clean
    /// up their temporary file and leave an existing destination untouched.
    pub fn publish_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let file_name = path.file_name().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "image path has no file name")
        })?;
        let mut temporary_name = std::ffi::OsString::from(".");
        temporary_name.push(file_name);
        temporary_name.push(format!(
            ".birnam-tmp-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let temporary_path = parent.join(temporary_name);

        let result = (|| -> io::Result<()> {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary_path)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary_path, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        result
    }

    fn read_u64(bytes: &[u8]) -> u64 {
        u64::from_le_bytes(bytes.try_into().expect("fixed-width image field"))
    }

    fn read_u32(bytes: &[u8]) -> u32 {
        u32::from_le_bytes(bytes.try_into().expect("fixed-width image field"))
    }

    const WYHASH_SECRET: [u64; 4] = [
        0xa076_1d64_78bd_642f,
        0xe703_7ed1_a0b4_28db,
        0x8ebc_6af0_9c88_c6e3,
        0x5899_65cc_7537_4cc3,
    ];

    pub(crate) fn wyhash(seed: u64, input: &[u8]) -> u64 {
        let mut state = [seed ^ mix(seed ^ WYHASH_SECRET[0], WYHASH_SECRET[1]); 3];
        let (a, b) = if input.len() <= 16 {
            let mut a = 0;
            let mut b = 0;
            small_key(input, &mut a, &mut b);
            (a, b)
        } else {
            let mut offset = 0;
            if input.len() >= 48 {
                while offset + 48 < input.len() {
                    round(&mut state, &input[offset..offset + 48]);
                    offset += 48;
                }
                state[0] ^= state[1] ^ state[2];
            }
            let remaining = &input[offset..];
            let mut inner = 0;
            while inner + 16 < remaining.len() {
                state[0] = mix(
                    read_u64(&remaining[inner..inner + 8]) ^ WYHASH_SECRET[1],
                    read_u64(&remaining[inner + 8..inner + 16]) ^ state[0],
                );
                inner += 16;
            }
            (
                read_u64(&input[input.len() - 16..input.len() - 8]),
                read_u64(&input[input.len() - 8..]),
            )
        };
        let mut a = a ^ WYHASH_SECRET[1];
        let mut b = b ^ state[0];
        let product = (a as u128).wrapping_mul(b as u128);
        a = product as u64;
        b = (product >> 64) as u64;
        mix(
            a ^ WYHASH_SECRET[0] ^ input.len() as u64,
            b ^ WYHASH_SECRET[1],
        )
    }

    fn small_key(input: &[u8], a: &mut u64, b: &mut u64) {
        if input.len() >= 4 {
            let end = input.len() - 4;
            let quarter = (input.len() >> 3) << 2;
            *a = (u64::from(read_u32(&input[..4])) << 32)
                | u64::from(read_u32(&input[quarter..quarter + 4]));
            *b = (u64::from(read_u32(&input[end..end + 4])) << 32)
                | u64::from(read_u32(&input[end - quarter..end - quarter + 4]));
        } else if !input.is_empty() {
            *a = (u64::from(input[0]) << 16)
                | (u64::from(input[input.len() >> 1]) << 8)
                | u64::from(input[input.len() - 1]);
        }
    }

    fn round(state: &mut [u64; 3], input: &[u8]) {
        for index in 0..3 {
            state[index] = mix(
                read_u64(&input[index * 16..index * 16 + 8]) ^ WYHASH_SECRET[index + 1],
                read_u64(&input[index * 16 + 8..index * 16 + 16]) ^ state[index],
            );
        }
    }

    fn mix(left: u64, right: u64) -> u64 {
        let product = (left as u128).wrapping_mul(right as u128);
        product as u64 ^ (product >> 64) as u64
    }
}

#[cfg(any(feature = "module", test))]
pub(crate) use support::authority_digest_from_bytes;
#[allow(unused_imports)]
pub use support::*;

#[cfg(feature = "module")]
mod runtime {
    use super::*;
    use birnam_abi::{
        IdApi, MarkFn, Oop, PORTABLE_CODE_API_V2_VERSION, PORTABLE_CODE_API_V3_VERSION,
        PORTABLE_PROGRAM_VIEW_API_V1_VERSION, PortableCodeApiV2, PortableCodeApiV3,
        PortableProgramViewApiV1, borrowed_oop_args,
    };
    use core::ffi::{c_char, c_int, c_void};
    use core::mem::size_of;
    use core::ptr;
    use sha2::{Digest, Sha256};
    use std::collections::HashMap;
    use std::ffi::{CStr, OsStr};
    use std::fs;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};

    const RUST_IMAGE_IDENTITY: &[u8] = b"rust-image-r8-portable\0";
    const IMAGE_REPORT_VT_NAME: &[u8] = b"ImageSaveReportVt\0";
    const IMAGE_REPORT_NAME: &[u8] = b"ImageSaveReport\0";
    const COLLECTOR_VT_NAME: &[u8] = b"ImageCollectorVt\0";
    const MAX_IMAGE_BYTES: u64 = 1 << 30;
    const RUST_CODEGEN_NAME: &[u8] = b"RustCodegen\0";
    const RUST_CODEGEN_INITIALIZE_CURRENT_GENERATION: &[u8] = b"initializeCurrentGeneration\0";
    const RUST_CODEGEN_BEGIN_IMAGE_RECOVERY: &[u8] = b"beginImageDispatchRecovery\0";
    const RUST_CODEGEN_END_IMAGE_RECOVERY: &[u8] = b"endImageDispatchRecovery\0";
    const COMPILE_PORTABLE_IMAGE_CODE: &[u8] = b"compilePortableImageCode:bindings:selection:\0";
    const BEGIN_PORTABLE_IMAGE_REBUILD: &[u8] = b"beginPortableImageRebuild:bindings:selection:\0";
    const BEGIN_PORTABLE_IMAGE_MAPPED: &[u8] =
        b"beginPortableImageMapped:programs:bindings:selection:\0";
    const PORTABLE_IMAGE_BLOCK_VTABLE: &[u8] = b"portableImageBlockVtable:\0";
    const PREPARE_PORTABLE_IMAGE_BINDING: &[u8] = b"preparePortableImageBinding:implementation:\0";
    const FINISH_PORTABLE_IMAGE_UNIT: &[u8] = b"finishPortableImageUnit\0";
    const CANCEL_PORTABLE_IMAGE_UNIT: &[u8] = b"cancelPortableImageUnit\0";
    const INSTALL_PORTABLE_IMAGE_BINDING: &[u8] = b"installPortableImageBinding:on:as:source:\0";
    const MACHINE_NAME: &[u8] = b"Machine\0";
    const FRAME_VIEW_NAME: &[u8] = b"FrameView\0";
    const CONTINUATION_NAME: &[u8] = b"Continuation\0";
    const CONTINUE_ACTION_NAME: &[u8] = b"ContinueAction\0";
    const CALL_ACTION_NAME: &[u8] = b"CallAction\0";
    const TAIL_CALL_ACTION_NAME: &[u8] = b"TailCallAction\0";
    const RETURN_ACTION_NAME: &[u8] = b"ReturnAction\0";
    const SUSPEND_ACTION_NAME: &[u8] = b"SuspendAction\0";
    const CAPTURE_ACTION_NAME: &[u8] = b"CaptureAction\0";
    const RESTORE_ACTION_NAME: &[u8] = b"RestoreAction\0";
    const BOUNDARY_CALL_ACTION_NAME: &[u8] = b"BoundaryCallAction\0";
    const EXIT_BOUNDARY_ACTION_NAME: &[u8] = b"ExitBoundaryAction\0";
    const ABORT_ACTION_NAME: &[u8] = b"AbortAction\0";
    const DISCARD_RECEIPT_NAME: &[u8] = b"DiscardReceipt\0";
    const FOREIGN_ADDRESS_NAME: &[u8] = b"ForeignAddress\0";
    const PORTABLE_BLOCK_CARRIER_VTABLE_NAME: &[u8] = b"_BirnamPortableBlockCarrierVt";
    const PORTABLE_PROGRAM_VIEW_API_NAME: &[u8] = b"_BirnamPortableProgramViewApiV1\0";
    const PORTABLE_PROGRAM_VIEW_API_VTABLE_NAME: &[u8] = b"_BirnamPortableProgramViewApiV1Vt\0";
    const PORTABLE_CODE_API_V3_NAME: &[u8] = b"_BirnamPortableCodeApiV3\0";
    const PORTABLE_CODE_API_V3_VTABLE_NAME: &[u8] = b"_BirnamPortableCodeApiV3Vt\0";
    const PORTABLE_CODE_API_V2_NAME: &[u8] = b"_BirnamPortableCodeApiV2\0";
    const PORTABLE_CODE_API_V2_VTABLE_NAME: &[u8] = b"_BirnamPortableCodeApiV2Vt\0";

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ImageRejectionKind {
        Machine,
        StepAction,
        FrameView,
        Continuation,
        DiscardReceipt,
        ForeignAddress,
    }

    impl ImageRejectionKind {
        fn symbol(self) -> &'static [u8] {
            match self {
                Self::Machine => b"machineNotImageable\0",
                Self::StepAction => b"stepActionNotImageable\0",
                Self::FrameView => b"frameViewNotImageable\0",
                Self::Continuation => b"continuationNotImageable\0",
                Self::DiscardReceipt => b"discardReceiptNotImageable\0",
                Self::ForeignAddress => b"foreignAddressNotImageable\0",
            }
        }

        fn detail(self) -> &'static str {
            match self {
                Self::Machine => "Rust Image rejected process-local Machine",
                Self::StepAction => "Rust Image rejected process-local StepAction",
                Self::FrameView => "Rust Image rejected process-local FrameView",
                Self::Continuation => "Rust Image rejected process-local Continuation",
                Self::DiscardReceipt => "Rust Image rejected process-local DiscardReceipt",
                Self::ForeignAddress => "Rust Image rejected process-local ForeignAddress",
            }
        }
    }

    #[derive(Clone, Copy)]
    struct Runtime {
        api: usize,
        vtables: usize,
        imports: usize,
        report_obj: usize,
        report: &'static Mutex<SaveReport>,
    }

    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    static ROOT_REGISTERED: OnceLock<()> = OnceLock::new();
    unsafe extern "C" fn mark_runtime_roots(mark: MarkFn) {
        if let Some(runtime) = RUNTIME.get() {
            unsafe { mark(runtime.report_obj as Oop) };
        }
    }

    struct FactCollection {
        writer: retained_facts::FactsWriter,
        heap: HeapPreflight,
        kept: u32,
        recovered: u32,
        recoveries_needing_code: u32,
        rejected: u32,
    }

    struct FactPreflight {
        kept: u32,
        recovered: u32,
        rejected: u32,
        facts_payload: Vec<u8>,
        heap_payload: Option<Vec<u8>>,
        heap_ok: bool,
        recovered_slots: u32,
        rejected_slots: u32,
        writer_ok: bool,
        image_rejection: Option<ImageRejectionKind>,
    }

    static FACT_COLLECTION: Mutex<Option<FactCollection>> = Mutex::new(None);
    struct PortableSaveRecords {
        programs: Vec<u8>,
        bindings: Vec<u8>,
        selection: Vec<u8>,
    }
    static PORTABLE_SAVE_RECORDS: Mutex<Option<PortableSaveRecords>> = Mutex::new(None);
    static IMAGE_COMPOSITION_IDENTITY: Mutex<Option<[u8; 64]>> = Mutex::new(None);

    #[repr(C)]
    struct DlInfo {
        filename: *const c_char,
        base: *mut c_void,
        symbol_name: *const c_char,
        symbol_address: *mut c_void,
    }

    #[cfg_attr(target_os = "linux", link(name = "dl"))]
    unsafe extern "C" {
        fn dladdr(address: *const c_void, info: *mut DlInfo) -> c_int;
    }

    unsafe fn floor_symbol(api: &IdApi, name: &[u8]) -> Option<usize> {
        if name.last() != Some(&0) {
            return None;
        }
        static FLOOR_HANDLE: OnceLock<usize> = OnceLock::new();
        let handle = *FLOOR_HANDLE.get_or_init(|| {
            let mut info = DlInfo {
                filename: ptr::null(),
                base: ptr::null_mut(),
                symbol_name: ptr::null(),
                symbol_address: ptr::null_mut(),
            };
            // The host loads the selected floor RTLD_LOCAL. Resolve private
            // image seams from that exact image, never from global symbol order.
            if unsafe { dladdr(api.dlsym as *const () as *const c_void, &mut info) } == 0
                || info.filename.is_null()
            {
                return 0;
            }
            #[cfg(target_os = "macos")]
            const RTLD_NOW_LOCAL: c_int = 2 | 4;
            #[cfg(not(target_os = "macos"))]
            const RTLD_NOW_LOCAL: c_int = 2;
            unsafe { (api.dlopen)(info.filename, RTLD_NOW_LOCAL) as usize }
        });
        if handle == 0 {
            return None;
        }
        let address = unsafe { (api.dlsym)(handle as *mut c_void, name.as_ptr().cast()) };
        (!address.is_null()).then_some(address as usize)
    }

    struct HeapPreflight {
        authority: Option<[u8; AUTHORITY_DIGEST_LEN]>,
        objects: Vec<usize>,
        relocations: HashMap<usize, u32>,
        portable_programs: Vec<(usize, Vec<u8>)>,
        recovered_slots: u32,
        rejected_slots: u32,
        image_rejection: Option<ImageRejectionKind>,
    }

    #[derive(Clone, Copy)]
    struct PortableBindingSnapshot {
        target: Oop,
        selector: Oop,
        implementation: Oop,
        provenance: Oop,
    }

    enum OwnedBindingField {
        Nil,
        True,
        False,
        Symbol(Vec<u8>),
        HeapRef(u32),
        ExportRef(Vec<u8>),
    }

    impl OwnedBindingField {
        fn borrowed(&self) -> birnam_artifact::code_binding_inventory::Field<'_> {
            use birnam_artifact::code_binding_inventory::Field;
            match self {
                Self::Nil => Field::Nil,
                Self::True => Field::True,
                Self::False => Field::False,
                Self::Symbol(bytes) => Field::Symbol(bytes),
                Self::HeapRef(index) => Field::HeapRef(*index),
                Self::ExportRef(bytes) => Field::ExportRef(bytes),
            }
        }
    }

    struct OwnedBinding {
        target: OwnedBindingField,
        selector: Vec<u8>,
        implementation: OwnedBindingField,
        provenance: OwnedBindingField,
    }

    unsafe fn vector_values(api: &IdApi, vector: Oop) -> Option<Vec<Oop>> {
        let size = unsafe { send0(api, vector, b"size\0") };
        if unsafe { (birnam_abi::integer::is_integer)(size) } == 0 {
            return None;
        }
        let size = usize::try_from(unsafe { (birnam_abi::integer::value)(size) }).ok()?;
        if size > 1_048_576 {
            return None;
        }
        (0..size)
            .map(|index| {
                let index = i64::try_from(index).ok()?;
                Some(unsafe { send1(api, vector, b"at:\0", (birnam_abi::integer::make)(index)) })
            })
            .collect()
    }

    unsafe fn portable_binding_snapshot(api: &IdApi) -> Option<Vec<PortableBindingSnapshot>> {
        let codegen = unsafe { birnam_abi::namespace_import::import(execution_imports(), RUST_CODEGEN_NAME.as_ptr().cast()) };
        if codegen.is_null() || codegen == api.nilObj {
            return None;
        }
        let rows = unsafe { send0(api, codegen, b"portableImageBindings\0") };
        let rows = unsafe { vector_values(api, rows) }?;
        if rows.len() > 262_144 {
            return None;
        }
        rows.into_iter()
            .map(|row| {
                let fields = unsafe { vector_values(api, row) }?;
                (fields.len() == 4).then_some(PortableBindingSnapshot {
                    target: fields[0],
                    selector: fields[1],
                    implementation: fields[2],
                    provenance: fields[3],
                })
            })
            .collect()
    }

    unsafe fn runtime_oop_is_canonical_reference(api: &IdApi, object: Oop) -> bool {
        if object.is_null()
            || object == api.nilObj
            || object == api.trueObj
            || object == api.falseObj
        {
            return true;
        }
        let vtable = unsafe { (api.vtableOf)(object) };
        (unsafe { vtable_inherits_from(api, vtable, api.symbol_vt) })
            || unsafe { c_string_bytes((birnam_abi::namespace_names::name_of)(object)) }.is_some()
    }

    unsafe fn vtable_inherits_from(api: &IdApi, mut vtable: Oop, ancestor: Oop) -> bool {
        let mut remaining = unsafe { birnam_abi::vtable_enumeration::count(runtime().vtables as *const _) }.saturating_add(1);
        while !vtable.is_null() && remaining != 0 {
            if vtable == ancestor {
                return true;
            }
            vtable = unsafe { birnam_abi::vtable_parent(api, vtable) };
            remaining -= 1;
        }
        false
    }

    enum BoxedValue {
        Integer(i64),
        Float(u64),
        Word(u64),
        String(Vec<u8>),
    }

    unsafe fn boxed_value(api: &IdApi, object: Oop) -> Option<BoxedValue> {
        if object.is_null() || unsafe { c_string_bytes((birnam_abi::namespace_names::name_of)(object)) }.is_some() {
            return None;
        }
        let vtable = unsafe { (api.vtableOf)(object) };
        if unsafe { vtable_inherits_from(api, vtable, birnam_abi::integer::behavior()) } {
            return Some(BoxedValue::Integer(unsafe { (birnam_abi::integer::value)(object) }));
        }
        if unsafe { vtable_inherits_from(api, vtable, birnam_abi::float::behavior()) } {
            return Some(BoxedValue::Float(
                unsafe { (birnam_abi::float::value)(object) }.to_bits(),
            ));
        }
        if unsafe { vtable_inherits_from(api, vtable, birnam_abi::word::behavior()) } {
            return Some(BoxedValue::Word(unsafe { (birnam_abi::word::value)(object) }));
        }
        if unsafe { vtable_inherits_from(api, vtable, birnam_abi::string::behavior()) } {
            let mut length = 0usize;
            let pointer = unsafe { (birnam_abi::string::bytes)(object, &mut length) };
            if pointer.is_null() && length != 0 {
                return None;
            }
            let bytes = if length == 0 {
                Vec::new()
            } else {
                unsafe { core::slice::from_raw_parts(pointer, length) }.to_vec()
            };
            return Some(BoxedValue::String(bytes));
        }
        None
    }

    unsafe fn belongs_to_exported_capability_family(
        api: &IdApi,
        object: Oop,
        prototype_name: &[u8],
    ) -> bool {
        let prototype = unsafe { birnam_abi::namespace_import::import(execution_imports(), prototype_name.as_ptr().cast()) };
        if prototype.is_null() || prototype == api.nilObj || object == prototype {
            return false;
        }
        let family_vtable = unsafe { (api.vtableOf)(prototype) };
        let object_vtable = unsafe { (api.vtableOf)(object) };
        !family_vtable.is_null()
            && !object_vtable.is_null()
            && unsafe { vtable_inherits_from(api, object_vtable, family_vtable) }
    }

    unsafe fn image_rejection_kind(api: &IdApi, object: Oop) -> Option<ImageRejectionKind> {
        if unsafe { belongs_to_exported_capability_family(api, object, MACHINE_NAME) } {
            return Some(ImageRejectionKind::Machine);
        }
        if [
            CONTINUE_ACTION_NAME,
            CALL_ACTION_NAME,
            TAIL_CALL_ACTION_NAME,
            RETURN_ACTION_NAME,
            SUSPEND_ACTION_NAME,
            CAPTURE_ACTION_NAME,
            RESTORE_ACTION_NAME,
            ABORT_ACTION_NAME,
            BOUNDARY_CALL_ACTION_NAME,
            EXIT_BOUNDARY_ACTION_NAME,
        ]
        .into_iter()
        .any(|name| unsafe { belongs_to_exported_capability_family(api, object, name) })
        {
            return Some(ImageRejectionKind::StepAction);
        }
        if unsafe { belongs_to_exported_capability_family(api, object, FRAME_VIEW_NAME) } {
            return Some(ImageRejectionKind::FrameView);
        }
        if unsafe { belongs_to_exported_capability_family(api, object, CONTINUATION_NAME) } {
            return Some(ImageRejectionKind::Continuation);
        }
        if unsafe { belongs_to_exported_capability_family(api, object, DISCARD_RECEIPT_NAME) } {
            return Some(ImageRejectionKind::DiscardReceipt);
        }
        if unsafe { belongs_to_exported_capability_family(api, object, FOREIGN_ADDRESS_NAME) } {
            return Some(ImageRejectionKind::ForeignAddress);
        }
        None
    }

    impl HeapPreflight {
        fn new(authority: Option<[u8; AUTHORITY_DIGEST_LEN]>) -> Self {
            Self {
                authority,
                objects: Vec::new(),
                relocations: HashMap::new(),
                portable_programs: Vec::new(),
                recovered_slots: 0,
                rejected_slots: 0,
                image_rejection: None,
            }
        }

        unsafe fn intern(&mut self, api: &IdApi, object: Oop) -> Option<u32> {
            if let Some(kind) = unsafe { image_rejection_kind(api, object) } {
                self.image_rejection.get_or_insert(kind);
                return None;
            }
            let key = object as usize;
            if let Some(id) = self.relocations.get(&key) {
                return Some(*id);
            }
            let vtable = unsafe { (api.vtableOf)(object) };
            if vtable.is_null() {
                return None;
            }
            if unsafe { boxed_value(api, object) }.is_some() {
                let id = u32::try_from(self.objects.len()).ok()?;
                self.relocations.insert(key, id);
                self.objects.push(key);
                return Some(id);
            }
            let portable_program = unsafe { copy_code_object_program(api, object) };
            let portable_block = unsafe { portable_block_code_object(api, vtable) };
            let has_exported_vtable =
                unsafe { c_string_bytes((birnam_abi::namespace_names::name_of)(vtable)) }.is_some();
            let has_resumable_vtable =
                unsafe { self.block_vtable_image_reference(api, vtable).is_some() };
            let owned_parent = unsafe { birnam_abi::vtable_owner(api, vtable) } == object;
            let parent = unsafe { birnam_abi::vtable_parent(api, vtable) };
            let has_exported_parent = owned_parent
                && !parent.is_null()
                && unsafe { c_string_bytes((birnam_abi::namespace_names::name_of)(parent)) }.is_some();
            if !has_exported_vtable
                && !has_resumable_vtable
                && !has_exported_parent
                && portable_block.is_none()
            {
                return None;
            }
            let payload_size = unsafe { (api.objectPayloadSize)(object) };
            if !payload_size.is_multiple_of(core::mem::size_of::<usize>()) {
                return None;
            }
            let words = payload_size / core::mem::size_of::<usize>();
            for index in 0..words {
                let kind = unsafe { (api.slotKind)(vtable, index) };
                if kind != birnam_abi::SLOT_RAW
                    && kind != birnam_abi::SLOT_OOP
                    && kind != birnam_abi::SLOT_CODE
                {
                    return None;
                }
            }
            let id = u32::try_from(self.objects.len()).ok()?;
            self.relocations.insert(key, id);
            self.objects.push(key);
            if let Some(program) = portable_program {
                self.portable_programs.push((key, program));
                let provenance = unsafe { send0(api, object, b"sourceProvenance\0") };
                if !unsafe { runtime_oop_is_canonical_reference(api, provenance) }
                    && unsafe { c_string_bytes((birnam_abi::namespace_names::name_of)(provenance)) }.is_none()
                {
                    unsafe { self.intern(api, provenance) }?;
                }
                return Some(id);
            }
            for index in 0..words {
                if unsafe { (api.slotKind)(vtable, index) } != birnam_abi::SLOT_OOP {
                    continue;
                }
                let address =
                    (object as *const u8).wrapping_add(index * core::mem::size_of::<usize>());
                // SAFETY: the layout marks this initialized payload word as an Oop.
                let child = unsafe { core::ptr::read_unaligned(address.cast::<usize>()) as Oop };
                if unsafe { runtime_oop_is_canonical_reference(api, child) }
                    || unsafe { self.block_vtable_image_reference(api, child).is_some() }
                {
                    continue;
                }
                unsafe { self.intern(api, child) }?;
            }
            Some(id)
        }

        unsafe fn finish_v8(&mut self, api: &IdApi) -> Option<(Vec<u8>, Vec<u8>)> {
            let roots = self
                .portable_programs
                .iter()
                .map(|(_, program)| program.as_slice())
                .collect::<Vec<_>>();
            let programs = birnam_artifact::portable_inventory::encode_closure(&roots).ok()?;
            let inventory = birnam_artifact::portable_inventory::inspect(&programs).ok()?;
            let mut payload = Vec::new();
            payload.extend_from_slice(&u32::try_from(self.objects.len()).ok()?.to_le_bytes());
            for object_index in 0..self.objects.len() {
                let object = self.objects[object_index] as Oop;
                if let Some(kind) = unsafe { image_rejection_kind(api, object) } {
                    self.image_rejection.get_or_insert(kind);
                    return None;
                }
                let vtable = unsafe { (api.vtableOf)(object) };
                if let Some(boxed) = unsafe { boxed_value(api, object) } {
                    match boxed {
                        BoxedValue::Integer(value) => {
                            payload.push(13);
                            payload.extend_from_slice(&value.to_le_bytes());
                        }
                        BoxedValue::Float(bits) => {
                            payload.push(14);
                            payload.extend_from_slice(&bits.to_le_bytes());
                        }
                        BoxedValue::Word(value) => {
                            payload.push(15);
                            payload.extend_from_slice(&value.to_le_bytes());
                        }
                        BoxedValue::String(bytes) => {
                            payload.push(16);
                            payload
                                .extend_from_slice(&u32::try_from(bytes.len()).ok()?.to_le_bytes());
                            payload.extend_from_slice(&bytes);
                        }
                    }
                    payload.extend_from_slice(&0u32.to_le_bytes());
                    continue;
                }
                if let Some(program) = self
                    .portable_programs
                    .iter()
                    .find(|(candidate, _)| *candidate == object as usize)
                    .map(|(_, program)| program)
                {
                    encode_v8_name_field(&mut payload, 9, b"CodeObjectVt")?;
                    payload.extend_from_slice(&2u32.to_le_bytes());
                    let program_index = (0..inventory.program_count as usize)
                        .find(|index| inventory.program(*index) == Some(program.as_slice()))?;
                    payload.push(11);
                    payload.extend_from_slice(&u32::try_from(program_index).ok()?.to_le_bytes());
                    let provenance = unsafe { send0(api, object, b"sourceProvenance\0") };
                    unsafe { encode_v8_oop(api, &mut payload, provenance, self) }?;
                    continue;
                }
                let portable_code_object = unsafe { portable_block_code_object(api, vtable) };
                let layout_vtable = if portable_code_object.is_some()
                    || unsafe { c_string_bytes((birnam_abi::namespace_names::name_of)(vtable)) }.is_some()
                {
                    vtable
                } else {
                    let parent = unsafe { birnam_abi::vtable_parent(api, vtable) };
                    unsafe { c_string_bytes((birnam_abi::namespace_names::name_of)(parent)) }?;
                    parent
                };
                if let Some(code_object) = portable_code_object {
                    let code_object = *self.relocations.get(&(code_object as usize))?;
                    payload.push(12);
                    payload.extend_from_slice(&code_object.to_le_bytes());
                } else {
                    let name = unsafe { c_string_bytes((birnam_abi::namespace_names::name_of)(layout_vtable)) }?;
                    encode_v8_name_field(&mut payload, 9, &name)?;
                }
                let payload_size = unsafe { (api.objectPayloadSize)(object) };
                let words = payload_size / size_of::<usize>();
                payload.extend_from_slice(&u32::try_from(words).ok()?.to_le_bytes());
                for index in 0..words {
                    let kind = unsafe { (api.slotKind)(layout_vtable, index) };
                    if kind == birnam_abi::SLOT_CODE {
                        self.rejected_slots = self.rejected_slots.saturating_add(1);
                        return None;
                    }
                    let address = (object as *const u8).wrapping_add(index * size_of::<usize>());
                    if kind == birnam_abi::SLOT_OOP {
                        let child =
                            unsafe { core::ptr::read_unaligned(address.cast::<usize>()) as Oop };
                        unsafe { encode_v8_oop(api, &mut payload, child, self) }?;
                    } else if kind == birnam_abi::SLOT_RAW {
                        let word = unsafe { core::ptr::read_unaligned(address.cast::<i64>()) };
                        payload.push(3);
                        payload.extend_from_slice(&word.to_le_bytes());
                    } else {
                        return None;
                    }
                }
            }
            Some((payload, programs))
        }

        unsafe fn block_vtable_image_reference(&self, api: &IdApi, vtable: Oop) -> Option<u32> {
            let authority = self.authority.as_ref()?;
            let raw_authority = decode_lower_hex(authority)?;
            let address = unsafe {
                floor_symbol(
                    api,
                    b"birnam_id_stackless_block_vtable_image_reference_v1\0",
                )
            }?;
            // SAFETY: the selected floor exports this exact loader-private v1
            // signature. `floor_symbol` binds the image that owns this IdApi.
            let query: unsafe extern "C" fn(*const u8, usize, Oop, *mut u32) -> u32 =
                unsafe { core::mem::transmute(address) };
            let mut index = 0;
            (unsafe {
                query(
                    raw_authority.as_ptr(),
                    raw_authority.len(),
                    vtable,
                    &mut index,
                )
            } == 1)
                .then_some(index)
        }
    }

    fn encode_v8_name_field(output: &mut Vec<u8>, tag: u8, bytes: &[u8]) -> Option<()> {
        if bytes.is_empty() || bytes.contains(&0) {
            return None;
        }
        output.push(tag);
        output.extend_from_slice(&u32::try_from(bytes.len()).ok()?.to_le_bytes());
        output.extend_from_slice(bytes);
        Some(())
    }

    unsafe fn encode_v8_oop(
        api: &IdApi,
        output: &mut Vec<u8>,
        object: Oop,
        heap: &HeapPreflight,
    ) -> Option<()> {
        if object.is_null() {
            output.push(8);
        } else if object == api.nilObj {
            output.push(0);
        } else if object == api.trueObj {
            output.push(1);
        } else if object == api.falseObj {
            output.push(2);
        } else if unsafe { vtable_inherits_from(api, (api.vtableOf)(object), api.symbol_vt) } {
            encode_v8_name_field(output, 5, &unsafe {
                c_string_bytes((api.symbolName)(object))
            }?)?;
        } else if let Some(name) = unsafe { c_string_bytes((birnam_abi::namespace_names::name_of)(object)) } {
            encode_v8_name_field(output, 9, &name)?;
        } else if unsafe { (api.vtableOf)(object) } == api.vtable_vt
            && let Some(code_object) = unsafe { portable_block_code_object(api, object) }
        {
            output.push(12);
            output.extend_from_slice(&heap.relocations.get(&(code_object as usize))?.to_le_bytes());
        } else {
            output.push(7);
            output.extend_from_slice(&heap.relocations.get(&(object as usize))?.to_le_bytes());
        }
        Some(())
    }

    unsafe fn binding_field(
        api: &IdApi,
        object: Oop,
        heap: &HeapPreflight,
        provenance: bool,
    ) -> Option<OwnedBindingField> {
        if object == api.nilObj {
            return Some(OwnedBindingField::Nil);
        }
        if object == api.trueObj {
            return Some(OwnedBindingField::True);
        }
        if object == api.falseObj {
            return Some(OwnedBindingField::False);
        }
        let vtable = unsafe { (api.vtableOf)(object) };
        if unsafe { vtable_inherits_from(api, vtable, api.symbol_vt) } && !provenance {
            return Some(OwnedBindingField::Symbol(unsafe {
                c_string_bytes((api.symbolName)(object))
            }?));
        }
        if let Some(name) = unsafe { c_string_bytes((birnam_abi::namespace_names::name_of)(object)) } {
            return Some(OwnedBindingField::ExportRef(name));
        }
        heap.relocations
            .get(&(object as usize))
            .copied()
            .map(OwnedBindingField::HeapRef)
    }

    unsafe fn portable_binding_records(
        api: &IdApi,
        snapshot: &[PortableBindingSnapshot],
        heap: &HeapPreflight,
        programs: &[u8],
    ) -> Option<(Vec<u8>, Vec<u8>)> {
        let mut owned = Vec::with_capacity(snapshot.len());
        for binding in snapshot {
            owned.push(OwnedBinding {
                target: unsafe { binding_field(api, binding.target, heap, false) }?,
                selector: unsafe { symbol_bytes(api, binding.selector) }?,
                implementation: unsafe { binding_field(api, binding.implementation, heap, false) }?,
                provenance: unsafe { binding_field(api, binding.provenance, heap, true) }?,
            });
        }
        let specs = owned
            .iter()
            .map(
                |binding| birnam_artifact::code_binding_inventory::BindingSpec {
                    target: binding.target.borrowed(),
                    selector: &binding.selector,
                    implementation: binding.implementation.borrowed(),
                    provenance: binding.provenance.borrowed(),
                },
            )
            .collect::<Vec<_>>();
        let bindings = birnam_artifact::code_binding_inventory::encode(&specs).ok()?;
        let program_count = birnam_artifact::portable_inventory::inspect(programs)
            .ok()?
            .program_count as usize;
        let selection = birnam_artifact::materialization_selection::encode(
            programs,
            &bindings,
            &vec![birnam_artifact::materialization_selection::Disposition::Resumable; owned.len()],
            &vec![
                birnam_artifact::materialization_selection::Disposition::Resumable;
                program_count
            ],
        )
        .ok()?;
        Some((bindings, selection))
    }

    #[allow(dead_code)]
    #[derive(Clone, Copy, Default)]
    enum ReportStatus {
        #[default]
        Never,
        Saved,
        Rejected,
        IoFailed,
    }

    #[derive(Clone, Copy)]
    struct SaveReport {
        status: ReportStatus,
        saved_facts: u32,
        recovered_facts: u32,
        rejected_facts: u32,
        recovered_slots: u32,
        rejected_slots: u32,
        code_units: u32,
        detail: [u8; 256],
        detail_len: usize,
    }

    impl Default for SaveReport {
        fn default() -> Self {
            Self {
                status: ReportStatus::Never,
                saved_facts: 0,
                recovered_facts: 0,
                rejected_facts: 0,
                recovered_slots: 0,
                rejected_slots: 0,
                code_units: 0,
                detail: [0; 256],
                detail_len: 0,
            }
        }
    }

    static SAVE_REPORT: Mutex<SaveReport> = Mutex::new(SaveReport {
        status: ReportStatus::Never,
        saved_facts: 0,
        recovered_facts: 0,
        rejected_facts: 0,
        recovered_slots: 0,
        rejected_slots: 0,
        code_units: 0,
        detail: [0; 256],
        detail_len: 0,
    });

    fn abort(message: &str) -> ! {
        eprintln!("birnam Rust Image: {message}");
        std::process::abort();
    }

    fn runtime() -> &'static Runtime {
        RUNTIME
            .get()
            .unwrap_or_else(|| abort("module_init was not called"))
    }

    unsafe fn api_ref() -> &'static IdApi {
        // SAFETY: module_init records the process-owned IdApi address and the host
        // retains that table for the loaded floor's lifetime.
        unsafe { &*(runtime().api as *const IdApi) }
    }

    fn execution_imports() -> *const birnam_abi::namespace_import::ApiV1 { runtime().imports as *const birnam_abi::namespace_import::ApiV1 }

    unsafe fn intern(api: &IdApi, selector: &[u8]) -> Oop {
        // SAFETY: all current selectors are static NUL-terminated byte strings.
        unsafe { (api.intern)(selector.as_ptr().cast::<c_char>()) }
    }

    unsafe fn send0(api: &IdApi, receiver: Oop, selector: &[u8]) -> Oop {
        // Keep the argument pointer non-null even for a zero-argument send. Native
        // primitive methods use a non-null slice pointer in their native signature;
        // passing an empty Rust slice preserves that contract across the C ABI.
        let arguments: [Oop; 0] = [];
        unsafe { (api.send)(receiver, intern(api, selector), arguments.as_ptr(), 0) }
    }

    unsafe fn send1(api: &IdApi, receiver: Oop, selector: &[u8], argument: Oop) -> Oop {
        let arguments = [argument];
        unsafe {
            (api.send)(
                receiver,
                intern(api, selector),
                arguments.as_ptr(),
                arguments.len(),
            )
        }
    }

    unsafe fn send2(api: &IdApi, receiver: Oop, selector: &[u8], first: Oop, second: Oop) -> Oop {
        let arguments = [first, second];
        unsafe {
            (api.send)(
                receiver,
                intern(api, selector),
                arguments.as_ptr(),
                arguments.len(),
            )
        }
    }

    unsafe fn image_rejection_error(api: &IdApi, kind: ImageRejectionKind) -> Oop {
        let error = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"Error".as_ptr()) };
        if error.is_null() || error == api.nilObj {
            return api.nilObj;
        }
        let arguments = [unsafe { intern(api, kind.symbol()) }];
        unsafe {
            (api.send)(
                error,
                intern(api, b"kind:\0"),
                arguments.as_ptr(),
                arguments.len(),
            )
        }
    }

    unsafe fn c_string_bytes(pointer: *const c_char) -> Option<Vec<u8>> {
        if pointer.is_null() {
            return None;
        }
        // SAFETY: IdApi lends one NUL-terminated name while the owning Symbol
        // remains rooted. Copy it before any later allocation, send, or GC.
        Some(unsafe { CStr::from_ptr(pointer) }.to_bytes().to_vec())
    }

    unsafe fn oop_string_bytes(api: &IdApi, object: Oop) -> Option<Vec<u8>> {
        if object.is_null() || unsafe { (api.vtableOf)(object) } != unsafe { birnam_abi::string::behavior() } {
            return None;
        }
        let mut length = 0usize;
        let bytes = unsafe { (birnam_abi::string::bytes)(object, &mut length) };
        if bytes.is_null() && length != 0 {
            return None;
        }
        Some(if length == 0 {
            Vec::new()
        } else {
            unsafe { core::slice::from_raw_parts(bytes, length) }.to_vec()
        })
    }

    unsafe fn symbol_bytes(api: &IdApi, object: Oop) -> Option<Vec<u8>> {
        if object.is_null() || unsafe { (api.vtableOf)(object) } != api.symbol_vt {
            return None;
        }
        unsafe { c_string_bytes((api.symbolName)(object)) }
    }

    unsafe fn portable_program_view_api(api: &IdApi) -> Option<&'static PortableProgramViewApiV1> {
        let object = unsafe { birnam_abi::namespace_import::import(execution_imports(), PORTABLE_PROGRAM_VIEW_API_NAME.as_ptr().cast()) };
        let vtable = unsafe { birnam_abi::namespace_import::import(execution_imports(), PORTABLE_PROGRAM_VIEW_API_VTABLE_NAME.as_ptr().cast()) };
        if object.is_null()
            || object == api.nilObj
            || vtable.is_null()
            || vtable == api.nilObj
            || unsafe { (api.vtableOf)(object) } != vtable
            || unsafe { (api.objectPayloadSize)(object) } != size_of::<PortableProgramViewApiV1>()
        {
            return None;
        }
        let capability = unsafe { &*object.cast::<PortableProgramViewApiV1>() };
        (capability.abi_version == PORTABLE_PROGRAM_VIEW_API_V1_VERSION
            && capability.struct_size == size_of::<PortableProgramViewApiV1>() as u64)
            .then_some(capability)
    }

    unsafe fn portable_code_api_v3(api: &IdApi) -> Option<&'static PortableCodeApiV3> {
        let object = unsafe { birnam_abi::namespace_import::import(execution_imports(), PORTABLE_CODE_API_V3_NAME.as_ptr().cast()) };
        let vtable = unsafe { birnam_abi::namespace_import::import(execution_imports(), PORTABLE_CODE_API_V3_VTABLE_NAME.as_ptr().cast()) };
        if object.is_null()
            || object == api.nilObj
            || vtable.is_null()
            || vtable == api.nilObj
            || unsafe { (api.vtableOf)(object) } != vtable
            || unsafe { (api.objectPayloadSize)(object) } != size_of::<PortableCodeApiV3>()
        {
            return None;
        }
        let capability = unsafe { &*object.cast::<PortableCodeApiV3>() };
        (capability.abi_version == PORTABLE_CODE_API_V3_VERSION
            && capability.struct_size == size_of::<PortableCodeApiV3>() as u64)
            .then_some(capability)
    }

    unsafe fn portable_code_api_v2(api: &IdApi) -> Option<&'static PortableCodeApiV2> {
        let object = unsafe { birnam_abi::namespace_import::import(execution_imports(), PORTABLE_CODE_API_V2_NAME.as_ptr().cast()) };
        let vtable = unsafe { birnam_abi::namespace_import::import(execution_imports(), PORTABLE_CODE_API_V2_VTABLE_NAME.as_ptr().cast()) };
        if object.is_null()
            || object == api.nilObj
            || vtable.is_null()
            || vtable == api.nilObj
            || unsafe { (api.vtableOf)(object) } != vtable
            || unsafe { (api.objectPayloadSize)(object) } != size_of::<PortableCodeApiV2>()
        {
            return None;
        }
        let capability = unsafe { &*object.cast::<PortableCodeApiV2>() };
        (capability.abi_version == PORTABLE_CODE_API_V2_VERSION
            && capability.struct_size == size_of::<PortableCodeApiV2>() as u64)
            .then_some(capability)
    }

    unsafe fn copy_code_object_program(api: &IdApi, object: Oop) -> Option<Vec<u8>> {
        let vtable = unsafe { (api.vtableOf)(object) };
        if unsafe { c_string_bytes((birnam_abi::namespace_names::name_of)(vtable)) }.as_deref()
            != Some(b"CodeObjectVt")
        {
            return None;
        }
        let capability = unsafe { portable_program_view_api(api) }?;
        let length = unsafe { (capability.copy_program)(object, ptr::null_mut(), 0) };
        if length == 0 || length > 64 << 20 {
            return None;
        }
        let mut bytes = vec![0; length];
        if unsafe { (capability.copy_program)(object, bytes.as_mut_ptr(), bytes.len()) } != length
            || birnam_artifact::portable_program::inspect(&bytes).is_err()
        {
            return None;
        }
        Some(bytes)
    }

    unsafe fn portable_block_code_object(api: &IdApi, vtable: Oop) -> Option<Oop> {
        if vtable.is_null() {
            return None;
        }
        let owner = unsafe { birnam_abi::vtable_owner(api, vtable) };
        if owner.is_null() {
            return None;
        }
        let owner_vtable = unsafe { (api.vtableOf)(owner) };
        if unsafe { c_string_bytes((birnam_abi::namespace_names::name_of)(owner_vtable)) }.as_deref()
            != Some(PORTABLE_BLOCK_CARRIER_VTABLE_NAME)
            || unsafe { (api.objectPayloadSize)(owner) } != 2 * size_of::<Oop>()
        {
            return None;
        }
        let code_object = unsafe { owner.cast::<Oop>().read() };
        unsafe { copy_code_object_program(api, code_object) }.map(|_| code_object)
    }

    unsafe fn has_global_recovery_source(api: &IdApi, store: Oop, attribute: Oop) -> bool {
        let source_entity = unsafe { intern(api, b"__globalsrc__\0") };
        (unsafe { send2(api, store, b"hasValue:attr:\0", source_entity, attribute) }) == api.trueObj
    }

    #[derive(Clone, Copy)]
    enum DeferredFactClass {
        Source,
        Code,
        Reject,
    }

    unsafe fn deferred_fact_class(
        api: &IdApi,
        store: Oop,
        entity: Oop,
        attribute: Oop,
    ) -> DeferredFactClass {
        let Some(entity_name) = (unsafe { symbol_bytes(api, entity) }) else {
            return DeferredFactClass::Reject;
        };
        let Some(attribute_name) = (unsafe { symbol_bytes(api, attribute) }) else {
            return DeferredFactClass::Reject;
        };
        if entity_name == b"__globals__" {
            if attribute_name.starts_with(b"__") {
                return DeferredFactClass::Code;
            }
            if unsafe { has_global_recovery_source(api, store, attribute) } {
                return DeferredFactClass::Source;
            }
            return DeferredFactClass::Reject;
        }
        if unsafe { (api.vtableOf)(entity) } == api.vtable_vt {
            return DeferredFactClass::Code;
        }
        DeferredFactClass::Reject
    }

    unsafe fn encode_runtime_oop(
        api: &IdApi,
        encoder: &mut FieldEncoder,
        object: Oop,
        heap: &mut HeapPreflight,
    ) -> bool {
        if object.is_null() {
            encoder.null_oop();
            return true;
        }
        if object == api.nilObj {
            encoder.nil();
            return true;
        }
        if object == api.trueObj {
            encoder.true_value();
            return true;
        }
        if object == api.falseObj {
            encoder.false_value();
            return true;
        }
        if let Some(kind) = unsafe { image_rejection_kind(api, object) } {
            heap.image_rejection.get_or_insert(kind);
            return false;
        }
        if let Some(index) = unsafe { heap.block_vtable_image_reference(api, object) } {
            encoder.portable_block_vtable(index);
            return true;
        }
        let vtable = unsafe { (api.vtableOf)(object) };
        if unsafe { vtable_inherits_from(api, vtable, api.symbol_vt) } {
            let Some(bytes) = (unsafe { c_string_bytes((api.symbolName)(object)) }) else {
                return false;
            };
            return encoder.symbol(&bytes).is_ok();
        }
        if let Some(name) = unsafe { c_string_bytes((birnam_abi::namespace_names::name_of)(object)) } {
            return encoder.export_ref(&name).is_ok();
        }
        let Some(id) = (unsafe { heap.intern(api, object) }) else {
            return false;
        };
        encoder.heap_ref(id);
        true
    }

    fn record_fact_counts(
        kept: u32,
        recovered: u32,
        rejected: u32,
        recovered_slots: u32,
        rejected_slots: u32,
    ) {
        let mut report = runtime()
            .report
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        report.saved_facts = kept;
        report.recovered_facts = recovered;
        report.rejected_facts = rejected;
        report.recovered_slots = recovered_slots;
        report.rejected_slots = rejected_slots;
    }

    /// Select the image writer's generation authority explicitly.
    ///
    /// Mixed hosts retain the canonical `Codegen` owner. The Rust owner is only
    /// admitted by an explicit test/selection environment value, and must expose
    /// the durable-generation bootstrap plus capture selectors before it can be
    /// used. An unknown explicit value fails closed instead of silently falling
    /// back to another authority.
    unsafe fn writer_codegen_authority(api: &IdApi) -> Option<Oop> {
        let requested = std::env::var("BIRNAM_RUST_IMAGE_WRITER_AUTHORITY").ok();
        if requested.as_deref() == Some("rust") {
            let codegen = unsafe { birnam_abi::namespace_import::import(execution_imports(), RUST_CODEGEN_NAME.as_ptr().cast()) };
            if codegen.is_null()
                || codegen == api.nilObj
                || !unsafe {
                    direct_method_present(api, codegen, RUST_CODEGEN_INITIALIZE_CURRENT_GENERATION)
                        && direct_method_present(api, codegen, b"currentGenerationDigest\0")
                        && direct_method_present(api, codegen, b"compileImageCode:manifest:\0")
                }
            {
                return None;
            }
            let initialized =
                unsafe { send0(api, codegen, RUST_CODEGEN_INITIALIZE_CURRENT_GENERATION) };
            return (initialized != api.nilObj && !initialized.is_null()).then_some(codegen);
        }
        if requested.is_some() {
            return None;
        }
        let codegen = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"Codegen".as_ptr()) };
        if codegen.is_null() || codegen == api.nilObj {
            return None;
        }
        if rust_image_canonical_selected() {
            // Match the native image driver's first-save behavior for script hosts:
            // close the initial dispatch generation before asking Codegen for its
            // digest. Explicit Rust Image fixtures do not set the canonical marker
            // and therefore retain their diagnostic-only no-publication contract.
            let globals = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"Globals".as_ptr()) };
            if !globals.is_null() && globals != api.nilObj {
                let key = unsafe { intern(api, b"DispatchGeneration\0") };
                let generations = [key];
                let generations =
                    unsafe { (api.send)(globals, intern(api, b"at:\0"), generations.as_ptr(), 1) };
                if !generations.is_null() && generations != api.nilObj {
                    let current = unsafe { send0(api, generations, b"current\0") };
                    if current.is_null() || current == api.nilObj {
                        let _ = unsafe { send0(api, generations, b"initializeCurrent\0") };
                    }
                }
            }
        }
        Some(codegen)
    }

    fn rust_image_publication_selected() -> bool {
        std::env::var("BIRNAM_RUST_IMAGE_PUBLICATION")
            .ok()
            .as_deref()
            == Some("rust")
    }

    fn rust_image_writer_authority_selected() -> bool {
        std::env::var("BIRNAM_RUST_IMAGE_WRITER_AUTHORITY")
            .ok()
            .as_deref()
            == Some("rust")
    }

    fn rust_image_canonical_selected() -> bool {
        std::env::var("BIRNAM_RUST_IMAGE_CANONICAL").ok().as_deref() == Some("rust")
    }

    unsafe fn current_generation_digest(api: &IdApi) -> Option<[u8; AUTHORITY_DIGEST_LEN]> {
        let codegen = unsafe { writer_codegen_authority(api) }?;
        let digest = unsafe { send0(api, codegen, b"currentGenerationDigest\0") };
        if digest.is_null()
            || digest == api.nilObj
            || unsafe { (api.vtableOf)(digest) } != unsafe { birnam_abi::string::behavior() }
        {
            return None;
        }
        let mut length = 0;
        let bytes = unsafe { (birnam_abi::string::bytes)(digest, &mut length) };
        if bytes.is_null() && length != 0 {
            return None;
        }
        let bytes = if length == 0 {
            &[]
        } else {
            // SAFETY: stringBytes supplies `length` readable bytes for this live String.
            unsafe { core::slice::from_raw_parts(bytes, length) }
        };
        authority_digest_from_bytes(bytes)
    }

    unsafe fn compile_portable_image_code(
        api: &IdApi,
        records: &PortableSaveRecords,
    ) -> Option<Vec<u8>> {
        let codegen = unsafe { birnam_abi::namespace_import::import(execution_imports(), RUST_CODEGEN_NAME.as_ptr().cast()) };
        if codegen.is_null() || codegen == api.nilObj {
            return None;
        }
        let programs = unsafe { (birnam_abi::string::make)(records.programs.as_ptr(), records.programs.len()) };
        let bindings = unsafe { (birnam_abi::string::make)(records.bindings.as_ptr(), records.bindings.len()) };
        let selection =
            unsafe { (birnam_abi::string::make)(records.selection.as_ptr(), records.selection.len()) };
        let arguments = [programs, bindings, selection];
        let code = unsafe {
            (api.send)(
                codegen,
                intern(api, COMPILE_PORTABLE_IMAGE_CODE),
                arguments.as_ptr(),
                arguments.len(),
            )
        };
        if code.is_null() || code == api.nilObj || unsafe { (api.vtableOf)(code) } != unsafe { birnam_abi::string::behavior() }
        {
            return None;
        }
        unsafe { oop_string_bytes(api, code) }.filter(|bytes| !bytes.is_empty())
    }

    fn composition_identity(checksums: &[u64]) -> Option<[u8; 64]> {
        let mut digest = Sha256::new();
        digest.update(b"birnam.image-composition.v1\0");
        digest.update(u32::try_from(checksums.len()).ok()?.to_le_bytes());
        for checksum in checksums {
            digest.update(checksum.to_le_bytes());
        }
        let raw: [u8; 32] = digest.finalize().into();
        let mut spelling = [0; 64];
        for (index, byte) in raw.into_iter().enumerate() {
            spelling[index * 2] = b"0123456789abcdef"[(byte >> 4) as usize];
            spelling[index * 2 + 1] = b"0123456789abcdef"[(byte & 0x0f) as usize];
        }
        Some(spelling)
    }

    fn decode_lower_hex(spelling: &[u8]) -> Option<[u8; 32]> {
        if spelling.len() != 64 {
            return None;
        }
        fn nibble(value: u8) -> Option<u8> {
            match value {
                b'0'..=b'9' => Some(value - b'0'),
                b'a'..=b'f' => Some(value - b'a' + 10),
                _ => None,
            }
        }
        let mut raw = [0; 32];
        for (index, pair) in spelling.chunks_exact(2).enumerate() {
            raw[index] = nibble(pair[0])? << 4 | nibble(pair[1])?;
        }
        Some(raw)
    }

    #[derive(Clone, Copy)]
    enum V8Value<'a> {
        Nil,
        True,
        False,
        RawWord(u64),
        Symbol(&'a [u8]),
        HeapRef(u32),
        Null,
        ExportRef(&'a [u8]),
        ProgramRef(u32),
        PortableBlockVtable(u32),
        BoxedInteger(i64),
        BoxedFloat(u64),
        BoxedWord(u64),
        BoxedString(&'a [u8]),
    }

    struct V8HeapObject<'a> {
        locator: V8Value<'a>,
        fields: Vec<V8Value<'a>>,
    }

    fn parse_v8_value<'a>(bytes: &'a [u8], offset: &mut usize) -> Option<V8Value<'a>> {
        let tag = *bytes.get(*offset)?;
        *offset = offset.checked_add(1)?;
        match tag {
            0 => Some(V8Value::Nil),
            1 => Some(V8Value::True),
            2 => Some(V8Value::False),
            3 => {
                let end = offset.checked_add(8)?;
                let bits = u64::from_le_bytes(bytes.get(*offset..end)?.try_into().ok()?);
                *offset = end;
                Some(V8Value::RawWord(bits))
            }
            5 | 9 | 16 => {
                let end = offset.checked_add(4)?;
                let length = u32::from_le_bytes(bytes.get(*offset..end)?.try_into().ok()?) as usize;
                *offset = end;
                let end = offset.checked_add(length)?;
                let value = bytes.get(*offset..end)?;
                *offset = end;
                match tag {
                    5 => Some(V8Value::Symbol(value)),
                    9 => Some(V8Value::ExportRef(value)),
                    16 => Some(V8Value::BoxedString(value)),
                    _ => None,
                }
            }
            7 | 11 | 12 => {
                let end = offset.checked_add(4)?;
                let index = u32::from_le_bytes(bytes.get(*offset..end)?.try_into().ok()?);
                *offset = end;
                match tag {
                    7 => Some(V8Value::HeapRef(index)),
                    11 => Some(V8Value::ProgramRef(index)),
                    12 => Some(V8Value::PortableBlockVtable(index)),
                    _ => None,
                }
            }
            8 => Some(V8Value::Null),
            13..=15 => {
                let end = offset.checked_add(8)?;
                let bits = u64::from_le_bytes(bytes.get(*offset..end)?.try_into().ok()?);
                *offset = end;
                match tag {
                    13 => Some(V8Value::BoxedInteger(i64::from_le_bytes(
                        bits.to_le_bytes(),
                    ))),
                    14 => Some(V8Value::BoxedFloat(bits)),
                    15 => Some(V8Value::BoxedWord(bits)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn parse_v8_heaps<'a>(payloads: &[&'a [u8]]) -> Option<Vec<V8HeapObject<'a>>> {
        let mut objects = Vec::new();
        for payload in payloads {
            let mut offset = 0usize;
            let end = offset.checked_add(4)?;
            let count = u32::from_le_bytes(payload.get(offset..end)?.try_into().ok()?);
            offset = end;
            for _ in 0..count {
                let locator = parse_v8_value(payload, &mut offset)?;
                let end = offset.checked_add(4)?;
                let words = u32::from_le_bytes(payload.get(offset..end)?.try_into().ok()?);
                offset = end;
                let fields = (0..words)
                    .map(|_| parse_v8_value(payload, &mut offset))
                    .collect::<Option<Vec<_>>>()?;
                objects.push(V8HeapObject { locator, fields });
            }
            if offset != payload.len() {
                return None;
            }
        }
        Some(objects)
    }

    unsafe fn import_bytes(_api: &IdApi, name: &[u8]) -> Option<Oop> {
        let mut terminated = Vec::with_capacity(name.len().checked_add(1)?);
        terminated.extend_from_slice(name);
        terminated.push(0);
        let object = unsafe { birnam_abi::namespace_import::import(execution_imports(), terminated.as_ptr().cast()) };
        (!object.is_null()).then_some(object)
    }

    unsafe fn materialize_v8_value(
        api: &IdApi,
        value: V8Value<'_>,
        relocations: &[Oop],
    ) -> Option<Oop> {
        match value {
            V8Value::Nil => Some(api.nilObj),
            V8Value::True => Some(api.trueObj),
            V8Value::False => Some(api.falseObj),
            V8Value::Symbol(bytes) => {
                let mut terminated = Vec::with_capacity(bytes.len().checked_add(1)?);
                terminated.extend_from_slice(bytes);
                terminated.push(0);
                Some(unsafe { (api.intern)(terminated.as_ptr().cast()) })
            }
            V8Value::HeapRef(index) => relocations.get(index as usize).copied(),
            V8Value::Null => Some(ptr::null_mut()),
            V8Value::ExportRef(bytes) => unsafe { import_bytes(api, bytes) },
            V8Value::RawWord(_)
            | V8Value::ProgramRef(_)
            | V8Value::PortableBlockVtable(_)
            | V8Value::BoxedInteger(_)
            | V8Value::BoxedFloat(_)
            | V8Value::BoxedWord(_)
            | V8Value::BoxedString(_) => None,
        }
    }

    unsafe fn materialize_v8_heaps(
        api: &IdApi,
        decoded: &v8::DecodedImage<'_>,
        materialization_owner: Oop,
    ) -> Option<Vec<Oop>> {
        let objects = parse_v8_heaps(&decoded.heaps)?;
        let programs = birnam_artifact::portable_inventory::inspect(decoded.programs).ok()?;
        let mut relocations = vec![ptr::null_mut(); objects.len()];

        for (index, object) in objects.iter().enumerate() {
            relocations[index] = match object.locator {
                V8Value::BoxedInteger(value) => unsafe { (birnam_abi::integer::make)(value) },
                V8Value::BoxedFloat(bits) => unsafe { (birnam_abi::float::make)(f64::from_bits(bits)) },
                V8Value::BoxedWord(value) => unsafe { (birnam_abi::word::make)(value) },
                V8Value::BoxedString(bytes) => unsafe {
                    (birnam_abi::string::make)(bytes.as_ptr(), bytes.len())
                },
                _ => continue,
            };
            if relocations[index].is_null() {
                return None;
            }
        }

        // Ordinary exported-layout shells have no execution authority and may
        // be allocated before portable meaning. This also resolves the usual
        // heap provenance references before immutable CodeObjects are admitted.
        for (index, object) in objects.iter().enumerate() {
            let V8Value::ExportRef(name) = object.locator else {
                continue;
            };
            if name == b"CodeObjectVt" {
                continue;
            }
            let vtable = unsafe { import_bytes(api, name) }?;
            let bytes = object.fields.len().checked_mul(size_of::<usize>())?;
            relocations[index] = unsafe { (api.alloc)(vtable, bytes) };
            if relocations[index].is_null() {
                return None;
            }
        }

        let portable_api = unsafe { portable_code_api_v3(api) }?;
        for (index, object) in objects.iter().enumerate() {
            let V8Value::ExportRef(name) = object.locator else {
                continue;
            };
            if name != b"CodeObjectVt" || object.fields.len() != 2 {
                continue;
            }
            let V8Value::ProgramRef(program_index) = object.fields[0] else {
                return None;
            };
            let provenance = unsafe { materialize_v8_value(api, object.fields[1], &relocations) }?;
            let program = programs.program(program_index as usize)?;
            relocations[index] = unsafe {
                (portable_api.admit_program_with_provenance)(
                    program.as_ptr(),
                    program.len(),
                    provenance,
                )
            };
            if relocations[index].is_null() || relocations[index] == api.nilObj {
                return None;
            }
        }

        let codegen = unsafe { birnam_abi::namespace_import::import(execution_imports(), RUST_CODEGEN_NAME.as_ptr().cast()) };
        if codegen.is_null() || codegen == api.nilObj {
            return None;
        }
        let block_api = unsafe { portable_code_api_v2(api) }?;
        for (index, object) in objects.iter().enumerate() {
            let V8Value::PortableBlockVtable(code_object_index) = object.locator else {
                continue;
            };
            let code_object = *relocations.get(code_object_index as usize)?;
            let code_object_record = objects.get(code_object_index as usize)?;
            let V8Value::ProgramRef(program_index) = *code_object_record.fields.first()? else {
                return None;
            };
            let capture_count = object.fields.len().checked_sub(1)?;
            let index_oop = unsafe { (birnam_abi::integer::make)(i64::from(program_index)) };
            let vtable = unsafe { send1(api, codegen, PORTABLE_IMAGE_BLOCK_VTABLE, index_oop) };
            if vtable.is_null() || vtable == api.nilObj {
                return None;
            }
            let captures = vec![api.nilObj; capture_count];
            relocations[index] = unsafe {
                (block_api.new_materialized_block)(
                    vtable,
                    code_object,
                    captures.as_ptr(),
                    captures.len(),
                    materialization_owner,
                )
            };
            if relocations[index].is_null() || relocations[index] == api.nilObj {
                return None;
            }
        }

        for (index, object) in objects.iter().enumerate() {
            if matches!(object.locator, V8Value::ExportRef(b"CodeObjectVt")) {
                continue;
            }
            let receiver = *relocations.get(index)?;
            if receiver.is_null() {
                return None;
            }
            let vtable = unsafe { (api.vtableOf)(receiver) };
            for (word, field) in object.fields.iter().copied().enumerate() {
                let address = unsafe { receiver.cast::<usize>().add(word) };
                match unsafe { (api.slotKind)(vtable, word) } {
                    birnam_abi::SLOT_OOP => {
                        let value = unsafe { materialize_v8_value(api, field, &relocations) }?;
                        unsafe { address.write(value as usize) };
                    }
                    birnam_abi::SLOT_RAW => {
                        let V8Value::RawWord(value) = field else {
                            return None;
                        };
                        unsafe { address.write(value as usize) };
                    }
                    _ => return None,
                }
            }
        }
        Some(relocations)
    }

    struct StagedV8Facts {
        store: Oop,
        facts: Vec<[Oop; 6]>,
        basis: Oop,
        oldest: Oop,
        policy: Oop,
        argument: Oop,
    }

    unsafe fn prepare_v8_facts(
        api: &IdApi,
        payloads: &[&[u8]],
        relocations: &[Oop],
    ) -> Option<StagedV8Facts> {
        if payloads.is_empty() {
            return Some(StagedV8Facts {
                store: api.nilObj,
                facts: Vec::new(),
                basis: api.nilObj,
                oldest: api.nilObj,
                policy: api.nilObj,
                argument: api.nilObj,
            });
        }
        let store = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"Store".as_ptr()) };
        if store.is_null() || store == api.nilObj {
            return None;
        }
        let mut facts = Vec::new();
        let mut final_metadata = None;
        for payload in payloads {
            if payload.len() < 36 {
                return None;
            }
            let basis = u64::from_le_bytes(payload[0..8].try_into().ok()?);
            let oldest = u64::from_le_bytes(payload[8..16].try_into().ok()?);
            let policy = payload[16];
            let argument = u64::from_le_bytes(payload[24..32].try_into().ok()?);
            let count = u32::from_le_bytes(payload[32..36].try_into().ok()?);
            let metadata = (basis, oldest, policy, argument);
            if final_metadata
                .replace(metadata)
                .is_some_and(|old| old != metadata)
            {
                return None;
            }
            let mut offset = 36usize;
            for _ in 0..count {
                let op = *payload.get(offset)?;
                let historied = *payload.get(offset.checked_add(1)?)?;
                let tx_end = offset.checked_add(10)?;
                let tx = u64::from_le_bytes(payload.get(offset + 2..tx_end)?.try_into().ok()?);
                offset = tx_end;
                let entity = parse_v8_value(payload, &mut offset)?;
                let attribute = parse_v8_value(payload, &mut offset)?;
                let value = parse_v8_value(payload, &mut offset)?;
                facts.push([
                    unsafe { materialize_v8_value(api, entity, relocations) }?,
                    unsafe { materialize_v8_value(api, attribute, relocations) }?,
                    unsafe { materialize_v8_value(api, value, relocations) }?,
                    unsafe { (birnam_abi::integer::make)(tx as i64) },
                    unsafe { (birnam_abi::integer::make)(i64::from(op)) },
                    if historied == 1 {
                        api.trueObj
                    } else {
                        api.falseObj
                    },
                ]);
            }
            if offset != payload.len() {
                return None;
            }
        }
        let (basis, oldest, policy, argument) = final_metadata?;
        Some(StagedV8Facts {
            store,
            facts,
            basis: unsafe { (birnam_abi::integer::make)(basis as i64) },
            oldest: unsafe { (birnam_abi::integer::make)(oldest as i64) },
            policy: unsafe { (birnam_abi::integer::make)(i64::from(policy)) },
            argument: unsafe { (birnam_abi::integer::make)(argument as i64) },
        })
    }

    unsafe fn replay_v8_facts(api: &IdApi, staged: &StagedV8Facts) -> Option<usize> {
        if staged.facts.is_empty() && staged.store == api.nilObj {
            return Some(0);
        }
        if unsafe { send0(api, staged.store, b"imageLoadBegin\0") } == api.nilObj {
            return None;
        }
        for arguments in &staged.facts {
            if unsafe {
                (api.send)(
                    staged.store,
                    intern(api, b"imageAddFact:attr:value:tx:op:historied:\0"),
                    arguments.as_ptr(),
                    arguments.len(),
                )
            } == api.nilObj
            {
                return None;
            }
        }
        let arguments = [
            staged.basis,
            api.falseObj,
            staged.oldest,
            staged.policy,
            staged.argument,
        ];
        (unsafe {
            (api.send)(
                staged.store,
                intern(api, b"imageLoadEnd:raw:oldest:policy:argument:\0"),
                arguments.as_ptr(),
                arguments.len(),
            )
        } != api.nilObj)
            .then_some(staged.facts.len())
    }

    unsafe fn materialize_binding_field(
        api: &IdApi,
        field: birnam_artifact::code_binding_inventory::Field<'_>,
        relocations: &[Oop],
    ) -> Option<Oop> {
        use birnam_artifact::code_binding_inventory::Field;
        match field {
            Field::Nil => Some(api.nilObj),
            Field::True => Some(api.trueObj),
            Field::False => Some(api.falseObj),
            Field::Symbol(bytes) => {
                let mut name = bytes.to_vec();
                name.push(0);
                Some(unsafe { (api.intern)(name.as_ptr().cast()) })
            }
            Field::HeapRef(index) => relocations.get(index as usize).copied(),
            Field::ExportRef(bytes) => unsafe { import_bytes(api, bytes) },
        }
    }

    unsafe fn prepare_v8_bindings(api: &IdApi, bytes: &[u8], relocations: &[Oop]) -> Option<usize> {
        let inventory = birnam_artifact::code_binding_inventory::inspect(bytes).ok()?;
        let codegen = unsafe { birnam_abi::namespace_import::import(execution_imports(), RUST_CODEGEN_NAME.as_ptr().cast()) };
        if codegen.is_null() || codegen == api.nilObj {
            return None;
        }
        for index in 0..inventory.binding_count as usize {
            let binding = inventory.binding_spec(index)?;
            let target = unsafe { materialize_binding_field(api, binding.target, relocations) }?;
            let implementation =
                unsafe { materialize_binding_field(api, binding.implementation, relocations) }?;
            let provenance =
                unsafe { materialize_binding_field(api, binding.provenance, relocations) }?;
            if target.is_null()
                || implementation.is_null()
                || provenance.is_null()
                || unsafe { (api.vtableOf)(target) }.is_null()
            {
                return None;
            }
            let index = unsafe { (birnam_abi::integer::make)(i64::try_from(index).ok()?) };
            let arguments = [index, implementation];
            if unsafe {
                (api.send)(
                    codegen,
                    intern(api, PREPARE_PORTABLE_IMAGE_BINDING),
                    arguments.as_ptr(),
                    arguments.len(),
                )
            } != api.trueObj
            {
                return None;
            }
        }
        Some(inventory.binding_count as usize)
    }

    unsafe fn install_v8_bindings(api: &IdApi, bytes: &[u8], relocations: &[Oop]) -> Option<usize> {
        let inventory = birnam_artifact::code_binding_inventory::inspect(bytes).ok()?;
        let codegen = unsafe { birnam_abi::namespace_import::import(execution_imports(), RUST_CODEGEN_NAME.as_ptr().cast()) };
        if codegen.is_null() || codegen == api.nilObj {
            return None;
        }
        for index in 0..inventory.binding_count as usize {
            let binding = inventory.binding_spec(index)?;
            let target = unsafe { materialize_binding_field(api, binding.target, relocations) }?;
            let implementation =
                unsafe { materialize_binding_field(api, binding.implementation, relocations) }?;
            let provenance =
                unsafe { materialize_binding_field(api, binding.provenance, relocations) }?;
            let index_oop = unsafe { (birnam_abi::integer::make)(i64::try_from(index).ok()?) };
            let arguments = [index_oop, target, implementation, provenance];
            if unsafe {
                (api.send)(
                    codegen,
                    intern(api, INSTALL_PORTABLE_IMAGE_BINDING),
                    arguments.as_ptr(),
                    arguments.len(),
                )
            } == api.nilObj
            {
                return None;
            }
        }
        Some(inventory.binding_count as usize)
    }

    unsafe fn begin_v8_native_unit(
        api: &IdApi,
        decoded: &v8::DecodedImage<'_>,
        rebuild: bool,
    ) -> Option<(Oop, bool)> {
        let codegen = unsafe { birnam_abi::namespace_import::import(execution_imports(), RUST_CODEGEN_NAME.as_ptr().cast()) };
        if codegen.is_null() || codegen == api.nilObj {
            return None;
        }
        let programs = unsafe { (birnam_abi::string::make)(decoded.programs.as_ptr(), decoded.programs.len()) };
        let bindings = unsafe { (birnam_abi::string::make)(decoded.bindings.as_ptr(), decoded.bindings.len()) };
        let selection =
            unsafe { (birnam_abi::string::make)(decoded.selection.as_ptr(), decoded.selection.len()) };
        if !rebuild {
            if let Some(code) = decoded.code {
                let unit = code.get(72..)?;
                let unit = unsafe { (birnam_abi::string::make)(unit.as_ptr(), unit.len()) };
                let arguments = [unit, programs, bindings, selection];
                let owner = unsafe {
                    (api.send)(
                        codegen,
                        intern(api, BEGIN_PORTABLE_IMAGE_MAPPED),
                        arguments.as_ptr(),
                        arguments.len(),
                    )
                };
                return (owner != api.nilObj && !owner.is_null()).then_some((owner, true));
            }
        }
        let arguments = [programs, bindings, selection];
        let owner = unsafe {
            (api.send)(
                codegen,
                intern(api, BEGIN_PORTABLE_IMAGE_REBUILD),
                arguments.as_ptr(),
                arguments.len(),
            )
        };
        (owner != api.nilObj && !owner.is_null()).then_some((owner, false))
    }

    unsafe fn load_v8(api: &IdApi, bytes: &[u8], rebuild: bool) -> Oop {
        let Ok(decoded) = v8::decode(bytes, IMAGE_ARCH_TAG, runtime_abi_hash()) else {
            return api.nilObj;
        };
        unsafe { (api.gcSetAuto)(0) };
        struct AutoGc<'a>(&'a IdApi);
        impl Drop for AutoGc<'_> {
            fn drop(&mut self) {
                unsafe { (self.0.gcSetAuto)(1) };
            }
        }
        let _auto_gc = AutoGc(api);
        let Some((owner, mapped)) = (unsafe { begin_v8_native_unit(api, &decoded, rebuild) })
        else {
            return api.nilObj;
        };
        struct StageGuard<'a> {
            api: &'a IdApi,
            imports: *const birnam_abi::namespace_import::ApiV1,
            armed: bool,
        }
        impl Drop for StageGuard<'_> {
            fn drop(&mut self) {
                if !self.armed {
                    return;
                }
                let codegen = unsafe { birnam_abi::namespace_import::import(self.imports, RUST_CODEGEN_NAME.as_ptr().cast()) };
                if !codegen.is_null() && codegen != self.api.nilObj {
                    let _ = unsafe { send0(self.api, codegen, CANCEL_PORTABLE_IMAGE_UNIT) };
                }
            }
        }
        let mut stage = StageGuard { api, imports: execution_imports(), armed: true };
        let Some(relocations) = (unsafe { materialize_v8_heaps(api, &decoded, owner) }) else {
            return api.nilObj;
        };
        let Some(staged_facts) = (unsafe { prepare_v8_facts(api, &decoded.facts, &relocations) })
        else {
            return api.nilObj;
        };
        if unsafe { prepare_v8_bindings(api, decoded.bindings, &relocations) }.is_none() {
            return api.nilObj;
        }
        let codegen = unsafe { birnam_abi::namespace_import::import(execution_imports(), RUST_CODEGEN_NAME.as_ptr().cast()) };
        if codegen.is_null()
            || codegen == api.nilObj
            || unsafe { send0(api, codegen, FINISH_PORTABLE_IMAGE_UNIT) } != api.trueObj
        {
            return api.nilObj;
        }
        stage.armed = false;
        if unsafe { replay_v8_facts(api, &staged_facts) }.is_none() {
            return api.nilObj;
        }
        if unsafe { install_v8_bindings(api, decoded.bindings, &relocations) }.is_none() {
            return api.nilObj;
        }
        if mapped { api.trueObj } else { api.falseObj }
    }

    unsafe fn runtime_image_path_bytes(api: &IdApi, path: Oop) -> Option<Vec<u8>> {
        if path.is_null() || unsafe { (api.vtableOf)(path) } != unsafe { birnam_abi::string::behavior() } {
            return None;
        }
        let mut length = 0;
        let bytes = unsafe { (birnam_abi::string::bytes)(path, &mut length) };
        if bytes.is_null() && length != 0 {
            return None;
        }
        let bytes = if length == 0 {
            &[][..]
        } else {
            // SAFETY: stringBytes supplies `length` readable bytes for this live String.
            unsafe { core::slice::from_raw_parts(bytes, length) }
        };
        Some(bytes.to_vec())
    }

    fn preflight_runtime_image_bytes(bytes: &[u8]) -> bool {
        u64::try_from(bytes.len()).unwrap_or(u64::MAX) <= MAX_IMAGE_BYTES
            && bytes.get(8..12) == Some(&v8::FORMAT_VERSION.to_le_bytes())
            && v8::decode(bytes, IMAGE_ARCH_TAG, runtime_abi_hash()).is_ok()
    }

    unsafe fn preflight_runtime_image(api: &IdApi, path: Oop) -> bool {
        let Some(path) = (unsafe { runtime_image_path_bytes(api, path) }) else {
            return false;
        };
        let image_path = Path::new(OsStr::from_bytes(&path));
        let Ok(metadata) = fs::metadata(image_path) else {
            return false;
        };
        if !metadata.is_file() || metadata.len() > MAX_IMAGE_BYTES {
            return false;
        }
        let Ok(bytes) = fs::read(image_path) else {
            return false;
        };
        preflight_runtime_image_bytes(&bytes)
    }

    unsafe fn direct_method_present(api: &IdApi, target: Oop, selector: &[u8]) -> bool {
        let vtable = unsafe { (api.vtableOf)(target) };
        if vtable.is_null() {
            return false;
        }
        let selector = unsafe { intern(api, selector) };
        let mut index = 0usize;
        loop {
            let entry = unsafe { birnam_abi::vtable_enumeration::selector_at(runtime().vtables as *const _, vtable, index) };
            if entry.is_null() {
                return false;
            }
            if entry == selector {
                return true;
            }
            index = index.saturating_add(1);
        }
    }

    unsafe fn load_with_rust_codegen_bytes(api: &IdApi, image_bytes: &[u8], rebuild: bool) -> Oop {
        if let Ok(mut identity) = IMAGE_COMPOSITION_IDENTITY.lock() {
            *identity = None;
        }
        if !preflight_runtime_image_bytes(image_bytes) {
            return api.nilObj;
        }
        let Ok(decoded) = v8::decode(image_bytes, IMAGE_ARCH_TAG, runtime_abi_hash()) else {
            return api.nilObj;
        };
        let composition = decoded
            .code
            .and_then(|code| code.get(0..8))
            .and_then(|bytes| bytes.try_into().ok())
            .map(u64::from_le_bytes)
            .and_then(|checksum| composition_identity(&[checksum]));
        let codegen = unsafe { birnam_abi::namespace_import::import(execution_imports(), RUST_CODEGEN_NAME.as_ptr().cast()) };
        if codegen.is_null()
            || codegen == api.nilObj
            || unsafe { send0(api, codegen, RUST_CODEGEN_BEGIN_IMAGE_RECOVERY) } != api.trueObj
        {
            return api.nilObj;
        }
        let loaded = unsafe { load_v8(api, image_bytes, rebuild) };
        let ended = unsafe { send0(api, codegen, RUST_CODEGEN_END_IMAGE_RECOVERY) };
        if (loaded != api.trueObj && loaded != api.falseObj) || ended != api.trueObj {
            return api.nilObj;
        }
        if let Ok(mut identity) = IMAGE_COMPOSITION_IDENTITY.lock() {
            *identity = composition;
        }
        loaded
    }

    unsafe fn load_with_rust_codegen(api: &IdApi, path: Oop, rebuild: bool) -> Oop {
        let Some(path_bytes) = (unsafe { runtime_image_path_bytes(api, path) }) else {
            return api.nilObj;
        };
        let Ok(image_bytes) = fs::read(Path::new(OsStr::from_bytes(&path_bytes))) else {
            return api.nilObj;
        };
        unsafe { load_with_rust_codegen_bytes(api, &image_bytes, rebuild) }
    }

    unsafe fn collect_store_facts(
        api: &IdApi,
        authority: Option<[u8; AUTHORITY_DIGEST_LEN]>,
    ) -> FactPreflight {
        if let Ok(mut records) = PORTABLE_SAVE_RECORDS.lock() {
            *records = None;
        }
        let store = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"Store".as_ptr()) };
        if store.is_null() || store == api.nilObj {
            return FactPreflight {
                kept: 0,
                recovered: 0,
                rejected: 0,
                facts_payload: Vec::new(),
                heap_payload: Some(Vec::new()),
                heap_ok: true,
                recovered_slots: 0,
                rejected_slots: 0,
                writer_ok: false,
                image_rejection: None,
            };
        }
        if unsafe { send0(api, store, b"compact\0") } != store {
            return FactPreflight {
                kept: 0,
                recovered: 0,
                rejected: 0,
                facts_payload: Vec::new(),
                heap_payload: None,
                heap_ok: false,
                recovered_slots: 0,
                rejected_slots: 0,
                writer_ok: false,
                image_rejection: None,
            };
        }
        let portable_bindings = unsafe { portable_binding_snapshot(api) };
        let collector_vt = unsafe { birnam_abi::namespace_import::import(execution_imports(), COLLECTOR_VT_NAME.as_ptr().cast()) };
        if collector_vt.is_null() {
            return FactPreflight {
                kept: 0,
                recovered: 0,
                rejected: 0,
                facts_payload: Vec::new(),
                heap_payload: None,
                heap_ok: false,
                recovered_slots: 0,
                rejected_slots: 0,
                writer_ok: false,
                image_rejection: None,
            };
        }
        let basis_object = unsafe { send0(api, store, b"basis\0") };
        let basis = if unsafe { (birnam_abi::integer::is_integer)(basis_object) } != 0 {
            unsafe { (birnam_abi::integer::value)(basis_object) }.max(0) as u64
        } else {
            0
        };
        let oldest_object = unsafe { send0(api, store, b"oldestRetainedBasis\0") };
        let policy_object = unsafe { send0(api, store, b"retentionPolicyCode\0") };
        let argument_object = unsafe { send0(api, store, b"retentionPolicyArgument\0") };
        let metadata = if [oldest_object, policy_object, argument_object]
            .iter()
            .all(|value| unsafe { (birnam_abi::integer::is_integer)(*value) } != 0)
        {
            let oldest = unsafe { (birnam_abi::integer::value)(oldest_object) };
            let policy = unsafe { (birnam_abi::integer::value)(policy_object) };
            let argument = unsafe { (birnam_abi::integer::value)(argument_object) };
            let retention = match (policy, argument) {
                (0, 0) => Some(retained_facts::RetentionPolicy::CurrentOnly),
                (1, value) if value >= 0 => {
                    Some(retained_facts::RetentionPolicy::KeepSince(value as u64))
                }
                (2, value) if value >= 0 => {
                    Some(retained_facts::RetentionPolicy::KeepLast(value as u64))
                }
                _ => None,
            };
            (oldest >= 0)
                .then_some(oldest as u64)
                .zip(retention)
                .and_then(|(oldest, retention)| {
                    retained_facts::FactsWriter::new(basis, oldest, retention).ok()
                })
        } else {
            None
        };
        let Some(facts_writer) = metadata else {
            return FactPreflight {
                kept: 0,
                recovered: 0,
                rejected: 0,
                facts_payload: Vec::new(),
                heap_payload: None,
                heap_ok: false,
                recovered_slots: 0,
                rejected_slots: 0,
                writer_ok: false,
                image_rejection: None,
            };
        };
        let collector = unsafe { (api.alloc)(collector_vt, 0) };
        if collector.is_null() {
            return FactPreflight {
                kept: 0,
                recovered: 0,
                rejected: 0,
                facts_payload: Vec::new(),
                heap_payload: None,
                heap_ok: false,
                recovered_slots: 0,
                rejected_slots: 0,
                writer_ok: false,
                image_rejection: None,
            };
        }
        {
            let mut collection = FACT_COLLECTION
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *collection = Some(FactCollection {
                writer: facts_writer,
                heap: HeapPreflight::new(authority),
                kept: 0,
                recovered: 0,
                recoveries_needing_code: 0,
                rejected: 0,
            });
        }
        let arguments = [collector];
        let _ = unsafe {
            (api.send)(
                store,
                intern(api, b"imageFactsInto:\0"),
                arguments.as_ptr(),
                arguments.len(),
            )
        };
        let collection = FACT_COLLECTION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        let Some(mut collection) = collection else {
            return FactPreflight {
                kept: 0,
                recovered: 0,
                rejected: 0,
                facts_payload: Vec::new(),
                heap_payload: None,
                heap_ok: false,
                recovered_slots: 0,
                rejected_slots: 0,
                writer_ok: false,
                image_rejection: None,
            };
        };
        let facts_payload = collection.writer.finish();
        if let Some(bindings) = portable_bindings.as_deref() {
            for object in bindings
                .iter()
                .flat_map(|binding| [binding.target, binding.implementation, binding.provenance])
            {
                if unsafe { runtime_oop_is_canonical_reference(api, object) }
                    || unsafe { c_string_bytes((birnam_abi::namespace_names::name_of)(object)) }.is_some()
                {
                    continue;
                }
                if unsafe { collection.heap.intern(api, object) }.is_none() {
                    collection.heap.rejected_slots =
                        collection.heap.rejected_slots.saturating_add(1);
                }
            }
        }
        let portable_heap = unsafe { collection.heap.finish_v8(api) };
        let heap_payload = portable_heap.as_ref().map(|(heap, _)| heap.clone());
        let portable_records = portable_heap.and_then(|(_, programs)| {
            let (bindings, selection) = unsafe {
                portable_binding_records(
                    api,
                    portable_bindings.as_deref()?,
                    &collection.heap,
                    &programs,
                )
            }?;
            Some(PortableSaveRecords {
                programs,
                bindings,
                selection,
            })
        });
        if let Ok(mut records) = PORTABLE_SAVE_RECORDS.lock() {
            *records = portable_records;
        }
        let heap_ok = heap_payload.is_some();
        let writer_ok = heap_payload
            .as_deref()
            .zip(PORTABLE_SAVE_RECORDS.lock().ok())
            .and_then(|(heap, records)| {
                let records = records.as_ref()?;
                v8::compose(v8::ImageSpec {
                    architecture: IMAGE_ARCH_TAG,
                    runtime_abi: runtime_abi_hash(),
                    programs: &records.programs,
                    bindings: &records.bindings,
                    selection: &records.selection,
                    heaps: &[heap],
                    facts: &[&facts_payload],
                    code: None,
                })
                .ok()
            })
            .is_some();
        FactPreflight {
            kept: collection.kept,
            recovered: collection.recovered,
            rejected: collection.rejected,
            facts_payload,
            heap_payload,
            heap_ok,
            recovered_slots: collection.heap.recovered_slots,
            rejected_slots: collection.heap.rejected_slots,
            writer_ok,
            image_rejection: collection.heap.image_rejection,
        }
    }

    unsafe extern "C" fn system_save_image(
        self_object: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        let api = unsafe { api_ref() };
        if count != 1 {
            reject_save("saveImage: expects one String path");
            return api.nilObj;
        }
        let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
            reject_save("saveImage: received an invalid argument buffer");
            return api.nilObj;
        };
        let path = arguments[0];
        if path.is_null() || unsafe { (api.vtableOf)(path) } != unsafe { birnam_abi::string::behavior() } {
            reject_save("saveImage: expects one String path");
            return api.nilObj;
        }
        let authority_digest = unsafe { current_generation_digest(api) };
        let preflight = unsafe { collect_store_facts(api, authority_digest) };
        if let Some(kind) = preflight.image_rejection {
            reject_save(kind.detail());
            record_fact_counts(
                preflight.kept,
                preflight.recovered,
                preflight.rejected,
                preflight.recovered_slots,
                preflight.rejected_slots,
            );
            runtime()
                .report
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .code_units = 0;
            return unsafe { image_rejection_error(api, kind) };
        }
        // New saves make the accepted clean v8 cut. Code is an authority-bound
        // mapped acceleration; canonical inventories remain sufficient for a
        // forced source-free rebuild.
        let portable_records = PORTABLE_SAVE_RECORDS
            .lock()
            .ok()
            .and_then(|mut records| records.take());
        let portable_code = portable_records
            .as_ref()
            .and_then(|records| unsafe { compile_portable_image_code(api, records) });
        let composed = preflight
            .heap_payload
            .as_deref()
            .zip(portable_records.as_ref())
            .zip(portable_code.as_deref())
            .and_then(|((heap, records), code)| {
                (preflight.rejected == 0
                    && preflight.image_rejection.is_none()
                    && preflight.heap_ok
                    && preflight.writer_ok)
                    .then(|| {
                        v8::compose(v8::ImageSpec {
                            architecture: IMAGE_ARCH_TAG,
                            runtime_abi: runtime_abi_hash(),
                            programs: &records.programs,
                            bindings: &records.bindings,
                            selection: &records.selection,
                            heaps: &[heap],
                            facts: &[&preflight.facts_payload],
                            code: Some(code),
                        })
                        .ok()
                    })
                    .flatten()
            });
        let publication_requested = rust_image_publication_selected();
        let canonical_publication = rust_image_canonical_selected() && !publication_requested;
        let publication_authorized = if publication_requested {
            rust_image_writer_authority_selected()
        } else {
            canonical_publication
        };
        let published = if publication_authorized {
            composed.as_ref().is_some_and(|bytes| {
                let path_bytes = unsafe { runtime_image_path_bytes(api, path) };
                path_bytes.is_some_and(|path_bytes| {
                    publish_atomic(Path::new(OsStr::from_bytes(&path_bytes)), bytes).is_ok()
                })
            })
        } else {
            false
        };
        if published {
            record_fact_counts(
                preflight.kept,
                preflight.recovered,
                preflight.rejected,
                preflight.recovered_slots,
                preflight.rejected_slots,
            );
            let mut report = runtime()
                .report
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            report.status = ReportStatus::Saved;
            report.detail_len = 0;
            report.code_units = 1;
            return self_object;
        }
        reject_save(if let Some(kind) = preflight.image_rejection {
            kind.detail()
        } else if !preflight.heap_ok {
            "Rust Image heap preflight rejected unsupported layout"
        } else if preflight.rejected != 0 {
            "Rust Image fact preflight rejected unsupported values"
        } else if !preflight.writer_ok {
            "Rust Image writer rejected preflight payload"
        } else if publication_requested && !publication_authorized {
            "Rust Image publication requires explicit Rust writer authority"
        } else if portable_records.is_none() {
            "Rust Image portable inventories are unavailable"
        } else if portable_code.is_none() {
            "Rust Image portable native Code compilation failed"
        } else if composed.is_none() {
            "Rust Image v8 composition failed"
        } else if publication_requested {
            "Rust Image atomic publication failed"
        } else {
            "Rust Image code mapping/rebuild orchestration is not installed"
        });
        record_fact_counts(
            preflight.kept,
            preflight.recovered,
            preflight.rejected,
            preflight.recovered_slots,
            preflight.rejected_slots,
        );
        {
            let mut report = runtime()
                .report
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            report.code_units = 0;
        }
        if let Some(kind) = preflight.image_rejection {
            unsafe { image_rejection_error(api, kind) }
        } else {
            api.nilObj
        }
    }

    unsafe extern "C" fn system_load_image(
        _self_object: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        let api = unsafe { api_ref() };
        let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
            return api.nilObj;
        };
        if count != 1 || !unsafe { preflight_runtime_image(api, arguments[0]) } {
            return api.nilObj;
        }
        unsafe { load_with_rust_codegen(api, arguments[0], false) }
    }

    unsafe extern "C" fn system_load_image_rebuild(
        _self_object: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        let api = unsafe { api_ref() };
        let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
            return api.nilObj;
        };
        if count != 2 || !unsafe { preflight_runtime_image(api, arguments[0]) } {
            return api.nilObj;
        }
        unsafe { load_with_rust_codegen(api, arguments[0], arguments[1] == api.trueObj) }
    }

    unsafe extern "C" fn system_load_image_bytes_rebuild(
        _self_object: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        let api = unsafe { api_ref() };
        let Some(arguments) = (unsafe { borrowed_oop_args(args, count) }) else {
            return api.nilObj;
        };
        if count != 2 {
            return api.nilObj;
        }
        let Some(image_bytes) = (unsafe { runtime_image_path_bytes(api, arguments[0]) }) else {
            return api.nilObj;
        };
        if !preflight_runtime_image_bytes(&image_bytes) {
            return api.nilObj;
        }
        unsafe { load_with_rust_codegen_bytes(api, &image_bytes, arguments[1] == api.trueObj) }
    }

    unsafe extern "C" fn system_image_save_report(
        _self_object: Oop,
        _args: *const Oop,
        count: usize,
    ) -> Oop {
        if count != 0 {
            return unsafe { api_ref() }.nilObj;
        }
        runtime().report_obj as Oop
    }

    unsafe extern "C" fn system_image_composition_identity(
        _self_object: Oop,
        _args: *const Oop,
        count: usize,
    ) -> Oop {
        let api = unsafe { api_ref() };
        if count != 0 {
            return api.nilObj;
        }
        let identity = IMAGE_COMPOSITION_IDENTITY
            .lock()
            .ok()
            .and_then(|identity| *identity);
        let Some(identity) = identity else {
            return api.nilObj;
        };
        unsafe { (birnam_abi::string::make)(identity.as_ptr(), identity.len()) }
    }

    unsafe extern "C" fn system_bake_exe(_self_object: Oop, args: *const Oop, count: usize) -> Oop {
        let api = unsafe { api_ref() };
        if count != 1 || unsafe { borrowed_oop_args(args, count) }.is_none() {
            return api.nilObj;
        }
        api.nilObj
    }

    unsafe extern "C" fn report_status(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
        let api = unsafe { api_ref() };
        if count != 0 {
            return api.nilObj;
        }
        let status = runtime()
            .report
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .status;
        unsafe {
            intern(
                api,
                match status {
                    ReportStatus::Never => b"never\0",
                    ReportStatus::Saved => b"saved\0",
                    ReportStatus::Rejected => b"rejected\0",
                    ReportStatus::IoFailed => b"ioFailed\0",
                },
            )
        }
    }

    fn report_value(field: fn(&SaveReport) -> u32) -> u32 {
        let report = runtime()
            .report
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        field(&report)
    }

    unsafe fn report_count_value(
        _self_object: Oop,
        _args: *const Oop,
        count: usize,
        field: fn(&SaveReport) -> u32,
    ) -> Oop {
        let api = unsafe { api_ref() };
        if count != 0 {
            return api.nilObj;
        }
        unsafe { (birnam_abi::integer::make)(i64::from(report_value(field))) }
    }

    unsafe extern "C" fn report_saved_facts(
        self_object: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        unsafe { report_count_value(self_object, args, count, |report| report.saved_facts) }
    }

    unsafe extern "C" fn report_recovered_facts(
        self_object: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        unsafe { report_count_value(self_object, args, count, |report| report.recovered_facts) }
    }

    unsafe extern "C" fn report_rejected_facts(
        self_object: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        unsafe { report_count_value(self_object, args, count, |report| report.rejected_facts) }
    }

    unsafe extern "C" fn report_recovered_slots(
        self_object: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        unsafe { report_count_value(self_object, args, count, |report| report.recovered_slots) }
    }

    unsafe extern "C" fn report_rejected_slots(
        self_object: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        unsafe { report_count_value(self_object, args, count, |report| report.rejected_slots) }
    }

    unsafe extern "C" fn report_code_units(
        self_object: Oop,
        args: *const Oop,
        count: usize,
    ) -> Oop {
        unsafe { report_count_value(self_object, args, count, |report| report.code_units) }
    }

    unsafe extern "C" fn report_detail(_self_object: Oop, _args: *const Oop, count: usize) -> Oop {
        let api = unsafe { api_ref() };
        if count != 0 {
            return api.nilObj;
        }
        let (detail, detail_len) = {
            let report = runtime()
                .report
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            (report.detail, report.detail_len)
        };
        unsafe { (birnam_abi::string::make)(detail.as_ptr(), detail_len) }
    }

    unsafe extern "C" fn collector_collect(
        _self_object: Oop,
        _args: *const Oop,
        count: usize,
    ) -> Oop {
        let api = unsafe { api_ref() };
        if count != 6 {
            if let Some(collection) = FACT_COLLECTION
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .as_mut()
            {
                collection.rejected = collection.rejected.saturating_add(1);
            }
            return api.nilObj;
        }
        let Some(arguments) = (unsafe { borrowed_oop_args(_args, count) }) else {
            if let Some(collection) = FACT_COLLECTION
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .as_mut()
            {
                collection.rejected = collection.rejected.saturating_add(1);
            }
            return api.nilObj;
        };
        let tx_object = arguments[3];
        let op_object = arguments[4];
        let historied_object = arguments[5];
        if unsafe { (birnam_abi::integer::is_integer)(tx_object) } == 0
            || unsafe { (birnam_abi::integer::is_integer)(op_object) } == 0
            || (historied_object != api.trueObj && historied_object != api.falseObj)
        {
            if let Some(collection) = FACT_COLLECTION
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .as_mut()
            {
                collection.rejected = collection.rejected.saturating_add(1);
            }
            return api.nilObj;
        }
        let tx = unsafe { (birnam_abi::integer::value)(tx_object) };
        let op = unsafe { (birnam_abi::integer::value)(op_object) };
        if tx < 0 || !(0..=1).contains(&op) {
            if let Some(collection) = FACT_COLLECTION
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .as_mut()
            {
                collection.rejected = collection.rejected.saturating_add(1);
            }
            return api.nilObj;
        }
        let mut collection = FACT_COLLECTION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(collection) = collection.as_mut() else {
            return api.nilObj;
        };
        // A selected projected global is reconstructed by the generation's
        // mapped or freshly rebuilt image unit. Serializing the closure object
        // as a Store value would require its artifact-local vtable before code
        // recovery has run and would duplicate the authoritative initializer.
        let value_vtable = unsafe { (api.vtableOf)(arguments[2]) };
        let projected_global = unsafe { symbol_bytes(api, arguments[0]) }.as_deref()
            == Some(b"__globals__".as_slice())
            && unsafe {
                collection
                    .heap
                    .block_vtable_image_reference(api, value_vtable)
                    .is_some()
            };
        if projected_global {
            collection.recovered = collection.recovered.saturating_add(1);
            return api.nilObj;
        }
        let mut fields = FieldEncoder::new();
        let encoded = unsafe {
            encode_runtime_oop(api, &mut fields, arguments[0], &mut collection.heap)
                && encode_runtime_oop(api, &mut fields, arguments[1], &mut collection.heap)
                && encode_runtime_oop(api, &mut fields, arguments[2], &mut collection.heap)
        };
        let fact_written = encoded
            && collection
                .writer
                .fact_encoded(
                    historied_object == api.trueObj,
                    op as u8,
                    tx as u64,
                    fields.as_bytes(),
                )
                .is_ok();
        if !fact_written {
            if collection.heap.image_rejection.is_some() || encoded {
                collection.rejected = collection.rejected.saturating_add(1);
            } else {
                let store = unsafe { birnam_abi::namespace_import::import(execution_imports(), c"Store".as_ptr()) };
                match if store.is_null() || store == api.nilObj {
                    DeferredFactClass::Reject
                } else {
                    unsafe { deferred_fact_class(api, store, arguments[0], arguments[1]) }
                } {
                    DeferredFactClass::Source => {
                        collection.recovered = collection.recovered.saturating_add(1);
                    }
                    DeferredFactClass::Code => {
                        collection.recovered = collection.recovered.saturating_add(1);
                        collection.recoveries_needing_code =
                            collection.recoveries_needing_code.saturating_add(1);
                    }
                    DeferredFactClass::Reject => {
                        collection.rejected = collection.rejected.saturating_add(1);
                    }
                }
            }
        } else {
            collection.kept = collection.kept.saturating_add(1);
        }
        api.nilObj
    }

    unsafe fn bring_up(api: *mut IdApi, presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1, exports: *const birnam_abi::namespace_export::ApiV1, vtables: *const birnam_abi::vtable_enumeration::ApiV2) {
        if api.is_null() {
            abort("module_init received a null IdApi");
        }
        let api_ref = unsafe { &*api };
        let report_vt =
            if unsafe { birnam_abi::namespace_presence::contains(presence, IMAGE_REPORT_VT_NAME.as_ptr().cast()) } != 0 {
                unsafe { birnam_abi::namespace_import::import(imports, IMAGE_REPORT_VT_NAME.as_ptr().cast()) }
            } else {
                let vt = unsafe { (api_ref.proto)(api_ref.object_vt) };
                unsafe { (api_ref.setLayout)(vt, ptr::null(), 0, birnam_abi::SLOT_RAW) };
                vt
            };
        let report_obj = if unsafe { birnam_abi::namespace_presence::contains(presence, IMAGE_REPORT_NAME.as_ptr().cast()) } != 0
        {
            unsafe { birnam_abi::namespace_import::import(imports, IMAGE_REPORT_NAME.as_ptr().cast()) }
        } else {
            let object = unsafe { (api_ref.alloc)(report_vt, 0) };
            unsafe {
                birnam_abi::set_vtable_owner(api_ref, report_vt, object);
                let _ = birnam_abi::namespace_export::export(exports, IMAGE_REPORT_VT_NAME.as_ptr().cast(), report_vt);
                let _ = birnam_abi::namespace_export::export(exports, IMAGE_REPORT_NAME.as_ptr().cast(), object);
            }
            object
        };
        let collector_vt = unsafe { (api_ref.proto)(api_ref.object_vt) };
        unsafe {
            (api_ref.method)(report_vt, intern(api_ref, b"status\0"), report_status);
            (api_ref.method)(
                report_vt,
                intern(api_ref, b"savedFacts\0"),
                report_saved_facts,
            );
            (api_ref.method)(
                report_vt,
                intern(api_ref, b"recoveredFacts\0"),
                report_recovered_facts,
            );
            (api_ref.method)(
                report_vt,
                intern(api_ref, b"rejectedFacts\0"),
                report_rejected_facts,
            );
            (api_ref.method)(
                report_vt,
                intern(api_ref, b"recoveredSlots\0"),
                report_recovered_slots,
            );
            (api_ref.method)(
                report_vt,
                intern(api_ref, b"rejectedSlots\0"),
                report_rejected_slots,
            );
            (api_ref.method)(
                report_vt,
                intern(api_ref, b"codeUnits\0"),
                report_code_units,
            );
            (api_ref.method)(report_vt, intern(api_ref, b"detail\0"), report_detail);
            (api_ref.method)(
                collector_vt,
                intern(api_ref, b"e:a:v:tx:op:historied:\0"),
                collector_collect,
            );
            let _ = birnam_abi::namespace_export::export(exports, COLLECTOR_VT_NAME.as_ptr().cast(), collector_vt);
            let system_vt = birnam_abi::namespace_import::import(imports, c"SystemVt".as_ptr());
            if !system_vt.is_null() {
                (api_ref.method)(
                    system_vt,
                    intern(api_ref, b"saveImage:\0"),
                    system_save_image,
                );
                (api_ref.method)(
                    system_vt,
                    intern(api_ref, b"imageSaveReport\0"),
                    system_image_save_report,
                );
                (api_ref.method)(
                    system_vt,
                    intern(api_ref, b"loadImage:\0"),
                    system_load_image,
                );
                (api_ref.method)(
                    system_vt,
                    intern(api_ref, b"loadImage:rebuild:\0"),
                    system_load_image_rebuild,
                );
                (api_ref.method)(
                    system_vt,
                    intern(api_ref, b"loadImageBytes:rebuild:\0"),
                    system_load_image_bytes_rebuild,
                );
                (api_ref.method)(
                    system_vt,
                    intern(api_ref, b"imageCompositionIdentity\0"),
                    system_image_composition_identity,
                );
                (api_ref.method)(system_vt, intern(api_ref, b"bakeExe:\0"), system_bake_exe);
            }
        }
        let _ = RUNTIME.set(Runtime {
            api: api as usize,
            vtables: vtables as usize,
            imports: imports as usize,
            report_obj: report_obj as usize,
            report: &SAVE_REPORT,
        });
        ROOT_REGISTERED.get_or_init(|| unsafe {
            (api_ref.gcRegisterRootProvider)(mark_runtime_roots);
        });
    }

    fn reject_save(detail: &str) {
        let mut report = runtime()
            .report
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *report = SaveReport {
            status: ReportStatus::Rejected,
            detail_len: detail.len().min(256),
            ..SaveReport::default()
        };
        let detail_len = report.detail_len;
        report.detail[..detail_len].copy_from_slice(&detail.as_bytes()[..detail_len]);
    }

    /// Identifies the first dynamic Rust Image orchestration shell.
    #[unsafe(no_mangle)]
    pub extern "C" fn birnam_image_rust_identity() -> *const c_char {
        RUST_IMAGE_IDENTITY.as_ptr().cast()
    }

    /// Declare that this module treats IdApi Symbol-name pointers as borrowed.
    #[unsafe(no_mangle)]
    pub extern "C" fn birnam_symbol_borrow_v1() -> core::ffi::c_int {
        1
    }

    /// Initialize the Rust Image shell against the process floor.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn module_init(api: *mut IdApi, state: *mut c_void, presence: *const birnam_abi::namespace_presence::ApiV1, imports: *const birnam_abi::namespace_import::ApiV1, exports: *const birnam_abi::namespace_export::ApiV1, vtables: *const birnam_abi::vtable_enumeration::ApiV2, _observation: *const birnam_abi::method_observation::ApiV1) -> *mut c_void {
        let _ = state;
        if RUNTIME.get().is_none() {
            unsafe { bring_up(api, presence, imports, exports, vtables) };
        }
        ptr::null_mut()
    }
}

birnam_abi::integer_dependency!();

birnam_abi::float_dependency!();

birnam_abi::word_dependency!();

birnam_abi::string_dependency!();

birnam_abi::namespace_names_dependency!();
