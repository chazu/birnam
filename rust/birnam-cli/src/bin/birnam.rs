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
            "birnam — Rust host\nUsage: birnam <file.birnam> | <file.image> | <file.dylib> | --help | --version\n\nSource execution uses the Id message runtime and source stdlib.\nImages restore and run global main.\n--compile-module SOURCE OUTPUT creates a durable module artifact; .dylib files activate through Module.\nREPL and project commands await Host migration."
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
        CommandKind::Image(path) => {
            if args.len() != 1 {
                return Err(
                    "image execution accepts one file; image arguments are not yet supported"
                        .into(),
                );
            }
            let resources = resolve().map_err(|e| e.to_string())?;
            let host =
                HostRuntime::start_image(&resources, StartupPolicy::from_environment(), &path)
                    .map_err(|e| e.to_string())?;
            host.run_image_main()
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
        CommandKind::Worker => {
            let worker = command::parse_worker_args(&args[1..]).map_err(|e| format!("{e:?}"))?;
            let resources = resolve().map_err(|e| e.to_string())?;
            HostRuntime::run_worker(
                &resources,
                StartupPolicy::from_environment(),
                &worker.image,
                &worker.receiver,
                &worker.selector,
                worker.channel_fd,
            )
            .map_err(|e| e.to_string())?;
        }
        CommandKind::CompileModule => {
            let options =
                command::parse_compile_module_args(&args[1..]).map_err(|e| format!("{e:?}"))?;
            if options.initial_generation_resumable {
                return Err("initial-generation selection awaits migration".into());
            }
            let resources = resolve().map_err(|e| e.to_string())?;
            let host = HostRuntime::start(
                &resources,
                StartupPolicy {
                    skip_stdlib: true,
                    ..StartupPolicy::from_environment()
                },
            )
            .map_err(|e| e.to_string())?;
            host.compile_module(&options.source, &options.output)
                .map_err(|e| e.to_string())?;
        }
        CommandKind::Artifact(path) => {
            if args.len() != 1 {
                return Err("artifact execution accepts one file".into());
            }
            let resources = resolve().map_err(|e| e.to_string())?;
            let host = HostRuntime::start(&resources, StartupPolicy::from_environment())
                .map_err(|e| e.to_string())?;
            host.load_artifact(&path).map_err(|e| e.to_string())?;
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
