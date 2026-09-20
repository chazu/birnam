//! Vtables and Code objects owned by their materialized JIT Unit.
//!
//! The keys are bookkeeping only. GC traces values only when it visits their
//! actual Unit; an unreachable Unit/vtable cycle therefore remains collectable.
//! Unit release removes the native storage, including after failed compilation.

use super::*;
use std::collections::BTreeMap;

static CACHES: Mutex<BTreeMap<usize, BTreeMap<u64, usize>>> = Mutex::new(BTreeMap::new());

pub(super) fn forget(unit: Oop) {
    CACHES.lock().unwrap().remove(&(unit as usize));
}

pub(super) unsafe extern "C" fn trace(unit: Oop, mark: birnam_abi::MarkFn) {
    if let Some(values) = CACHES.lock().unwrap().get(&(unit as usize)) {
        for &value in values.values() {
            // Mark enqueues a GC edge; it does not run language callbacks.
            unsafe { mark(value as Oop) };
        }
    }
}

fn owner(generation: u64) -> Option<usize> {
    // This shim is bound only into materializations using the Unit-address
    // identity convention. Other generated artifacts retain their own cache.
    if generation <= 1 || generation & 1 == 0 {
        return None;
    }
    usize::try_from(generation & !1).ok()
}

unsafe extern "C" fn get(generation: u64, index: u64) -> Oop {
    let Some(unit) = owner(generation) else {
        return ptr::null_mut();
    };
    CACHES
        .lock()
        .unwrap()
        .get(&unit)
        .and_then(|values| values.get(&index))
        .copied()
        .map_or(ptr::null_mut(), |value| value as Oop)
}

unsafe extern "C" fn set(generation: u64, index: u64, value: Oop) -> Oop {
    let Some(unit) = owner(generation) else {
        return ptr::null_mut();
    };
    // As in the generated-artifact cache contract, non-null values are live
    // Oops supplied by generated native code. Do not call IdApi here: a wrapped
    // reflection function could collect at this otherwise callback-free edge.
    if value.is_null() {
        return ptr::null_mut();
    }
    let mut caches = CACHES.lock().unwrap();
    let Some(values) = caches.get_mut(&unit) else {
        return ptr::null_mut();
    };
    values.insert(index, value as usize);
    value
}

pub(super) unsafe extern "C" fn bind(unit: Oop, _args: *const Oop, count: usize) -> Oop {
    let runtime = RUNTIME
        .get()
        .unwrap_or_else(|| abort("TCC runtime was not initialized"));
    let api = unsafe { api_ref(runtime) };
    // Validate heap membership, payload extent and type before reading state.
    // Private descendant vtables retain the Unit contract and inherited trace.
    if count != 0 || unsafe { (api.objectPayloadSize)(unit) } != size_of::<UnitPayload>() {
        return api.nilObj;
    }
    let mut vtable = unsafe { (api.vtableOf)(unit) };
    let mut remaining = unsafe { birnam_abi::vtable_enumeration::count(runtime.vtables as *const _) };
    while vtable != runtime.unit_vt as Oop && !vtable.is_null() && remaining != 0 {
        vtable = unsafe { birnam_abi::vtable_parent(api, vtable) };
        remaining -= 1;
    }
    if vtable != runtime.unit_vt as Oop {
        return api.nilObj;
    }
    let Some(state) = (unsafe { configured_state(unit) }) else {
        return api.nilObj;
    };
    if unsafe {
        tcc_add_symbol(
            state,
            c"birnam_vtable_cache_get".as_ptr(),
            get as *const c_void,
        )
    } < 0
        || unsafe {
            tcc_add_symbol(
                state,
                c"birnam_vtable_cache_set".as_ptr(),
                set as *const c_void,
            )
        } < 0
    {
        return api.nilObj;
    }
    CACHES.lock().unwrap().entry(unit as usize).or_default();
    unit
}

#[cfg(feature = "resource-metrics")]
pub(super) unsafe extern "C" fn unit_count(_: Oop, _: *const Oop, count: usize) -> Oop {
    unsafe { cache_total(count, false) }
}

#[cfg(feature = "resource-metrics")]
pub(super) unsafe extern "C" fn entry_count(_: Oop, _: *const Oop, count: usize) -> Oop {
    unsafe { cache_total(count, true) }
}

#[cfg(feature = "resource-metrics")]
unsafe fn cache_total(count: usize, entries: bool) -> Oop {
    let api = unsafe { api_ref(RUNTIME.get().expect("TCC runtime was not initialized")) };
    if count != 0 {
        return api.nilObj;
    }
    let total = {
        let caches = CACHES.lock().unwrap();
        if entries {
            caches.values().map(BTreeMap::len).sum()
        } else {
            caches.len()
        }
    };
    // Release the map borrow before result allocation can trigger collection.
    i64::try_from(total).map_or(api.nilObj, |total| unsafe { (birnam_abi::integer::make)(total) })
}

#[cfg(feature = "resource-metrics")]
pub(super) unsafe extern "C" fn check_contract(_: Oop, _: *const Oop, count: usize) -> Oop {
    let runtime = RUNTIME.get().expect("TCC runtime was not initialized");
    let api = unsafe { api_ref(runtime) };
    assert_eq!(count, 0);
    let unit = unsafe { new_unit(api, runtime) };
    assert_ne!(unit, api.nilObj);
    let generation = (unit as usize as u64) | 1;
    let ordinary = unsafe { (birnam_abi::integer::make)(73) };
    assert_eq!(unsafe { bind(ordinary, ptr::null(), 0) }, api.nilObj);
    assert_eq!(unsafe { bind(unit, ptr::null(), 1) }, api.nilObj);
    assert_eq!(unsafe { bind(unit, ptr::null(), 0) }, unit);
    assert_eq!(unsafe { bind(unit, ptr::null(), 0) }, unit);
    assert!(unsafe { get(0, 0) }.is_null());
    assert!(unsafe { get(generation & !1, 0) }.is_null());
    assert!(unsafe { set(generation, 0, ptr::null_mut()) }.is_null());
    let vtable = unsafe { (api.proto)(api.object_vt) };
    assert_eq!(unsafe { set(generation, u64::MAX, vtable) }, vtable);
    assert_eq!(unsafe { set(generation, 1_u64 << 60, ordinary) }, ordinary);
    assert_eq!(unsafe { get(generation, u64::MAX) }, vtable);
    assert_eq!(unsafe { get(generation, 1_u64 << 60) }, ordinary);
    // This is the production failure/release path, not a test-only cache reset.
    unsafe { release_payload(unit.cast()) };
    assert!(unsafe { get(generation, u64::MAX) }.is_null());
    assert!(unsafe { set(generation, 0, ordinary) }.is_null());
    assert_eq!(unsafe { bind(unit, ptr::null(), 0) }, api.nilObj);
    println!(
        "unit-cache contract: ordinary/vtable values, invalid owners/null values, repeated binding and release passed"
    );
    api.nilObj
}
