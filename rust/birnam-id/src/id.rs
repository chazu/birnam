//! Id implementation. Only its native carrier crosses into upper construction.
#[cfg(any(test, feature = "ffi-test-hooks"))]
pub(crate) mod id_dispatch;
#[cfg(not(any(test, feature = "ffi-test-hooks")))]
mod id_dispatch;
#[cfg(test)]
pub(crate) mod initial_objects;
#[cfg(not(test))]
mod initial_objects;
#[cfg(test)]
pub(crate) mod native_id;
#[cfg(not(test))]
mod native_id;
#[cfg(any(test, feature = "ffi-test-hooks"))]
pub(crate) mod object_memory;
#[cfg(not(any(test, feature = "ffi-test-hooks")))]
mod object_memory;
#[cfg(any(test, feature = "ffi-test-hooks"))]
pub(crate) mod symbol;
#[cfg(not(any(test, feature = "ffi-test-hooks")))]
mod symbol;
#[cfg(any(test, feature = "ffi-test-hooks"))]
pub(crate) mod vtable;
#[cfg(not(any(test, feature = "ffi-test-hooks")))]
mod vtable;

// Legacy id_init compatibility only; production receives the native carrier.
pub(crate) use native_id::protocol;
