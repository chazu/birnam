//! No-state C-layout mirror of the current `IdApi` v16 boundary.
//!
//! Raw data pointers are intentionally nullable. Function-pointer fields are
//! non-null; the separately declared method observation hook is optional.

use core::ffi::{c_char, c_int, c_void};
use core::mem::{align_of, offset_of, size_of};

pub mod foreign_address;
pub mod integer;
pub mod float;
pub mod word;
pub mod string;
pub mod string_collection;
pub mod printing;
pub mod system_gc;
pub mod system_output;
pub mod system_input;
pub mod environment;
pub mod file_io;
pub mod block_loop;
pub mod native_loader;
pub mod source_loader;
pub mod namespace;
pub mod namespace_names;
pub mod namespace_presence;
pub mod namespace_import;
pub mod namespace_export;
pub mod namespace_enumeration;
pub mod vtable_enumeration;
pub mod method_observation;
pub mod root_registry;
pub mod gc_settings;
pub mod text_view;
pub mod error_value;
pub mod range;
pub mod vector;
pub mod map;
pub mod collection_value;
pub mod native_allocation;
pub mod storage;
pub mod trace;
pub mod trace_live;
pub mod collect;
pub mod private;

pub const SLOT_RAW: u8 = 0;
pub const SLOT_OOP: u8 = 1;
pub const SLOT_CODE: u8 = 2;
pub const SLOT_UNKNOWN: u8 = 0xff;

pub type Oop = *mut c_void;

/// Private versioned capability published by the Portable Code native rung.
///
/// This is not an IdApi field and does not add an IdApi table entry.
/// Codegen imports the capability object by name, validates this exact raw
/// payload, and binds thin shims into generated units.
pub type PortableCodeAdmitProgramV1 = unsafe extern "C" fn(bytes: *const u8, length: usize) -> Oop;
pub type PortableCodeNewBlockV1 = unsafe extern "C" fn(
    block_vtable: Oop,
    code_object: Oop,
    captures: *const Oop,
    capture_count: usize,
) -> Oop;

#[derive(Clone, Copy)]
#[repr(C)]
pub struct PortableCodeApiV1 {
    pub abi_version: u64,
    pub struct_size: u64,
    pub admit_program: PortableCodeAdmitProgramV1,
    pub new_block: PortableCodeNewBlockV1,
}

pub const PORTABLE_CODE_API_V1_VERSION: u64 = 1;

/// K3 construction seam for a Block whose private vtable must retain both its
/// portable meaning and one process-local native materialization owner.
pub type PortableCodeNewMaterializedBlockV2 = unsafe extern "C" fn(
    block_vtable: Oop,
    code_object: Oop,
    captures: *const Oop,
    capture_count: usize,
    materialization_owner: Oop,
) -> Oop;

#[derive(Clone, Copy)]
#[repr(C)]
pub struct PortableCodeApiV2 {
    pub abi_version: u64,
    pub struct_size: u64,
    pub new_materialized_block: PortableCodeNewMaterializedBlockV2,
}

pub const PORTABLE_CODE_API_V2_VERSION: u64 = 2;

/// Private, allocation-free CodeObject-to-canonical-bytes view used only by
/// explicit compiler/image-loader materialization safe points.
///
/// The function returns the exact nonzero byte length for a valid immutable
/// CodeObject. It writes the complete program only when `destination` is
/// non-null and `capacity` is sufficient; size queries and undersized buffers
/// perform no write. Zero rejects the receiver.
pub type PortableProgramCopyV1 =
    unsafe extern "C" fn(code_object: Oop, destination: *mut u8, capacity: usize) -> usize;

#[derive(Clone, Copy)]
#[repr(C)]
pub struct PortableProgramViewApiV1 {
    pub abi_version: u64,
    pub struct_size: u64,
    pub copy_program: PortableProgramCopyV1,
}

pub const PORTABLE_PROGRAM_VIEW_API_V1_VERSION: u64 = 1;

/// K5 image-loader construction seam. Durable provenance remains an ordinary
/// Oop distinct from canonical PortableProgram meaning.
pub type PortableCodeAdmitProgramWithProvenanceV3 =
    unsafe extern "C" fn(bytes: *const u8, length: usize, provenance: Oop) -> Oop;

#[derive(Clone, Copy)]
#[repr(C)]
pub struct PortableCodeApiV3 {
    pub abi_version: u64,
    pub struct_size: u64,
    pub admit_program_with_provenance: PortableCodeAdmitProgramWithProvenanceV3,
}

