//! Production collection order, independent of storage and metadata owners.
mod phase;
pub use phase::birnam_collection_phase_factory_v1;

use birnam_abi::collect::{
    ApiV1, DeadV1, DetachedV1, FINALIZING, IDLE, MARKING, SourceV1, VERSION,
};
use birnam_abi::storage::{BUSY, INVALID, OK, Status};
use birnam_abi::{IdApi, borrowed_slice};
use core::{cell::Cell, ffi::c_void};

thread_local! { static ACTIVE: Cell<bool> = const { Cell::new(false) }; }
struct Active;
impl Drop for Active {
    fn drop(&mut self) {
        ACTIVE.set(false);
    }
}

fn aligned<T>(pointer: *const T) -> bool {
    !pointer.is_null() && pointer.addr().is_multiple_of(align_of::<T>())
}

/// Run one collection. A nonzero result never authorizes further collection
/// effects; after effects begin the caller must fail-stop, not resume execution.
/// The result count is written only on complete success.
///
/// # Safety
/// The source, output and callbacks are valid for this synchronous owner-thread
/// call. Dead addresses stay readable/unchanged until return. Operations obey
/// their ordered contract, hold no conflicting borrow across this call and do
/// not unwind. Each detached payload stays live until its matching reclaim.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_collect_v1(source: *const SourceV1, count: *mut usize) -> Status {
    if ACTIVE.get() {
        return BUSY;
    }
    if !aligned(source) || !aligned(count) {
        return INVALID;
    }
    let source = unsafe { *source };
    if source.version != VERSION || source.byte_size != size_of::<SourceV1>() {
        return INVALID;
    }
    macro_rules! required {
        ($($field:ident),+ $(,)?) => { $(let Some($field) = source.$field else { return INVALID; };)+ };
    }
    required!(
        phase,
        clear_marks,
        trace,
        inspect_retirement,
        settle_deferred,
        retire_metadata,
        snapshot_dead,
        detach,
        reclaim,
        reclaim_deferred,
        reset_accounting,
        prune
    );
    ACTIVE.set(true);
    let _active = Active;
    let context = source.context;
    // SAFETY: all calls are synchronous and the native source contract keeps
    // its owner operations, storage and code live for the complete sequence.
    unsafe {
        phase(context, MARKING);
        clear_marks(context);
        let status = trace(context);
        if status != OK {
            return status;
        }
        match inspect_retirement(context) {
            0 => {}
            1 => settle_deferred(context),
            _ => return INVALID,
        }
        retire_metadata(context);
        let mut dead = DeadV1 {
            addresses: core::ptr::null(),
            count: 0,
        };
        snapshot_dead(context, &raw mut dead);
        let Some(addresses) = borrowed_slice(dead.addresses, dead.count) else {
            return INVALID;
        };
        phase(context, FINALIZING);
        let mut reclaimed = 0usize;
        for &address in addresses {
            let mut detached = DetachedV1 {
                object: core::ptr::null_mut(),
                finalizer: None,
            };
            let status = detach(context, address, &raw mut detached);
            if status != OK {
                return status;
            }
            if detached.object.is_null() || detached.object.addr() != address {
                return INVALID;
            }
            if let Some(finalizer) = detached.finalizer {
                finalizer(detached.object);
            }
            let status = reclaim(context, address);
            if status != OK {
                return status;
            }
            reclaimed += 1;
        }
        let Some(reclaimed) = reclaimed.checked_add(reclaim_deferred(context)) else {
            return INVALID;
        };
        clear_marks(context);
        reset_accounting(context);
        phase(context, IDLE);
        prune(context);
        count.write(reclaimed);
    }
    OK
}

static API: ApiV1 = ApiV1 {
    version: VERSION,
    byte_size: size_of::<ApiV1>(),
    collect: Some(birnam_collect_v1),
};

#[unsafe(no_mangle)]
pub extern "C" fn birnam_collect_provider_v1() -> *const ApiV1 {
    &API
}

/// Stateless module initialization; no floor state is retained.
/// # Safety
/// The loader obeys the native module lifetime contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn module_init(_: *mut IdApi, _: *mut c_void, _presence: *const birnam_abi::namespace_presence::ApiV1, _imports: *const birnam_abi::namespace_import::ApiV1, _exports: *const birnam_abi::namespace_export::ApiV1, _vtables: *const birnam_abi::vtable_enumeration::ApiV2, _observation: *const birnam_abi::method_observation::ApiV1, _roots: *const birnam_abi::root_registry::ApiV1, _settings: *const birnam_abi::gc_settings::ApiV1, _layouts: *const birnam_abi::payload_layout::ApiV1, _hooks: *const birnam_abi::trace_hooks::ApiV1, _finalizers: *const birnam_abi::finalizers::ApiV1) -> *mut c_void {
    core::ptr::null_mut()
}

#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}

#[cfg(test)]
mod tests;
