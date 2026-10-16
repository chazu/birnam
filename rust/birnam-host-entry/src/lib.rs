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
mod image;
mod program;
mod startup;
mod worker;
fn dispatch(context: &Context, operation: &str, args: &[Operand]) -> Result<Operand, HostError> {
    if args.len() != operation.bytes().filter(|b| *b == b':').count() {
        return Err(HostError(format!("{operation}: wrong arity")));
    }
    let host = HostRuntime { context };
    let q = context.scope();

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
        "loadImageBytes:" | "startImageBytes:" => host.load_image_bytes(&native::bytes(&q, args, 0)?, operation.starts_with("start"))?,
        "evalScript:" => host.eval_script(&native::path(&q, args, 0)?)?,
        "compile:output:" => {
            host.compile_module(&native::path(&q, args, 0)?, &native::path(&q, args, 1)?)?
        }
        "loadArtifact:" => host.load_artifact(&native::path(&q, args, 0)?)?,
        "imageHasMain" => {
            return Ok(Operand::object(host.named(
                if host.global(&q, b"main")? != host.named(b"nil")? {
                    b"true"
                } else {
                    b"false"
                },
            )?));
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
            b"HostEntry",
            &[
                "bootstrap:tools:",
                "bootstrapSource",
                "startImage:rebuild:compiled:",
                "startImageBytes:",
                "loadImage:rebuild:compiled:",
                "loadImageBytes:",
                "evalScript:",
                "compile:output:",
                "loadArtifact:",
                "imageHasMain",
                "imageMain",
                "worker:receiver:selector:fd:",
                "shutdown",
            ],
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
