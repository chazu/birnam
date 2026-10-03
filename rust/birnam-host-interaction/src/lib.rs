use birnam_abi::id_protocol::{self as abi, Operand, Outcome, Protocol};
use birnam_host_support::{
    Context, HostError,
    native::{self, Owner},
};
use std::{
    ops::Deref,
    sync::atomic::{AtomicBool, AtomicPtr},
};
struct HostRuntime<'a> {
    context: &'a Context,
}
impl Deref for HostRuntime<'_> {
    type Target = Context;
    fn deref(&self) -> &Context {
        self.context
    }
}
mod interaction;
fn dispatch(context: &Context, operation: &str, args: &[Operand]) -> Result<Operand, HostError> {
    if args.len() != operation.bytes().filter(|b| *b == b':').count() {
        return Err(HostError(format!("{operation}: wrong arity")));
    }
    let host = HostRuntime { context };
    let q = context.scope();

    match operation {
        "loadTool:" => host.load_tool(&native::bytes(&q, args, 0)?)?,
        "repl:" => host.run_repl(native::boolean(&q, args, 0)?)?,
        "socket:bind:" => host.serve_socket(
            u16::try_from(native::word(&q, args, 0)?)
                .map_err(|_| HostError("invalid socket port".into()))?,
            &native::text(&q, args, 1)?,
        )?,
        "cli:" => host.run_cli_direct(&native::argv(&q, args, 0)?)?,
        _ => return Err(HostError("unknown HostInteraction operation".into())),
    }
    Ok(Operand::object(host.named(b"nil")?))
}
static INSTANCE: AtomicPtr<Owner> = AtomicPtr::new(std::ptr::null_mut());
static BUSY: AtomicBool = AtomicBool::new(false);
unsafe extern "C" fn initialize(id: *const Protocol, a: *const Operand, n: usize) -> Outcome {
    unsafe {
        native::initialize(
            id,
            a,
            n,
            &INSTANCE,
            &BUSY,
            b"HostInteraction",
            &["repl:", "socket:bind:", "cli:", "loadTool:"],
            dispatch,
        )
    }
}
#[unsafe(no_mangle)]
pub static birnam_native_module_v1: abi::Module = abi::Module {
    version: 1,
    byte_size: size_of::<abi::Module>() as u32,
    initialize: Some(initialize),
};
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
