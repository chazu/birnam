//! Oop-only Id message dispatch, bootstrap objects and continuation transfer.
#[cfg(not(any(test, panic = "abort")))]
compile_error!("runnable Id requires panic=abort");
pub(crate) use birnam_abi as abi;
mod boundary;
mod id;
mod raw;
#[unsafe(no_mangle)]
pub unsafe extern "C" fn birnam_id_bootstrap_v2(
    seeds: *const abi::id_protocol::Seed,
    n: usize,
) -> *const abi::id_protocol::Protocol {
    unsafe { id::bootstrap(seeds, n) }
}
