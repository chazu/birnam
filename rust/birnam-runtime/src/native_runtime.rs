//! Transitional upper construction through the actual Id loading protocol.
//! The ten inputs are completed native descriptors, in Runtime v15 order.
use birnam_abi::id_protocol::{self as native, Operand, Outcome, Protocol};

unsafe extern "C" fn initialize(id: *const Protocol, args: *const Operand, count: usize) -> Outcome {
    let Some(args) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    if let [request, api] = args {
        if unsafe { native::valid_protocol(id) } {
            if let (Some(request), Some(api)) = (request.as_bytes(), api.as_word()) {
                if let Some(name) = unsafe { birnam_abi::borrowed_bytes(request.pointer, request.length) } {
                    if name == b"compiler" {
                        if let Some(compiler) = crate::public_api::initialize_compiler(unsafe { &*id }, api as _) {
                            return Outcome::loaded(Operand::word(compiler.addr()));
                        }
                    } else if let Some(service) = crate::public_api::construction_service(unsafe { &*id }, api as _, name) {
                        return Outcome::loaded(Operand::word(service));
                    }
                }
            }
        }
        return Outcome::failed(native::INVALID_REQUEST, 0);
    }
    let [roots, lifetime, values, namespace, model, graph, collection, primitives, support, operations] = args else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let (Some(roots), Some(lifetime), Some(values), Some(namespace), Some(model),
        Some(graph), Some(collection), Some(primitives), Some(support), Some(operations)) =
        (roots.as_word(), lifetime.as_word(), values.as_word(), namespace.as_word(),
         model.as_word(), graph.as_word(), collection.as_word(), primitives.as_word(), support.as_word(), operations.as_word()) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let api = unsafe { crate::birnam_runtime_init_v15(id, roots as _, lifetime as _, values as _,
        namespace as _, model as _, graph as _, collection as _, primitives as _, support as _, operations as _) };
    if api.is_null() { Outcome::failed(native::INVALID_REQUEST, 0) }
    else { Outcome::loaded(Operand::word(api.addr())) }
}

#[unsafe(no_mangle)]
pub static birnam_native_module_v1: native::Module = native::Module {
    version: native::VERSION,
    byte_size: size_of::<native::Module>() as u32,
    initialize: Some(initialize),
};
