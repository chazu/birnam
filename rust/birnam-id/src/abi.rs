//! C-layout mirror of the frozen `IdApi` v1 boundary.
//!
//! Raw data pointers are intentionally nullable. Function-pointer fields are
//! non-null except for the explicitly optional method-overwrite hook.

use core::ffi::{c_char, c_int, c_void};
use core::mem::{align_of, offset_of, size_of};

pub(crate) const SLOT_RAW: u8 = 0;
pub(crate) const SLOT_OOP: u8 = 1;
pub(crate) const SLOT_CODE: u8 = 2;
pub(crate) const SLOT_UNKNOWN: u8 = 0xff;

pub(crate) type Oop = *mut c_void;
pub(crate) type Method = unsafe extern "C" fn(Oop, *const Oop, usize) -> Oop;
pub(crate) type MarkFn = unsafe extern "C" fn(Oop);
pub(crate) type RootProvider = unsafe extern "C" fn(MarkFn);
pub(crate) type TraceHook = unsafe extern "C" fn(Oop, MarkFn);
pub(crate) type Finalizer = unsafe extern "C" fn(Oop);
pub(crate) type MethodOverwriteHook = unsafe extern "C" fn(Oop, Oop);
pub(crate) type IdInitFn = unsafe extern "C" fn() -> *mut IdApi;
pub(crate) type ModuleInitFn = unsafe extern "C" fn(*mut IdApi, *mut c_void) -> *mut c_void;

/// Append-only C ABI table returned by `id_init`.
#[allow(dead_code, non_snake_case)]
#[repr(C)]
pub(crate) struct IdApi {
    pub(crate) dlopen: unsafe extern "C" fn(*const c_char, c_int) -> *mut c_void,
    pub(crate) dlsym: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,

    pub(crate) object_vt: Oop,
    pub(crate) vtable_vt: Oop,
    pub(crate) symbol_vt: Oop,

    pub(crate) intern: unsafe extern "C" fn(*const c_char) -> Oop,
    pub(crate) proto: unsafe extern "C" fn(Oop) -> Oop,
    pub(crate) method: unsafe extern "C" fn(Oop, Oop, Method),
    pub(crate) alloc: unsafe extern "C" fn(Oop, usize) -> Oop,

    pub(crate) import: unsafe extern "C" fn(*const c_char) -> Oop,
    pub(crate) exportObj: unsafe extern "C" fn(*const c_char, Oop) -> Oop,
    pub(crate) isExported: unsafe extern "C" fn(*const c_char) -> c_int,

    pub(crate) send: unsafe extern "C" fn(Oop, Oop, *const Oop, usize) -> Oop,

    pub(crate) smallint_vt: Oop,
    pub(crate) nilObj: Oop,
    pub(crate) trueObj: Oop,
    pub(crate) falseObj: Oop,
    pub(crate) mkSmallInt: unsafe extern "C" fn(i64) -> Oop,
    pub(crate) smallIntValue: unsafe extern "C" fn(Oop) -> i64,
    pub(crate) isSmallInt: unsafe extern "C" fn(Oop) -> c_int,

    pub(crate) string_vt: Oop,
    pub(crate) mkString: unsafe extern "C" fn(*const u8, usize) -> Oop,
    pub(crate) stringBytes: unsafe extern "C" fn(Oop, *mut usize) -> *const u8,
    pub(crate) symbolName: unsafe extern "C" fn(Oop) -> *const c_char,
    pub(crate) vtableOf: unsafe extern "C" fn(Oop) -> Oop,

    pub(crate) gcSetStackBottom: unsafe extern "C" fn(usize),
    pub(crate) gcRegisterRootProvider: unsafe extern "C" fn(RootProvider),
    pub(crate) gcCollect: unsafe extern "C" fn() -> usize,
    pub(crate) gcLiveCount: unsafe extern "C" fn() -> usize,
    pub(crate) setTraceHook: unsafe extern "C" fn(Oop, TraceHook),
    pub(crate) gcSetAuto: unsafe extern "C" fn(c_int),

