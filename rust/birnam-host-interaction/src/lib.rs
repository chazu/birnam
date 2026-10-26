use birnam_abi::Oop;
use birnam_host_support::{Context, HostError, native};
use std::ops::Deref;
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
fn dispatch(context: &Context, operation: &str, args: &[Oop]) -> Result<Oop, HostError> {
    if args.len() != operation.bytes().filter(|b| *b == b':').count() {
        return Err(HostError(format!("{operation}: wrong arity")));
    }
    let host = HostRuntime { context };
    let q = context.scope();

    let result = (|| {
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
        Ok(host.named(b"nil")?)
    })();
    result.and_then(|value| q.finish(value))
}
birnam_host_support::host_owner!(
    b"HostInteraction",
    dispatch,
    (operation_0, "repl:"),
    (operation_1, "socket:bind:"),
    (operation_2, "cli:"),
    (operation_3, "loadTool:"),
);
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
