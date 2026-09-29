use birnam_host::{HostRuntime, StartupPolicy, resolve};
use std::{env, process::ExitCode};
#[path = "../command.rs"]
#[allow(dead_code)] // Parsers for subsequent migration slices remain tested.
mod command;
use command::{CommandKind, classify_command};
const VERSION: &str = include_str!("../../../../VERSION");
fn run() -> Result<(), String> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    match classify_command(&args).map_err(|e| format!("{e:?}"))? {
        CommandKind::Help => println!(
            "birnam — Rust host\nUsage: birnam <file.birnam> | --help | --version\n\nSource execution uses the Id message runtime and source stdlib.\nImage entry, REPL, project commands and workers await Host migration."
        ),
        CommandKind::Version => println!("{}", VERSION.trim()),
        CommandKind::Script(path) => {
            if args.len() != 1 {
                return Err(
                    "source execution accepts one file; script arguments are not yet supported"
                        .into(),
                );
            }
            let resources = resolve().map_err(|e| e.to_string())?;
            let host = HostRuntime::start(&resources, StartupPolicy::from_environment())
                .map_err(|e| e.to_string())?;
            host.eval_script(&path).map_err(|e| e.to_string())?;
        }
        command => {
            return Err(format!(
                "{command:?}: command awaits Host migration; use birnam <file.birnam>"
            ));
        }
    }
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("birnam: {error}");
            ExitCode::FAILURE
        }
    }
}
