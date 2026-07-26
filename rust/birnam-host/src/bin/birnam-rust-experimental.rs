use std::env;
use std::ffi::OsString;
use std::fs;
use std::process::ExitCode;

use birnam_host::{
    CommandKind, ImageLoadOutcome, bake_executable, baked_image, classify_command, eval_script,
    load_image, parse_bake_args, parse_rebuild_args, start_from_environment,
};

const VERSION: &str = include_str!("../../../../VERSION");
const USAGE: &str = "birnam-rust-experimental — opt-in Rust host\n\n\
Supported: --help, --version, <file.birnam>, <file.image>, <artifact.dylib>,\n\
           --repl, --bake <image> <exe>, --rebuild <image>\n\
Project/CLI dispatch remains on the canonical Zig host.\n";

fn fail(message: impl std::fmt::Display) -> ExitCode {
    eprintln!("birnam-rust-experimental: {message}");
    ExitCode::from(1)
}

fn run(args: Vec<OsString>) -> Result<(), String> {
    let command = classify_command(&args).map_err(|error| format!("command: {error:?}"))?;
    match command {
        CommandKind::Help => {
            print!("{USAGE}");
            Ok(())
        }
        CommandKind::Version => {
            println!("birnam {VERSION}");
            Ok(())
        }
        CommandKind::Bake => {
            let options =
                parse_bake_args(&args[1..]).map_err(|error| format!("bake: {error:?}"))?;
            let runtime_path =
                env::current_exe().map_err(|error| format!("current exe: {error}"))?;
            let runtime =
                fs::read(runtime_path).map_err(|error| format!("read runtime: {error}"))?;
            let image = fs::read(&options.image).map_err(|error| format!("read image: {error}"))?;
            let baked =
                bake_executable(&runtime, &image).map_err(|error| format!("bake: {error:?}"))?;
            fs::write(&options.executable, baked)
                .map_err(|error| format!("write executable: {error}"))?;
            Ok(())
        }
        CommandKind::Rebuild => {
            let options =
                parse_rebuild_args(&args[1..]).map_err(|error| format!("rebuild: {error:?}"))?;
            let runtime = unsafe { start_from_environment() }
                .map_err(|error| format!("startup: {error:?}"))?;
            let mut anchor = 0usize;
            unsafe { runtime.prepare_execution(&mut anchor) };
            match unsafe { load_image(&runtime, &options.image, true) }
                .map_err(|error| format!("image: {error:?}"))?
            {
                ImageLoadOutcome::Mapped | ImageLoadOutcome::SourceRecovery => Ok(()),
            }
        }
        CommandKind::Script(path) => {
            let runtime = unsafe { start_from_environment() }
                .map_err(|error| format!("startup: {error:?}"))?;
            let mut anchor = 0usize;
            unsafe { runtime.prepare_execution(&mut anchor) };
            unsafe { eval_script(&runtime, &path) }
                .map(|_| ())
                .map_err(|error| format!("script: {error:?}"))
        }
        CommandKind::Image(path) => {
            let runtime = unsafe { start_from_environment() }
                .map_err(|error| format!("startup: {error:?}"))?;
            let mut anchor = 0usize;
            unsafe { runtime.prepare_execution(&mut anchor) };
            unsafe { load_image(&runtime, &path, false) }
                .map(|_| ())
                .map_err(|error| format!("image: {error:?}"))
        }
        CommandKind::Artifact(path) => {
            let runtime = unsafe { start_from_environment() }
                .map_err(|error| format!("startup: {error:?}"))?;
            let mut anchor = 0usize;
            unsafe { runtime.prepare_execution(&mut anchor) };
            unsafe { birnam_host::activate_artifact(&runtime, &path, None) }
                .map_err(|error| format!("artifact: {error:?}"))
        }
        CommandKind::Repl => {
            let runtime = unsafe { start_from_environment() }
                .map_err(|error| format!("startup: {error:?}"))?;
            let mut anchor = 0usize;
            unsafe { runtime.prepare_execution(&mut anchor) };
            let mut session =
                unsafe { runtime.start_repl() }.map_err(|error| format!("repl: {error:?}"))?;
            unsafe { runtime.run_repl(&mut session) }.map_err(|error| format!("repl: {error:?}"))
        }
        other => Err(format!(
            "{other:?} is not implemented by the experimental host"
        )),
    }
}

fn main() -> ExitCode {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    // Keep the trailer helper linked and checked in this executable's image
    // path without probing the filesystem during pure help/version exits.
    let _ = baked_image as fn(&[u8]) -> Option<&[u8]>;
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(error),
    }
}
