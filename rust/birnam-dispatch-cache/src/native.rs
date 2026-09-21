//! Native loading publishes only fully callable factory operations.
use birnam_abi::id_protocol::{self as native, Operand, Outcome, Protocol};
unsafe extern "C" fn initialize(_: *const Protocol, args: *const Operand, count: usize) -> Outcome {
    let invalid = || Outcome::failed(native::INVALID_REQUEST, 0);
    let Some([argument]) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else { return invalid(); };
    let Some(bytes) = argument.as_bytes() else { return invalid(); };
    let Some(name) = (unsafe { birnam_abi::borrowed_slice(bytes.pointer, bytes.length) }) else { return invalid(); };
    let factory = match name {
        b"dispatchCache" => unsafe { super::birnam_dispatch_cache_factory_v2() }.addr(),
        b"guardVerdict" => unsafe { super::birnam_guard_verdict_factory_v1() }.addr(),
        _ => return invalid(),
    };
    Outcome::loaded(Operand::word(factory))
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: native::VERSION, byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};
