//! Ordered live traversal. The selected source owns membership and mark bits.
use birnam_abi::storage::{BUSY, INVALID, OK, Status};
use birnam_abi::trace_live::{ApiV1, SinkV1, SourceV1, VERSION};
use core::cell::Cell;
use core::ffi::c_void;
use core::sync::atomic::{AtomicUsize, Ordering};

struct Walk {
    source: SourceV1,
    pending: Vec<usize>,
    token: usize,
    admitting: bool,
    error: Status,
}
thread_local! {
    static ACTIVE: Cell<*mut Walk> = const { Cell::new(core::ptr::null_mut()) };
}
// Process-wide tokens also reject sinks from another thread's invocation.
static NEXT_TOKEN: AtomicUsize = AtomicUsize::new(1);

struct Scope;
impl Drop for Scope {
    fn drop(&mut self) {
        ACTIVE.set(core::ptr::null_mut());
    }
}

unsafe extern "C" fn emit(context: *mut c_void, candidate: usize) -> Status {
    let pointer = ACTIVE.get();
    if pointer.is_null() {
        return BUSY;
    }
    let (admit, source_context) = unsafe {
        let walk = &mut *pointer;
        if walk.token == 0 || walk.token != context.addr() {
            return BUSY;
        }
        if walk.error != OK {
            return walk.error;
        }
        if walk.admitting {
            walk.error = BUSY;
            return BUSY;
        }
        walk.admitting = true;
        (walk.source.admit.unwrap(), walk.source.context)
    };
    // No borrow of Walk overlaps admission. Admission may mutate the source's
    // marks; roots/edges must release their source-state borrows before emit.
    let accepted = unsafe { admit(source_context, candidate) };
    let walk = unsafe { &mut *pointer };
    walk.admitting = false;
    if walk.error != OK {
        return walk.error;
    }
    match accepted {
        0 => {}
        1 => walk.pending.push(candidate),
        _ => walk.error = INVALID,
    }
    walk.error
}

unsafe fn invoke(pointer: *mut Walk, address: Option<usize>) -> Status {
    let Ok(token) = NEXT_TOKEN.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        value.checked_add(1)
    }) else {
        return INVALID;
    };
    let source = unsafe {
        (*pointer).token = token;
        (*pointer).source
    };
    let sink = SinkV1 {
        context: token as *mut c_void,
        emit,
    };
    let result = unsafe {
        match address {
            None => source.roots.unwrap()(source.context, &sink),
            Some(address) => source.edges.unwrap()(source.context, address, &sink),
        }
    };
    unsafe {
        (*pointer).token = 0;
        if (*pointer).error != OK {
            (*pointer).error
        } else {
            result
        }
    }
}

/// Mark on emission, then visit newly admitted candidates in LIFO order.
/// # Safety
/// Source callbacks and data stay live on this thread throughout the call.
/// Roots/edges use their sink synchronously and end all conflicting source
/// borrows before emitting. Admission implements a first-mark test, never emits,
/// and never calls arbitrary code. Marks and membership stay valid until return.
/// No callback may unwind. A failed trace may have changed source marks and must
/// never authorize sweeping. Sinks expire after each roots/edges invocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_trace_live_v1(source: *const SourceV1) -> Status {
    if !ACTIVE.get().is_null() {
        return BUSY;
    }
    if !super::aligned(source) {
        return INVALID;
    }
    let source = unsafe { *source };
    if source.version != VERSION
        || source.byte_size != size_of::<SourceV1>()
        || source.roots.is_none()
        || source.admit.is_none()
        || source.edges.is_none()
    {
        return INVALID;
    }
    let mut walk = Walk {
        source,
        pending: Vec::new(),
        token: 0,
        admitting: false,
        error: OK,
    };
    let pointer = &raw mut walk;
    ACTIVE.set(pointer);
    let _scope = Scope;
    let status = unsafe { invoke(pointer, None) };
    if status != OK {
        return status;
    }
    while let Some(address) = unsafe { (*pointer).pending.pop() } {
        let status = unsafe { invoke(pointer, Some(address)) };
        if status != OK {
            return status;
        }
    }
    OK
}

pub static API: ApiV1 = ApiV1 {
    version: VERSION,
    byte_size: size_of::<ApiV1>(),
    trace: Some(birnam_trace_live_v1),
};

#[unsafe(no_mangle)]
pub extern "C" fn birnam_trace_provider_v1() -> *const ApiV1 {
    &API
}

#[cfg(test)]
mod tests;
