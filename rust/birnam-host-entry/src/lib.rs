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
mod image;
mod program;
mod startup;
mod worker;
fn dispatch(context: &Context, operation: &str, args: &[Oop]) -> Result<Oop, HostError> {
    if args.len() != operation.bytes().filter(|b| *b == b':').count() {
        return Err(HostError(format!("{operation}: wrong arity")));
    }
    let host = HostRuntime { context };
    let q = context.scope();

    let result = (|| {
        match operation {
            "bootstrap:tools:" => {
                host.bootstrap(native::boolean(&q, args, 0)?, native::boolean(&q, args, 1)?)?
            }
            "bootstrapSource" => host.bootstrap_source()?,
            "loadImage:rebuild:compiled:" | "startImage:rebuild:compiled:" => {
                host.load_image_mode(
                    &native::path(&q, args, 0)?,
                    native::boolean(&q, args, 1)?,
                    native::boolean(&q, args, 2)?,
                    operation.starts_with("start"),
                )?;
                host.rebase_module(&q, &host.resources)?;
            }
            "loadImageBytes:" | "startImageBytes:" => {
                host.load_image_bytes(native::argument(&q, args, 0)?, operation.starts_with("start"))?
            }
            "evalScript:" => host.eval_script(&native::path(&q, args, 0)?)?,
            "compile:output:" => {
                host.compile_module(&native::path(&q, args, 0)?, &native::path(&q, args, 1)?)?
            }
            "loadArtifact:" => host.load_artifact(&native::path(&q, args, 0)?)?,
            "imageHasMain" => {
                return Ok(
                    host.named(if host.global(&q, b"main")? != host.named(b"nil")? {
                        b"true"
                    } else {
                        b"false"
                    })?,
                );
            }
            "imageMain" => host.run_image_main()?,
            "worker:receiver:selector:fd:" => host.run_worker(
                &native::path(&q, args, 0)?,
                &native::text(&q, args, 1)?,
                &native::text(&q, args, 2)?,
                i32::try_from(native::word(&q, args, 3)?)
                    .map_err(|_| HostError("invalid worker channel".into()))?,
            )?,
            "shutdown" => {
                host.send(host.named(b"Isolate")?, b"shutdown", &[])?;
            }
            _ => return Err(HostError("unknown HostEntry operation".into())),
        }
        Ok(host.named(b"nil")?)
    })();
    result.and_then(|value| q.finish(value))
}
birnam_host_support::host_owner!(
    b"HostEntry",
    dispatch,
    (operation_0, "bootstrap:tools:"),
    (operation_1, "bootstrapSource"),
    (operation_2, "startImage:rebuild:compiled:"),
    (operation_3, "startImageBytes:"),
    (operation_4, "loadImage:rebuild:compiled:"),
    (operation_5, "loadImageBytes:"),
    (operation_6, "evalScript:"),
    (operation_7, "compile:output:"),
    (operation_8, "loadArtifact:"),
    (operation_9, "imageHasMain"),
    (operation_10, "imageMain"),
    (operation_11, "worker:receiver:selector:fd:"),
    (operation_12, "shutdown"),
);
#[unsafe(no_mangle)]
pub extern "C" fn birnam_symbol_borrow_v1() -> i32 {
    1
}