    pub(crate) float_vt: Oop,
    pub(crate) mkFloat: unsafe extern "C" fn(f64) -> Oop,
    pub(crate) floatValue: unsafe extern "C" fn(Oop) -> f64,
    pub(crate) isFloat: unsafe extern "C" fn(Oop) -> c_int,
    pub(crate) setFinalizer: unsafe extern "C" fn(Oop, Finalizer),

    pub(crate) setLayout: unsafe extern "C" fn(Oop, *const u8, usize, u8),
    pub(crate) gcSetPrecise: unsafe extern "C" fn(c_int),

    pub(crate) objectPayloadSize: unsafe extern "C" fn(Oop) -> usize,
    pub(crate) exportedNameOf: unsafe extern "C" fn(Oop) -> *const c_char,
    pub(crate) slotKind: unsafe extern "C" fn(Oop, usize) -> u8,
    pub(crate) ensurePrivateVtable: unsafe extern "C" fn(Oop) -> Oop,
    pub(crate) setOwner: unsafe extern "C" fn(Oop, Oop),
    pub(crate) setParent: unsafe extern "C" fn(Oop, Oop),

    pub(crate) setMethodOverwriteHook: unsafe extern "C" fn(Option<MethodOverwriteHook>),
    pub(crate) vtableCount: unsafe extern "C" fn() -> usize,
    pub(crate) vtableAt: unsafe extern "C" fn(usize) -> Oop,
    pub(crate) vtableSelectorAt: unsafe extern "C" fn(Oop, usize) -> Oop,
    pub(crate) vtableParent: unsafe extern "C" fn(Oop) -> Oop,
    pub(crate) mkWord: unsafe extern "C" fn(u64) -> Oop,
    pub(crate) wordValue: unsafe extern "C" fn(Oop) -> u64,
    pub(crate) isWord: unsafe extern "C" fn(Oop) -> c_int,
    pub(crate) word_vt: Oop,
    pub(crate) vtableOwner: unsafe extern "C" fn(Oop) -> Oop,
}

pub(crate) const ABI_DESCRIPTOR_MAGIC: [u8; 8] = *b"BIRNABI1";
pub(crate) const ABI_TYPE_COUNT: usize = 8;
pub(crate) const ABI_FIELD_COUNT: usize = 54;

const LAYOUT_KIND_TYPE: usize = 0;
const LAYOUT_KIND_DATA_POINTER: usize = 1;
const LAYOUT_KIND_FUNCTION_POINTER: usize = 2;

/// Pointer-free layout entry used only by the diagnostic R1 ABI probe.
#[derive(Clone, Copy)]
#[repr(C)]
pub(crate) struct AbiLayoutEntryV1 {
    pub(crate) offset: usize,
    pub(crate) size: usize,
    pub(crate) alignment: usize,
    pub(crate) nullable: usize,
    pub(crate) kind: usize,
}