pub const PORTABLE_CODE_API_V3_VERSION: u64 = 3;
pub type Method = unsafe extern "C" fn(Oop, *const Oop, usize) -> Oop;
pub type MarkFn = unsafe extern "C" fn(Oop);
pub type RootProvider = unsafe extern "C" fn(MarkFn);
pub type TraceHook = unsafe extern "C" fn(Oop, MarkFn);
pub type Finalizer = unsafe extern "C" fn(Oop);
pub type IdInitFn = unsafe extern "C" fn() -> *mut IdApi;
/// Native initialization receives presence, import, export, vtable enumeration,
/// method observation, root registration and GC settings capabilities associated
/// with `api`. All live selections and their code/context must outlive any retained module
/// execution state; callers do not transfer ownership.
pub type ModuleInitFn = unsafe extern "C" fn(*mut IdApi, *mut c_void, *const namespace_presence::ApiV1, *const namespace_import::ApiV1, *const namespace_export::ApiV1, *const vtable_enumeration::ApiV2, *const method_observation::ApiV1, *const root_registry::ApiV1, *const gc_settings::ApiV1) -> *mut c_void;

/// Borrow a foreign read-only buffer while enforcing the ABI's null/count and
/// alignment rules.
///
/// A zero count always returns an empty slice without dereferencing `pointer`;
/// null with a nonzero count and a misaligned nonempty pointer return `None`.
///
/// # Safety
///
/// For a nonzero count, `pointer` must address `count` initialized `T` values
/// that remain readable for `'a`. This helper validates carrier shape, not
/// foreign allocation provenance or lifetime.
pub unsafe fn borrowed_slice<'a, T>(pointer: *const T, count: usize) -> Option<&'a [T]> {
    if count == 0 {
        return Some(&[]);
    }
    if pointer.is_null() || !(pointer as usize).is_multiple_of(align_of::<T>()) {
        return None;
    }
    // SAFETY: the caller supplies the readable extent/lifetime; the checks
    // above establish the non-null and alignment preconditions.
    Some(unsafe { core::slice::from_raw_parts(pointer, count) })
}

/// Borrow an IdApi Method/send argument buffer under the frozen zero-count/null
/// exception.
///
/// # Safety
///
/// For a nonzero count, `pointer` must address `count` initialized `Oop`
/// carriers that remain readable for `'a`.
pub unsafe fn borrowed_oop_args<'a>(pointer: *const Oop, count: usize) -> Option<&'a [Oop]> {
    // SAFETY: forwarded caller contract.
    unsafe { borrowed_slice(pointer, count) }
}

/// Borrow bytes supplied through a length-delimited C ABI field.
///
/// # Safety
///
/// For a nonzero count, `pointer` must address `count` readable bytes for `'a`.
pub unsafe fn borrowed_bytes<'a>(pointer: *const u8, count: usize) -> Option<&'a [u8]> {
    // SAFETY: forwarded caller contract.
    unsafe { borrowed_slice(pointer, count) }
}

/// Current versioned C ABI table returned by `id_init`.
#[allow(dead_code, non_snake_case)]
#[repr(C)]
pub struct IdApi {
    pub dlopen: unsafe extern "C" fn(*const c_char, c_int) -> *mut c_void,
    pub dlsym: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,

    pub object_vt: Oop,
    pub vtable_vt: Oop,
    pub symbol_vt: Oop,

    pub intern: unsafe extern "C" fn(*const c_char) -> Oop,
    pub proto: unsafe extern "C" fn(Oop) -> Oop,
    pub method: unsafe extern "C" fn(Oop, Oop, Method),
    pub alloc: unsafe extern "C" fn(Oop, usize) -> Oop,

    pub send: unsafe extern "C" fn(Oop, Oop, *const Oop, usize) -> Oop,

    pub nilObj: Oop,
    pub trueObj: Oop,
    pub falseObj: Oop,

    pub symbolName: unsafe extern "C" fn(Oop) -> *const c_char,
    pub vtableOf: unsafe extern "C" fn(Oop) -> Oop,

    pub gcSetStackBottom: unsafe extern "C" fn(usize),
    pub gcCollect: unsafe extern "C" fn() -> usize,
    pub gcLiveCount: unsafe extern "C" fn() -> usize,
    pub setTraceHook: unsafe extern "C" fn(Oop, TraceHook),

    pub setFinalizer: unsafe extern "C" fn(Oop, Finalizer),

    pub setLayout: unsafe extern "C" fn(Oop, *const u8, usize, u8),

