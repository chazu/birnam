//! The one ad-hoc Id request: load the real Id and adopt its carrier.
use birnam_abi::id_protocol::{self as native, Operand, Outcome, Protocol};
use core::ffi::c_void;

unsafe extern "C" fn message(
    context: *mut c_void,
    receiver: *mut c_void,
    selector: *mut c_void,
    args: *const Operand,
    count: usize,
) -> Outcome {
    let protocol = unsafe { &*context.cast::<Protocol>() };
    if receiver != protocol.receiver || selector != protocol.load {
        return Outcome::failed(native::UNBOUND, 0);
    }
    let Some(arguments) = (unsafe { birnam_abi::borrowed_slice(args, count) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some((path, arguments)) = arguments.split_first() else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some(path) = path.as_bytes() else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    let Some(path) = (unsafe { birnam_abi::borrowed_bytes(path.pointer, path.length) }) else {
        return Outcome::failed(native::INVALID_REQUEST, 0);
    };
    unsafe { birnam_abi::native_module::load(protocol, path, arguments) }
}

pub fn load(path: &[u8]) -> Result<Protocol, (u32, usize)> {
    // These identities exist only for this synchronous ad-hoc request. The Id
    // initializer constructs its own real objects, selectors and message entry.
    let mut tokens = [0u8; 2];
    let mut protocol = Protocol {
        version: native::VERSION,
        byte_size: size_of::<Protocol>() as u32,
        message: Some(message),
        context: core::ptr::null_mut(),
        receiver: core::ptr::from_mut(&mut tokens[0]).cast(),
        intern: core::ptr::null_mut(),
        load: core::ptr::from_mut(&mut tokens[1]).cast(),
    };
    protocol.context = core::ptr::from_mut(&mut protocol).cast();
    let argument = Operand::bytes(path);
    let result = unsafe {
        protocol.message.unwrap()(
            protocol.context,
            protocol.receiver,
            protocol.load,
            &argument,
            1,
        )
    };
    if result.status != native::LOADED {
        return Err((result.status, result.detail));
    }
    let Some(real) = result
        .value
        .as_protocol()
        .filter(|pointer| !pointer.is_null())
    else {
        return Err((native::INCOMPATIBLE_ABI, 0));
    };
    let real = unsafe { *real };
    if real.version != native::VERSION
        || real.byte_size as usize != size_of::<Protocol>()
        || real.message.is_none()
        || real.context.is_null()
        || real.receiver.is_null()
        || real.intern.is_null()
        || real.load.is_null()
    {
        return Err((native::INCOMPATIBLE_ABI, real.version as usize));
    }
    Ok(real)
}
