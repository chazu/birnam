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