    pub objectPayloadSize: unsafe extern "C" fn(Oop) -> usize,
    pub slotKind: unsafe extern "C" fn(Oop, usize) -> u8,

}

pub const ABI_DESCRIPTOR_MAGIC: [u8; 8] = *b"BIRNABI1";
pub const ABI_TYPE_COUNT: usize = 6;
pub const ABI_FIELD_COUNT: usize = 23;

const LAYOUT_KIND_TYPE: usize = 0;
const LAYOUT_KIND_DATA_POINTER: usize = 1;
const LAYOUT_KIND_FUNCTION_POINTER: usize = 2;

/// Pointer-free layout entry used only by the diagnostic R1 ABI probe.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct AbiLayoutEntryV1 {
    pub offset: usize,
    pub size: usize,
    pub alignment: usize,
    pub nullable: usize,
    pub kind: usize,
}

/// Pointer-free diagnostic description of the Rust mirror's actual layout.
///
/// Entry order is the canonical order in `abi/idapi-v16.json`. Names and
/// signatures remain owned by that manifest; this value supplies the Rust
/// compiler's concrete sizes, alignments, offsets, nullability, and field kind.
#[repr(C)]
pub struct AbiDescriptorV1 {
    pub magic: [u8; 8],
    pub descriptor_version: usize,
    pub descriptor_size: usize,
    pub schema_version: usize,
    pub target_arch: usize,
    pub target_os: usize,
    pub target_abi: usize,
    pub object_format: usize,
    pub endian: usize,
    pub pointer_bits: usize,
    pub c_int_bits: usize,
    pub id_api_size: usize,
    pub id_api_alignment: usize,
    pub type_count: usize,
    pub field_count: usize,
    pub types: [AbiLayoutEntryV1; ABI_TYPE_COUNT],
    pub fields: [AbiLayoutEntryV1; ABI_FIELD_COUNT],
    pub constants: [usize; 4],
    pub fingerprint: [u8; 32],
}

const fn type_layout<T>(nullable: bool) -> AbiLayoutEntryV1 {
    AbiLayoutEntryV1 {
        offset: 0,
        size: size_of::<T>(),
        alignment: align_of::<T>(),
        nullable: nullable as usize,
        kind: LAYOUT_KIND_TYPE,
    }
}

macro_rules! field_layout {
    ($field:ident, $nullable:expr, $kind:expr) => {
        AbiLayoutEntryV1 {
            offset: offset_of!(IdApi, $field),
            size: 8,
            alignment: 8,
            nullable: $nullable as usize,
            kind: $kind,
        }
    };
}