/// Pointer-free diagnostic description of the Rust mirror's actual layout.
///
/// Entry order is the canonical order in `abi/idapi-v1.json`. Names and
/// signatures remain owned by that manifest; this value supplies the Rust
/// compiler's concrete sizes, alignments, offsets, nullability, and field kind.
#[repr(C)]
pub(crate) struct AbiDescriptorV1 {
    pub(crate) magic: [u8; 8],
    pub(crate) descriptor_version: usize,
    pub(crate) descriptor_size: usize,
    pub(crate) schema_version: usize,
    pub(crate) target_arch: usize,
    pub(crate) target_os: usize,
    pub(crate) target_abi: usize,
    pub(crate) object_format: usize,
    pub(crate) endian: usize,
    pub(crate) pointer_bits: usize,
    pub(crate) c_int_bits: usize,
    pub(crate) id_api_size: usize,
    pub(crate) id_api_alignment: usize,
    pub(crate) type_count: usize,
    pub(crate) field_count: usize,
    pub(crate) types: [AbiLayoutEntryV1; ABI_TYPE_COUNT],
    pub(crate) fields: [AbiLayoutEntryV1; ABI_FIELD_COUNT],
    pub(crate) constants: [usize; 4],
    pub(crate) fingerprint: [u8; 32],
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

pub(crate) static ABI_DESCRIPTOR_V1: AbiDescriptorV1 = AbiDescriptorV1 {
    magic: ABI_DESCRIPTOR_MAGIC,
    descriptor_version: 1,
    descriptor_size: size_of::<AbiDescriptorV1>(),
    schema_version: 1,
    target_arch: 1,   // aarch64
    target_os: 1,     // macOS
    target_abi: 0,    // none
    object_format: 1, // Mach-O
    endian: 0,        // little
    pointer_bits: usize::BITS as usize,
    c_int_bits: (size_of::<c_int>() * 8),
    id_api_size: size_of::<IdApi>(),
    id_api_alignment: align_of::<IdApi>(),
    type_count: ABI_TYPE_COUNT,
    field_count: ABI_FIELD_COUNT,
    types: [
        type_layout::<Oop>(true),
        type_layout::<Method>(false),
        type_layout::<RootProvider>(false),
        type_layout::<TraceHook>(false),
        type_layout::<Finalizer>(false),
        type_layout::<MethodOverwriteHook>(false),
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
        field_layout!(import, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(exportObj, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(isExported, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(send, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(smallint_vt, true, LAYOUT_KIND_DATA_POINTER),
        field_layout!(nilObj, true, LAYOUT_KIND_DATA_POINTER),
        field_layout!(trueObj, true, LAYOUT_KIND_DATA_POINTER),
        field_layout!(falseObj, true, LAYOUT_KIND_DATA_POINTER),
        field_layout!(mkSmallInt, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(smallIntValue, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(isSmallInt, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(string_vt, true, LAYOUT_KIND_DATA_POINTER),
        field_layout!(mkString, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(stringBytes, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(symbolName, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(vtableOf, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(gcSetStackBottom, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(gcRegisterRootProvider, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(gcCollect, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(gcLiveCount, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(setTraceHook, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(gcSetAuto, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(float_vt, true, LAYOUT_KIND_DATA_POINTER),
        field_layout!(mkFloat, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(floatValue, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(isFloat, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(setFinalizer, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(setLayout, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(gcSetPrecise, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(objectPayloadSize, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(exportedNameOf, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(slotKind, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(ensurePrivateVtable, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(setOwner, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(setParent, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(setMethodOverwriteHook, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(vtableCount, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(vtableAt, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(vtableSelectorAt, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(vtableParent, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(mkWord, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(wordValue, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(isWord, false, LAYOUT_KIND_FUNCTION_POINTER),
        field_layout!(word_vt, true, LAYOUT_KIND_DATA_POINTER),
        field_layout!(vtableOwner, false, LAYOUT_KIND_FUNCTION_POINTER),
    ],
    constants: [
        SLOT_RAW as usize,
        SLOT_OOP as usize,
        SLOT_CODE as usize,
        SLOT_UNKNOWN as usize,
    ],
    fingerprint: [
        0xb0, 0x46, 0x84, 0xb3, 0x10, 0x79, 0x27, 0xc8, 0x6b, 0x41, 0xe6, 0x13, 0xe9, 0x41, 0xdf,
        0x16, 0x94, 0xfb, 0x7b, 0x52, 0xec, 0x40, 0x1e, 0x42, 0x36, 0x20, 0xfc, 0xc1, 0x74, 0xa0,
        0xde, 0xe2,
    ],
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
compile_error!("IdApi v1 is frozen only for 64-bit arm64 macOS");

const _: () = {
    assert!(size_of::<c_int>() == 4);
    assert!(size_of::<Oop>() == 8);
    assert!(align_of::<Oop>() == 8);
    assert!(size_of::<Method>() == 8);
    assert!(align_of::<Method>() == 8);
    assert!(size_of::<RootProvider>() == 8);
    assert!(size_of::<TraceHook>() == 8);
    assert!(size_of::<Finalizer>() == 8);
    assert!(size_of::<MethodOverwriteHook>() == 8);
    assert!(size_of::<Option<MethodOverwriteHook>>() == 8);
    assert!(size_of::<IdInitFn>() == 8);
    assert!(size_of::<ModuleInitFn>() == 8);
    assert!(size_of::<IdApi>() == 432);
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
    assert_field_offset!(import, 72);
    assert_field_offset!(exportObj, 80);
    assert_field_offset!(isExported, 88);
    assert_field_offset!(send, 96);
    assert_field_offset!(smallint_vt, 104);
    assert_field_offset!(nilObj, 112);
    assert_field_offset!(trueObj, 120);
    assert_field_offset!(falseObj, 128);
    assert_field_offset!(mkSmallInt, 136);
    assert_field_offset!(smallIntValue, 144);
    assert_field_offset!(isSmallInt, 152);
    assert_field_offset!(string_vt, 160);
    assert_field_offset!(mkString, 168);
    assert_field_offset!(stringBytes, 176);
    assert_field_offset!(symbolName, 184);
    assert_field_offset!(vtableOf, 192);
    assert_field_offset!(gcSetStackBottom, 200);
    assert_field_offset!(gcRegisterRootProvider, 208);
    assert_field_offset!(gcCollect, 216);
    assert_field_offset!(gcLiveCount, 224);
    assert_field_offset!(setTraceHook, 232);
    assert_field_offset!(gcSetAuto, 240);
    assert_field_offset!(float_vt, 248);
    assert_field_offset!(mkFloat, 256);
    assert_field_offset!(floatValue, 264);
    assert_field_offset!(isFloat, 272);
    assert_field_offset!(setFinalizer, 280);
    assert_field_offset!(setLayout, 288);
    assert_field_offset!(gcSetPrecise, 296);
    assert_field_offset!(objectPayloadSize, 304);
    assert_field_offset!(exportedNameOf, 312);
    assert_field_offset!(slotKind, 320);
    assert_field_offset!(ensurePrivateVtable, 328);
    assert_field_offset!(setOwner, 336);
    assert_field_offset!(setParent, 344);
    assert_field_offset!(setMethodOverwriteHook, 352);
    assert_field_offset!(vtableCount, 360);
    assert_field_offset!(vtableAt, 368);
    assert_field_offset!(vtableSelectorAt, 376);
    assert_field_offset!(vtableParent, 384);
    assert_field_offset!(mkWord, 392);
    assert_field_offset!(wordValue, 400);
    assert_field_offset!(isWord, 408);
    assert_field_offset!(word_vt, 416);
    assert_field_offset!(vtableOwner, 424);

    assert!(SLOT_RAW == 0);
    assert!(SLOT_OOP == 1);
    assert!(SLOT_CODE == 2);
    assert!(SLOT_UNKNOWN == 255);
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_layout_matches_the_v1_manifest() {
        assert_eq!((size_of::<Oop>(), align_of::<Oop>()), (8, 8));
        assert_eq!((size_of::<Method>(), align_of::<Method>()), (8, 8));
        assert_eq!(size_of::<Option<MethodOverwriteHook>>(), 8);
        assert_eq!((size_of::<IdApi>(), align_of::<IdApi>()), (432, 8));
        assert_eq!(offset_of!(IdApi, vtableOwner), 424);
        assert_eq!(
            [SLOT_RAW, SLOT_OOP, SLOT_CODE, SLOT_UNKNOWN],
            [0, 1, 2, 255]
        );
    }
}
