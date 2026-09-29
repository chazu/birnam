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
            "birnam — Rust host\nUsage: birnam <file.birnam> | <file.image> | <file.dylib> | --help | --version\n\nSource execution uses the Id message runtime and source stdlib.\nImages restore and run global main.\n--compile-module SOURCE OUTPUT creates a durable module artifact; .dylib files activate through Module.\nProject commands: check, build, run, fetch, install, test.\nREPL and tooling commands await migration."
        ),
        CommandKind::Version => println!("{}", VERSION.trim()),
        CommandKind::Check => {
            let command::ParseOutcome::Parsed(options) =
                command::parse_check_args(&args[1..]).map_err(|e| format!("{e:?}"))?
            else {
                println!("birnam check [directory] [--format=json]");
                return Ok(());
            };
            let host = HostRuntime::start(
                &resolve().map_err(|e| e.to_string())?,
                StartupPolicy {
                    skip_stdlib: false,
                    ..StartupPolicy::from_environment()
                },
            )
            .map_err(|e| e.to_string())?;
            let checked = match host.check_project(&options.directory) {
                Ok(checked) => checked,
                Err(error) => {
                    if options.format == command::OutputFormat::Json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "schema": "birnam-check-v1", "ok": false,
                                "checkedModules": 0, "projectRoot": null,
                                "resolutionDigest": null,
                                "diagnostics": [{"message": error.to_string(), "detail": error.to_string(),
                                    "module": null, "path": null, "line": null, "column": null}]
                            })
                        );
                    }
                    return Err(error.to_string());
                }
            };
            let count = checked.resolution.modules.len();
            if options.format == command::OutputFormat::Json {
                println!(
                    "{}",
                    serde_json::json!({"schema":"birnam-check-v1","ok":checked.diagnostics.is_empty(),"checkedModules":count,"projectRoot":checked.resolution.project_root,"resolutionDigest":checked.resolution_digest,"diagnostics":checked.diagnostics.iter().map(|d|serde_json::json!({"module":d.module,"path":d.path,"message":d.message,"detail":d.detail,"line":d.line,"column":d.column})).collect::<Vec<_>>() })
                );
            } else {
                for wrapped in &checked.wrapped {
                    println!("{} -> lib/lib{}.dylib", wrapped.name, wrapped.name)
                }
                if checked.diagnostics.is_empty() {
                    println!(
                        "checked {count} module{}; no compile diagnostics",
                        if count == 1 { "" } else { "s" }
                    )
                } else {
                    for d in &checked.diagnostics {
                        eprintln!("{}: {}", d.path, d.detail)
                    }
                }
            }
            if !checked.diagnostics.is_empty() {
                return Err("project check failed".into());
            }
        }
        CommandKind::Fetch => {
            let command::ParseOutcome::Parsed(options) =
                command::parse_fetch_args(&args[1..]).map_err(|e| format!("{e:?}"))?
            else {
                println!("birnam fetch [directory]");
                return Ok(());
            };
            let host = HostRuntime::start(
                &resolve().map_err(|e| e.to_string())?,
                StartupPolicy {
                    skip_stdlib: false,
                    ..StartupPolicy::from_environment()
                },
            )
            .map_err(|e| e.to_string())?;
            let result = host
                .fetch_project_dependencies(&options.directory)
                .map_err(|e| e.to_string())?;
            println!(
                "fetched {} Git dependencies; reused {} cached checkouts",
                result.fetched, result.reused
            );
        }
        CommandKind::Build => {
            let command::ParseOutcome::Parsed(options) =
                command::parse_build_args(&args[1..]).map_err(|e| format!("{e:?}"))?
            else {
                println!("birnam build [directory] [--profile=debug|release]");
                return Ok(());
            };
            let host = HostRuntime::start(
                &resolve().map_err(|e| e.to_string())?,
                StartupPolicy {
                    skip_stdlib: false,
                    ..StartupPolicy::from_environment()
                },
            )
            .map_err(|e| e.to_string())?;
            let build = host
                .build_project(&birnam_host::ProjectBuildRequest {
                    start: &options.directory,
                    target: "aarch64-macos",
                    profile: &options.profile,
                    tool_version: VERSION.trim(),
                })
                .map_err(|e| e.to_string())?;
            println!(
                "built {} modules at {} ({} reused)",
                build.artifacts.len(),
                build.output_root.display(),
                build
                    .artifacts
                    .iter()
                    .filter(|a| a.action == "reuse")
                    .count()
            );
        }
        CommandKind::Install => {
            let command::ParseOutcome::Parsed(options) =
                command::parse_install_args(&args[1..]).map_err(|e| format!("{e:?}"))?
            else {
                println!("birnam install [directory] [--build]");
                return Ok(());
            };
            let resources = resolve().map_err(|e| e.to_string())?;
            let host = HostRuntime::start(
                &resources,
                StartupPolicy {
                    skip_stdlib: false,
                    ..StartupPolicy::from_environment()
                },
            )
            .map_err(|e| e.to_string())?;
            let build = if options.build {
                host.build_project(&birnam_host::ProjectBuildRequest {
                    start: &options.directory,
                    target: "aarch64-macos",
                    profile: &options.profile,
                    tool_version: VERSION.trim(),
                })
            } else {
                host.locate_project_build(&options.directory, &options.profile)
            }
            .map_err(|e| e.to_string())?;
            let count = host
                .install_project_build(&build, &resources.lib_dir)
                .map_err(|e| e.to_string())?;
            println!(
                "installed {count} artifacts into {}",
                resources.lib_dir.display()
            );
        }
        CommandKind::Test => {
            let command::ParseOutcome::Parsed(options) =
                command::parse_test_args(&args[1..]).map_err(|e| format!("{e:?}"))?
            else {
                println!("birnam test [directory] [--build]");
                return Ok(());
            };
            let host = HostRuntime::start(
                &resolve().map_err(|e| e.to_string())?,
                StartupPolicy {
                    skip_stdlib: false,
                    ..StartupPolicy::from_environment()
                },
            )
            .map_err(|e| e.to_string())?;
            if options.build {
                host.build_project(&birnam_host::ProjectBuildRequest {
                    start: &options.directory,
                    target: "aarch64-macos",
                    profile: &options.profile,
                    tool_version: VERSION.trim(),
                })
                .map_err(|e| e.to_string())?;
            }
            let (root, sources) = host
                .project_test_sources(&options.directory)
                .map_err(|e| e.to_string())?;
            let executable = env::current_exe().map_err(|e| e.to_string())?;
            let mut passed = true;
            for source in sources {
                let status = std::process::Command::new(&executable)
                    .arg("--project-test-worker")
                    .arg(&root)
                    .arg(&options.profile)
                    .arg(source)
                    .status()
                    .map_err(|e| e.to_string())?;
                passed &= status.success();
            }
            if !passed {
                return Err("project tests failed".into());
            }
        }
        CommandKind::ProjectTestWorker => {
            let options = command::parse_project_test_worker_args(&args[1..])
                .map_err(|e| format!("{e:?}"))?;
            let host = HostRuntime::start(
                &resolve().map_err(|e| e.to_string())?,
                StartupPolicy {
                    skip_stdlib: false,
                    ..StartupPolicy::from_environment()
                },
            )
            .map_err(|e| e.to_string())?;
            host.run_project_test_source(
                &options.project_root,
                &options.test_source,
                &options.profile,
            )
            .map_err(|e| e.to_string())?;
        }
        CommandKind::Run => {
            let command::ParseOutcome::Parsed(options) =
                command::parse_run_args(&args[1..]).map_err(|e| format!("{e:?}"))?
            else {
                println!("birnam run [directory] [--build] [--no-repl] [-- arguments]");
                return Ok(());
            };
            let host = HostRuntime::start(
                &resolve().map_err(|e| e.to_string())?,
                StartupPolicy {
                    skip_stdlib: false,
                    ..StartupPolicy::from_environment()
                },
            )
            .map_err(|e| e.to_string())?;
            host.run_project(
                &birnam_host::RunOptions {
                    directory: options.directory,
                    build: options.build,
                    profile: options.profile,
                    repl: options.repl.map(|r| match r {
                        command::ReplMode::None => birnam_host::ReplMode::None,
                        command::ReplMode::Tty => birnam_host::ReplMode::Tty,
                        command::ReplMode::Socket => birnam_host::ReplMode::Socket,
                    }),
                    socket_bind: options.socket_bind,
                    socket_port: options.socket_port,
                    arguments: options.arguments,
                },
                VERSION.trim(),
            )
            .map_err(|e| e.to_string())?;
        }
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
