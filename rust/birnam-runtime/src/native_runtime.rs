//! Transitional upper construction through the actual Id loading protocol.
//! The eight inputs are completed native descriptors, in Runtime v13 order.
use birnam_abi::id_protocol::{self as native, Operand, Outcome, Protocol};

unsafe extern "C" fn initialize(id: *const Protocol, args: *const Operand, count: usize) -> Outcome {
    let Some(args) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let [roots, lifetime, values, namespace, model, graph, collection, primitives] = args else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let (Some(roots), Some(lifetime), Some(values), Some(namespace), Some(model),
        Some(graph), Some(collection), Some(primitives)) =
        (roots.as_word(), lifetime.as_word(), values.as_word(), namespace.as_word(),
         model.as_word(), graph.as_word(), collection.as_word(), primitives.as_word()) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let api = unsafe { crate::birnam_runtime_init_v13(id, roots as _, lifetime as _, values as _,
        namespace as _, model as _, graph as _, collection as _, primitives as _) };
    if api.is_null() { Outcome::failed(native::INVALID_REQUEST, 0) }
    else { Outcome::loaded(Operand::word(api.addr())) }
}

#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: native::VERSION,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};
