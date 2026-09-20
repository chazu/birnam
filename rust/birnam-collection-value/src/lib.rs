//! Existing private collection equivalence and key hashing; no owned objects.
#![allow(clippy::missing_safety_doc)]
use birnam_abi::collection_value::ApiV1;
use birnam_abi::{IdApi, Oop};
use core::ffi::{CStr, c_int, c_void};
use core::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
static READY: AtomicBool = AtomicBool::new(false);

unsafe fn same_value(api: &IdApi, left: Oop, right: Oop) -> bool {
    if left == right {
        return true;
    }
    if left.is_null() || right.is_null() {
        return false;
    }
    if unsafe { (birnam_abi::integer::is_integer)(left) } != 0
        && unsafe { (birnam_abi::integer::is_integer)(right) } != 0
    {
        return unsafe {
            (birnam_abi::integer::value)(left) == (birnam_abi::integer::value)(right)
        };
    }
    if unsafe { (birnam_abi::float::is_float)(left) } != 0
        && unsafe { (birnam_abi::float::is_float)(right) } != 0
    {
        return unsafe { (birnam_abi::float::value)(left) == (birnam_abi::float::value)(right) };
    }
    if unsafe { (api.vtableOf)(left) } != unsafe { birnam_abi::string::behavior() }
        || unsafe { (api.vtableOf)(right) } != unsafe { birnam_abi::string::behavior() }
    {
        return false;
    }
    let mut left_len = 0;
    let mut right_len = 0;
    let left_bytes = unsafe { (birnam_abi::string::bytes)(left, &mut left_len) };
    let right_bytes = unsafe { (birnam_abi::string::bytes)(right, &mut right_len) };
    if left_len != right_len {
        return false;
    }
    // SAFETY: stringBytes supplies borrowed byte extents for this synchronous
    // comparison; both slices end before any foreign call.
    unsafe {
        core::slice::from_raw_parts(left_bytes, left_len)
            == core::slice::from_raw_parts(right_bytes, right_len)
    }
}

fn map_mix(mut value: u64) -> u32 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (value ^ (value >> 31)) as u32
}

fn map_fnv1a(bytes: &[u8]) -> u32 {
    let mut hash = 2_166_136_261u32;
    for byte in bytes {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    hash
}

unsafe fn map_key_hash(api: &IdApi, key: Oop) -> u32 {
    if unsafe { (birnam_abi::integer::is_integer)(key) } != 0 {
        return map_mix(unsafe { (birnam_abi::integer::value)(key) } as u64);
    }
    let value_type = unsafe { (api.vtableOf)(key) };
    if value_type == unsafe { birnam_abi::string::behavior() } {
        let mut length = 0;
        let bytes = unsafe { (birnam_abi::string::bytes)(key, &mut length) };
        if length == 0 {
            return map_fnv1a(&[]);
        }
        if bytes.is_null() {
            return 0;
        }
        return map_fnv1a(unsafe { core::slice::from_raw_parts(bytes, length) });
    }
    if value_type == api.symbol_vt {
        let name = unsafe { (api.symbolName)(key) };
        if name.is_null() {
            return 0;
        }
        return map_fnv1a(unsafe { CStr::from_ptr(name) }.to_bytes());
    }
    if value_type == unsafe { birnam_abi::float::behavior() } {
        let value = unsafe { (birnam_abi::float::value)(key) };
        let bits = if value == 0.0 { 0 } else { value.to_bits() };
        return map_mix(bits);
    }
    map_mix(key as usize as u64)
}

unsafe extern "C" fn equal(api: *const IdApi, left: Oop, right: Oop) -> c_int {
    c_int::from(unsafe { same_value(&*api, left, right) })
}
unsafe extern "C" fn hash(api: *const IdApi, key: Oop) -> u32 {
    unsafe { map_key_hash(&*api, key) }
}
static API: ApiV1 = ApiV1 {
    version: 1,
    byte_size: size_of::<ApiV1>(),
    same_value: Some(equal),
    key_hash: Some(hash),
};
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(api: *mut IdApi, _: *mut c_void, _presence: *const birnam_abi::namespace_presence::ApiV1, _imports: *const birnam_abi::namespace_import::ApiV1, _exports: *const birnam_abi::namespace_export::ApiV1, _vtables: *const birnam_abi::vtable_enumeration::ApiV2, _observation: *const birnam_abi::method_observation::ApiV1, _roots: *const birnam_abi::root_registry::ApiV1) -> *mut c_void {
    assert!(!api.is_null(), "Collection value received a null IdApi");
    READY.store(true, Ordering::Release);
    ptr::null_mut()
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_collection_value_provider_v1() -> *const ApiV1 {
    if READY.load(Ordering::Acquire) {
        &API
    } else {
        ptr::null()
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> c_int {
    1
}
birnam_abi::integer_dependency!();
birnam_abi::float_dependency!();
birnam_abi::string_dependency!();