pub static ABI_DESCRIPTOR_V1: AbiDescriptorV1 = AbiDescriptorV1 {
    magic: ABI_DESCRIPTOR_MAGIC,
    descriptor_version: 1,
    descriptor_size: size_of::<AbiDescriptorV1>(),
    schema_version: 16,
    target_arch: 1,   // aarch64
    target_os: 1,     // macOS
    target_abi: 0,    // none
    object_format: 1, // Mach-O
    endian: 0,        // little
    pointer_bits: usize::BITS as usize,
    c_int_bits: c_int::BITS as usize,
    id_api_size: size_of::<IdApi>(),
    id_api_alignment: align_of::<IdApi>(),
    type_count: ABI_TYPE_COUNT,
    field_count: ABI_FIELD_COUNT,
    types: [
        type_layout::<Oop>(true),
        type_layout::<Method>(false),
        type_layout::<TraceHook>(false),
        type_layout::<Finalizer>(false),
        type_layout::<IdInitFn>(false),
        type_layout::<ModuleInitFn>(false),
    ],
    fields: [
        field_layout!(dlopen, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(dlsym, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(object_vt, true, LAYOUT_KIND_DATA_POINTER),
        field_layout!(vtable_vt, true, LAYOUT_KIND_DATA_POINTER),
        field_layout!(symbol_vt, true, LAYOUT_KIND_DATA_POINTER),
        field_layout!(intern, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(proto, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(method, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(alloc, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(send, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(nilObj, true, LAYOUT_KIND_DATA_POINTER),
        field_layout!(trueObj, true, LAYOUT_KIND_DATA_POINTER),
        field_layout!(falseObj, true, LAYOUT_KIND_DATA_POINTER),
        field_layout!(symbolName, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(vtableOf, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(gcSetStackBottom, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(gcCollect, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(gcLiveCount, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(setTraceHook, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(setFinalizer, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(setLayout, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(objectPayloadSize, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(slotKind, false, LAYOUT_KIND_FUNCTION_POINTER),
    ],
    constants: [
        SLOT_RAW as usize,
        SLOT_OOP as usize,
        SLOT_CODE as usize,
        SLOT_UNKNOWN as usize,
    ],
    fingerprint: [0xf8, 0x1c, 0x1c, 0xf9, 0x5e, 0x51, 0xd5, 0x07, 0x6d, 0x0f, 0xba, 0x39, 0x6c, 0x5b, 0xe3, 0x5b, 0xd7, 0x3d, 0xaf, 0x27, 0xb9, 0xf9, 0x0d, 0x25, 0x82, 0xab, 0x70, 0x18, 0x59, 0x3c, 0xca, 0x77],
};

macro_rules! assert_field_offset {
    ($field:ident, $offset:expr) => {
        assert!(offset_of!(IdApi, $field) == $offset);
    };
}

// The frozen manifest currently supports arm64 macOS only. A port must commit
// a distinct target descriptor rather than silently accepting a new ABI.
#[cfg(not(all(
    target_arch = "aarch64",
    target_os = "macos",
    target_pointer_width = "64"
)))]
compile_error!("IdApi is declared only for 64-bit arm64 macOS");

const _: () = {
    assert!(size_of::<c_int>() == 4);
    assert!(size_of::<Oop>() == 8);
    assert!(align_of::<Oop>() == 8);
    assert!(size_of::<Method>() == 8);
    assert!(align_of::<Method>() == 8);
    assert!(size_of::<RootProvider>() == 8);
    assert!(size_of::<TraceHook>() == 8);
    assert!(size_of::<Finalizer>() == 8);
    assert!(size_of::<IdInitFn>() == 8);
    assert!(size_of::<ModuleInitFn>() == 8);
    assert!(size_of::<IdApi>() == 184);
    assert!(align_of::<IdApi>() == 8);

    assert_field_offset!(dlopen, 0);
    assert_field_offset!(dlsym, 8);
    assert_field_offset!(object_vt, 16);
    assert_field_offset!(vtable_vt, 24);
    assert_field_offset!(symbol_vt, 32);
    assert_field_offset!(intern, 40);
    assert_field_offset!(proto, 48);
    assert_field_offset!(method, 56);
    assert_field_offset!(alloc, 64);
    assert_field_offset!(send, 72);
    assert_field_offset!(nilObj, 80);
    assert_field_offset!(trueObj, 88);
    assert_field_offset!(falseObj, 96);
    assert_field_offset!(symbolName, 104);
    assert_field_offset!(vtableOf, 112);
    assert_field_offset!(gcSetStackBottom, 120);
    assert_field_offset!(gcCollect, 128);
    assert_field_offset!(gcLiveCount, 136);
    assert_field_offset!(setTraceHook, 144);
    assert_field_offset!(setFinalizer, 152);
    assert_field_offset!(setLayout, 160);
    assert_field_offset!(objectPayloadSize, 168);
    assert_field_offset!(slotKind, 176);

    assert!(SLOT_RAW == 0);
    assert!(SLOT_OOP == 1);
    assert!(SLOT_CODE == 2);
    assert!(SLOT_UNKNOWN == 255);
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_layout_matches_the_current_manifest() {
        assert_eq!((size_of::<Oop>(), align_of::<Oop>()), (8, 8));
        assert_eq!((size_of::<Method>(), align_of::<Method>()), (8, 8));
        assert_eq!((size_of::<IdApi>(), align_of::<IdApi>()), (184, 8));
        assert_eq!(offset_of!(IdApi, slotKind), 176);
        assert_eq!(
            [SLOT_RAW, SLOT_OOP, SLOT_CODE, SLOT_UNKNOWN],
            [0, 1, 2, 255]
        );
    }

    #[test]
    fn borrowed_buffers_enforce_null_count_and_alignment_shape() {
        // SAFETY: zero-count buffers are never dereferenced.
        assert_eq!(
            unsafe { borrowed_oop_args(core::ptr::null(), 0) },
            Some(&[][..])
        );
        // SAFETY: this deliberately violates only the checked null/count shape.
        assert_eq!(unsafe { borrowed_oop_args(core::ptr::null(), 1) }, None);

        let values = [core::ptr::null_mut(), 42usize as Oop];
        // SAFETY: values supplies two initialized Oop carriers for this scope.
        assert_eq!(
            unsafe { borrowed_oop_args(values.as_ptr(), values.len()) },
            Some(&values[..])
        );

        let bytes = [1u8, 2, 3];
        // SAFETY: bytes supplies three initialized bytes for this scope.
        assert_eq!(
            unsafe { borrowed_bytes(bytes.as_ptr(), bytes.len()) },
            Some(&bytes[..])
        );
    }
}

/// Invoke the loaded object-model service. This is a message adapter, not an Id
/// implementation or a replacement native table entry.
/// # Safety
/// The object-model module must be mounted on this live owner-thread floor;
/// `object` is a live object in it. The ordinary send/root contracts apply.
pub unsafe fn ensure_private_vtable(api: &IdApi, object: Oop) -> Oop {
    unsafe {
        (api.send)(
            api.vtable_vt,
            (api.intern)(c"ensurePrivateVtableFor:".as_ptr()),
            &object,
            1,
        )
    }
}

/// Query the default-vtable association through ordinary dispatch. The default
/// implementation returns null for an unregistered candidate.
/// # Safety
/// Supply the live owner-thread API and preserve operands across a send.
/// Replacements may allocate, collect or reenter; no runtime borrow or lock may
/// span the call. This adapter contains no representation access or cached Method.
pub unsafe fn vtable_owner(api: &IdApi, vtable: Oop) -> Oop {
    unsafe { vtable_protocol(api, c"ownerOf:", &[vtable]) }
}

/// Change the default-vtable owner association through ordinary dispatch.
/// # Safety
/// The same send/root contract as `vtable_owner` applies. The default setter
/// requires a registered vtable and retains the existing strong owner edge.
pub unsafe fn set_vtable_owner(api: &IdApi, vtable: Oop, owner: Oop) {
    unsafe {
        vtable_protocol(api, c"ownerOf:put:", &[vtable, owner]);
    }
}

/// Query default delegation through ordinary dispatch.
/// # Safety
/// The same send/root contract as `vtable_owner` applies.
pub unsafe fn vtable_parent(api: &IdApi, vtable: Oop) -> Oop {
    unsafe { vtable_protocol(api, c"parentOf:", &[vtable]) }
}

/// Change default delegation through ordinary dispatch.
/// # Safety
/// The same send/root contract as `vtable_owner` applies. The default setter
/// requires a registered vtable and a registered or null parent.
pub unsafe fn set_vtable_parent(api: &IdApi, vtable: Oop, parent: Oop) {
    unsafe {
        vtable_protocol(api, c"parentOf:put:", &[vtable, parent]);
    }
}

unsafe fn vtable_protocol(api: &IdApi, selector: &core::ffi::CStr, args: &[Oop]) -> Oop {
    unsafe {
        (api.send)(
            api.vtable_vt,
            (api.intern)(selector.as_ptr()),
            args.as_ptr(),
            args.len(),
        )
    }
}

#[cfg(test)]
mod v16_manifest_tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn current_descriptor_matches_independent_v16_manifest() {
        let mut manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../../abi/idapi-v16.json")).unwrap();
        let expected = manifest
            .as_object_mut()
            .unwrap()
            .remove("fingerprint")
            .unwrap();
        let digest = Sha256::digest(serde_json::to_vec(&manifest).unwrap());
        assert_eq!(format!("{digest:x}"), expected["value"].as_str().unwrap());
        assert_eq!(digest.as_slice(), ABI_DESCRIPTOR_V1.fingerprint);
        assert_eq!(manifest["schema_version"], ABI_DESCRIPTOR_V1.schema_version);
        assert_eq!(manifest["id_api"]["size"], size_of::<IdApi>());
        assert_eq!(manifest["id_api"]["field_count"], ABI_FIELD_COUNT);
        let fields = manifest["id_api"]["fields"].as_array().unwrap();
        assert_eq!(fields.len(), ABI_FIELD_COUNT);
        for (index, (field, actual)) in fields.iter().zip(&ABI_DESCRIPTOR_V1.fields).enumerate() {
            assert_eq!(field["index"], index);
            assert_eq!(field["offset"], actual.offset);
            assert_eq!(field["size"], actual.size);
            assert_eq!(field["alignment"], actual.alignment);
            for removed in [
                "gcSetAuto", "gcSetPrecise", "gcRegisterRootProvider", "exportedNameOf", "isExported", "import", "exportObj", "vtableCount", "vtableAt", "vtableSelectorAt", "setMethodOverwriteHook",
                "string_vt", "mkString", "stringBytes",
                "smallint_vt", "mkSmallInt", "smallIntValue", "isSmallInt",
                "ensurePrivateVtable",
                "setOwner",
                "vtableOwner",
                "setParent",
                "vtableParent",
            ] {
                assert_ne!(field["name"], removed);
            }
        }
    }
}
