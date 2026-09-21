//! Upper Runtime construction through the actual Id loading protocol.
//! The ten inputs are completed native descriptors; no separate constructor export.
use birnam_abi::id_protocol::{self as native, Operand, Outcome, Protocol};

/// Upper Runtime construction from completed lower owners and live Trace graph.
/// # Safety
/// Transfer the five native owners once; retain base values and the Trace graph.
unsafe fn construct(id: *const birnam_abi::id_protocol::Protocol, roots: *const birnam_abi::root_registry::OwnedV1, records: *const birnam_abi::lifetime::OwnedV2, values: *const birnam_abi::base_values::ValuesV1, namespace: *const birnam_abi::namespace_owner::OwnedV1, model: *const birnam_abi::object_model::OwnedV1, graph: *const birnam_abi::trace_graph::GraphV2, collection: *const birnam_abi::collection_state::OwnedV2, primitives: *const birnam_abi::id_primitives::PrimitivesV1, support: *const birnam_abi::runtime_support::FactoriesV1, operations: *const birnam_abi::runtime_values::ApisV1) -> *mut core::ffi::c_void {
    if !unsafe { birnam_abi::id_protocol::valid_protocol(id) && birnam_abi::root_registry::valid_owned(roots) && birnam_abi::lifetime::valid(records) && birnam_abi::base_values::valid(values) && birnam_abi::namespace_owner::valid(namespace) && birnam_abi::object_model::valid(model) && birnam_abi::trace_graph::valid(graph) && birnam_abi::collection_state::valid(collection) && birnam_abi::id_primitives::valid(primitives) && birnam_abi::runtime_support::valid(support) && birnam_abi::runtime_values::valid(operations) } { return core::ptr::null_mut(); }
    crate::public_api::initialize_with_owners(unsafe { *id }, Some(unsafe { crate::root_registry::Service::adopt(roots) }), Some(unsafe { crate::lifetime::Owner::adopt(records) }), Some(unsafe { *values }), Some(unsafe { crate::env::Environment::adopt(namespace) }), Some(unsafe { crate::object_model::Owner::adopt(model) }), Some(unsafe { *graph }), Some(unsafe { crate::collection_state::Owner::adopt(collection) }), Some(unsafe { *primitives }), unsafe { *support }, Some(unsafe { *operations })).cast()
}

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
    let api = unsafe { construct(id, roots as _, lifetime as _, values as _,
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
