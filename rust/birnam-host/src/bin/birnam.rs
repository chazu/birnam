use std::env;
use std::ffi::{CString, OsString};
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use birnam_host::{
    CommandKind, OutputFormat, ParseOutcome, ProjectBuildRequest, StartupPolicy,
    WorkerStartupError, bake_executable, baked_image, classify_command, compile_module,
    eval_script, host_target_identity, load_image, parse_bake_args, parse_build_args,
    parse_check_args, parse_compile_module_args, parse_install_args,
    parse_project_test_worker_args, parse_rebuild_args, parse_run_args, parse_test_args,
    parse_worker_args, preflight_artifact, resolve, start, start_from_environment,
};

const VERSION: &str = include_str!("../../../../VERSION");
const USAGE: &str = "birnam — Rust host\n\n\
Supported: --help, --version, <file.birnam>, <file.image>, <artifact.dylib>,\n\
           --repl, --listen <port>, doc, docgen, init, wrap,\n\
           --worker <image> <receiver> <selector> --channel-fd <fd>,\n\
           --bake <image> <exe>, --rebuild <image>\n\
Rust-hosted check/build/install/run/test/lsp are canonical.\n";

const CHECK_USAGE: &str = "usage: birnam check [directory] [--format human|json]\n";
const BUILD_USAGE: &str = "usage: birnam build [directory] [--profile debug|release]\n";
const INSTALL_USAGE: &str =
    "usage: birnam install [directory] [--build] [--profile debug|release]\n";
const TEST_USAGE: &str = "usage: birnam test [directory] [--build] [--profile debug|release]\n";
const RUN_USAGE: &str = "usage: birnam run [directory] [--build] [--profile debug|release] [--repl|--no-repl] [-- args...]\n";

fn fail(message: impl std::fmt::Display) -> ExitCode {
    eprintln!("birnam: {message}");
    ExitCode::from(1)
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    for byte in value.bytes() {
        match byte {
            b'"' => escaped.push_str("\\\""),
            b'\\' => escaped.push_str("\\\\"),
            b'\n' => escaped.push_str("\\n"),
            b'\r' => escaped.push_str("\\r"),
            b'\t' => escaped.push_str("\\t"),
            0x00..=0x1f => escaped.push_str(&format!("\\u{byte:04x}")),
            _ => escaped.push(byte as char),
        }
    }
    escaped
}

fn print_check_human(result: &birnam_host::ProjectCheckResult) {
    let resolution = &result.resolution;
    println!("project: {}", resolution.project_root);
    println!("manifest: {}", resolution.manifest);
    println!("name: {}", resolution.project_name);
    if let Some(main) = &resolution.main {
        println!("main: {main}");
    }
    if !result.wrapped.is_empty() {
        println!("wrapped:");
        for wrapped in &result.wrapped {
            println!("  {} -> lib/lib{}.dylib", wrapped.name, wrapped.name);
        }
    }
    if let Some(test_sources) = &result.test_sources
        && !test_sources.is_empty()
    {
        println!("tests:");
        for source in test_sources {
            let path = PathBuf::from(source);
            if path.is_absolute() {
                println!("  {}", path.display());
            } else {
                println!(
                    "  {}",
                    PathBuf::from(&resolution.project_root).join(path).display()
                );
            }
        }
    }
    println!("module-order:");
    for module in &resolution.modules {
        print!("  #{} <- {}", module.name, module.sources.join(" + "));
        if !module.requires.is_empty() {
            let requires = module
                .requires
                .iter()
                .map(|name| format!("#{name}"))
                .collect::<Vec<_>>()
                .join(", ");
            print!(" requires {requires}");
        }
        println!();
    }
    println!("resolution-digest: {}", result.resolution_digest);
    if result.diagnostics.is_empty() {
        println!(
            "checked {} module{}; no compile diagnostics",
            resolution.modules.len(),
            if resolution.modules.len() == 1 {
                ""
            } else {
                "s"
            }
        );
    } else {
        for diagnostic in &result.diagnostics {
            if let (Some(line), Some(column)) = (diagnostic.line, diagnostic.column) {
                println!(
                    "{}:{}:{}: compileFailed: {}",
                    diagnostic.path, line, column, diagnostic.message
                );
            } else {
                println!("{}: compileFailed: {}", diagnostic.path, diagnostic.message);
            }
        }
    }
}

fn check_command(args: &[OsString]) -> Result<(), String> {
    let outcome = parse_check_args(&args[1..]).map_err(|error| format!("check: {error:?}"))?;
    match outcome {
        ParseOutcome::Help => {
            print!("{CHECK_USAGE}");
            Ok(())
        }
        ParseOutcome::Parsed(options) => {
            let runtime = unsafe { start_from_environment() }
                .map_err(|error| format!("startup: {error:?}"))?;
            let mut anchor = 0usize;
            unsafe { runtime.prepare_execution(&mut anchor) };
            let result = unsafe { runtime.check_project(&options.directory) }
                .map_err(|error| format!("check: {error:?}"))?;
            let resolution = &result.resolution;
            match options.format {
                OutputFormat::Human => {
                    print_check_human(&result);
                }
                OutputFormat::Json => {
                    print!(
                        "{{\"schema\":\"birnam-check-v1\",\"ok\":{},\"projectRoot\":\"{}\",\"manifest\":\"{}\",\"resolutionDigest\":\"{}\",\"checkedModules\":{},\"diagnostics\":[",
                        result.diagnostics.is_empty(),
                        json_escape(&resolution.project_root),
                        json_escape(&resolution.manifest),
                        result.resolution_digest,
                        resolution.modules.len(),
                    );
                    for (index, diagnostic) in result.diagnostics.iter().enumerate() {
                        if index != 0 {
                            print!(",");
                        }
                        print!(
                            "{{\"kind\":\"compileFailed\",\"message\":\"{}\",\"detail\":\"{}\",\"path\":\"{}\"",
                            json_escape(&diagnostic.message),
                            json_escape(&diagnostic.detail),
                            json_escape(&diagnostic.path),
                        );
                        if let Some(line) = diagnostic.line {
                            print!(",\"line\":{}", line);
                        }
                        if let Some(column) = diagnostic.column {
                            print!(",\"column\":{}", column);
                        }
                        print!("}}");
                    }
                    println!("]}}");
                }
            }
            Ok(())
        }
    }
}

fn build_command(args: &[OsString]) -> Result<(), String> {
    let outcome = parse_build_args(&args[1..]).map_err(|error| format!("build: {error:?}"))?;
    match outcome {
        ParseOutcome::Help => {
            print!("{BUILD_USAGE}");
            Ok(())
        }
        ParseOutcome::Parsed(options) => {
            let runtime = unsafe { start_from_environment() }
                .map_err(|error| format!("startup: {error:?}"))?;
            let mut anchor = 0usize;
            unsafe { runtime.prepare_execution(&mut anchor) };
            let built_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| format!("build: clock: {error}"))?
                .as_secs()
                .to_string();
            let target = host_target_identity();
            let request = ProjectBuildRequest {
                start: &options.directory,
                target: &target,
                profile: &options.profile,
                tool_version: VERSION.trim(),
                built_at: &built_at,
            };
            let result = unsafe { runtime.build_project(&request) }
                .map_err(|error| format!("build: {error:?}"))?;
            println!(
                "built {} module{} at {}",
                result.artifacts.len(),
                if result.artifacts.len() == 1 { "" } else { "s" },
                result.output_root.display()
            );
            Ok(())
        }
    }
}

fn install_command(args: &[OsString]) -> Result<(), String> {
    let outcome = parse_install_args(&args[1..]).map_err(|error| format!("install: {error:?}"))?;
    match outcome {
        ParseOutcome::Help => {
            print!("{INSTALL_USAGE}");
            Ok(())
        }
        ParseOutcome::Parsed(options) => {
            let runtime = unsafe { start_from_environment() }
                .map_err(|error| format!("startup: {error:?}"))?;
            let mut anchor = 0usize;
            unsafe { runtime.prepare_execution(&mut anchor) };
            let target = host_target_identity();
            let build = if options.build {
                let built_at = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|error| format!("install: clock: {error}"))?
                    .as_secs()
                    .to_string();
                let request = ProjectBuildRequest {
                    start: &options.directory,
                    target: &target,
                    profile: &options.profile,
                    tool_version: VERSION.trim(),
                    built_at: &built_at,
                };
                unsafe { runtime.build_project(&request) }
            } else {
                unsafe {
                    runtime.validate_project_build(
                        &options.directory,
                        &target,
                        &options.profile,
                        VERSION.trim(),
                    )
                }
            }
            .map_err(|error| format!("install: {error:?}"))?;
            let count = runtime
                .install_project_build(&build)
                .map_err(|error| format!("install: {error:?}"))?;
            println!(
                "installed {count} artifact{} into {}",
                if count == 1 { "" } else { "s" },
                runtime.resources.lib_dir.display()
            );
            Ok(())
        }
    }
}

fn run_project_command(args: &[OsString]) -> Result<(), String> {
    let outcome = parse_run_args(&args[1..]).map_err(|error| format!("run: {error:?}"))?;
    match outcome {
        ParseOutcome::Help => {
            print!("{RUN_USAGE}");
            Ok(())
        }
        ParseOutcome::Parsed(options) => {
            let runtime = unsafe { start_from_environment() }
                .map_err(|error| format!("startup: {error:?}"))?;
            let mut anchor = 0usize;
            unsafe { runtime.prepare_execution(&mut anchor) };
            unsafe { runtime.run_project_with_tool_version(&options, VERSION.trim()) }
                .map_err(|error| format!("run: {error:?}"))
        }
    }
}

fn test_worker_command(args: &[OsString]) -> Result<(), String> {
    let options = parse_project_test_worker_args(&args[1..])
        .map_err(|error| format!("project-test-worker: {error:?}"))?;
    let runtime =
        unsafe { start_from_environment() }.map_err(|error| format!("startup: {error:?}"))?;
    let mut anchor = 0usize;
    unsafe { runtime.prepare_execution(&mut anchor) };
    let target = host_target_identity();
    unsafe {
        runtime
            .run_project_test_source(
                &options.project_root,
                &options.test_source,
                &target,
                &options.profile,
                VERSION.trim(),
            )
            .map_err(|error| format!("project-test-worker: {error:?}"))
    }
}

fn compile_module_command(args: &[OsString]) -> Result<(), String> {
    let options = parse_compile_module_args(&args[1..])
        .map_err(|error| format!("--compile-module: {error:?}"))?;
    let resources = resolve().map_err(|error| format!("--compile-module resources: {error:?}"))?;
    let inherited = StartupPolicy::from_environment();
    let policy = StartupPolicy::from_flags_with_precision(
        inherited.force_source,
        true,
        inherited.precise_gc,
    );
    let runtime = unsafe { start(resources, policy) }
        .map_err(|error| format!("--compile-module startup: {error:?}"))?;
    let mut anchor = 0usize;
    unsafe { runtime.prepare_execution(&mut anchor) };
    unsafe { compile_module(&runtime, &options.source, &options.output) }
        .map_err(|error| format!("--compile-module: {error:?}"))?;
    println!(
        "[stage1] compiled {} -> {}",
        options.source.display(),
        options.output.display()
    );
    Ok(())
}

fn test_command(args: &[OsString]) -> Result<(), String> {
    let outcome = parse_test_args(&args[1..]).map_err(|error| format!("test: {error:?}"))?;
    match outcome {
        ParseOutcome::Help => {
            print!("{TEST_USAGE}");
            Ok(())
        }
        ParseOutcome::Parsed(options) => {
            let runtime = unsafe { start_from_environment() }
                .map_err(|error| format!("startup: {error:?}"))?;
            let mut anchor = 0usize;
            unsafe { runtime.prepare_execution(&mut anchor) };
            let target = host_target_identity();
            if options.build {
                let built_at = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|error| format!("test: clock: {error}"))?
                    .as_secs()
                    .to_string();
                let request = ProjectBuildRequest {
                    start: &options.directory,
                    target: &target,
                    profile: &options.profile,
                    tool_version: VERSION.trim(),
                    built_at: &built_at,
                };
                unsafe { runtime.build_project(&request) }
                    .map_err(|error| format!("test: {error:?}"))?;
            }
            let snapshot = unsafe { runtime.observe_project_snapshot(&options.directory) }
                .map_err(|error| format!("test: {error:?}"))?;
            let root = snapshot
                .projects
                .first()
                .ok_or_else(|| "test: project observation was empty".to_owned())?;
            let sources = root
                .test_sources
                .clone()
                .ok_or_else(|| "test: no test sources were configured or discovered".to_owned())?;
            if sources.is_empty() {
                return Err("test: no test sources were configured or discovered".to_owned());
            }
            let executable =
                env::current_exe().map_err(|error| format!("test: executable: {error}"))?;
            let mut failures = 0usize;
            for source in &sources {
                let test_path = PathBuf::from(&root.root).join(source);
                println!("== {} ==", test_path.display());
                let output = std::process::Command::new(&executable)
                    .arg("--project-test-worker")
                    .arg(&root.root)
                    .arg(&options.profile)
                    .arg(&test_path)
                    .output()
                    .map_err(|error| format!("test: could not start {source}: {error}"))?;
                print!("{}", String::from_utf8_lossy(&output.stdout));
                eprint!("{}", String::from_utf8_lossy(&output.stderr));
                if output.status.success() {
                    println!("ok   {}", test_path.display());
                } else {
                    println!("FAIL {}", test_path.display());
                    failures += 1;
                }
            }
            println!(
                "test: {} source{}; {} failure{}",
                sources.len(),
                if sources.len() == 1 { "" } else { "s" },
                failures,
                if failures == 1 { "" } else { "s" }
            );
            if failures == 0 {
                Ok(())
            } else {
                Err("test: one or more workers failed".to_owned())
            }
        }
    }
}

fn worker_command(args: &[OsString]) -> Result<(), String> {
    let options = parse_worker_args(&args[1..]).map_err(|error| format!("--worker: {error:?}"))?;
    let mut runtime =
        unsafe { start_from_environment() }.map_err(|error| format!("startup: {error:?}"))?;
    let mut anchor = 0usize;
    unsafe { runtime.prepare_execution(&mut anchor) };
    let result = unsafe {
        runtime.run_worker_image(
            &options.image,
            &options.receiver,
            &options.selector,
            options.channel_fd,
        )
    };
    let Err(error) = result else {
        return Ok(());
    };
    let kind = match &error {
        WorkerStartupError::Image(_) => "imageRecoveryFailed",
        WorkerStartupError::MissingDispatchGeneration => "noDispatchGeneration",
        WorkerStartupError::InvalidDispatchGeneration => "invalidCurrentGeneration",
        WorkerStartupError::MissingCodegen => "noCodegen",
        WorkerStartupError::InvalidGenerationDigest => "invalidCurrentGeneration",
        WorkerStartupError::WrongArity => "wrongArity",
        WorkerStartupError::MissingGlobals => "noGlobals",
        WorkerStartupError::ReceiverNotFound => "receiverNotFound",
        WorkerStartupError::SelectorNotFound => "selectorNotFound",
        WorkerStartupError::Driver(_) => "workerFailed",
        WorkerStartupError::EmbeddedNul => "embeddedNul",
    };
    let kind = CString::new(kind).expect("static worker diagnostic kind has no NUL");
    let message = format!("{error:?}");
    unsafe {
        runtime.isolate.boot_error(
            runtime.stack.api() as *const _ as *mut _,
            options.channel_fd,
            kind.as_ptr(),
            message.as_ptr(),
            message.len(),
        );
    }
    Err(format!("worker: {error:?}"))
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
            let mut runtime = unsafe { start_from_environment() }
                .map_err(|error| format!("startup: {error:?}"))?;
            let mut anchor = 0usize;
            unsafe { runtime.prepare_execution(&mut anchor) };
            let _ = unsafe { load_image(&mut runtime, &options.image, true) }
                .map_err(|error| format!("image: {error:?}"))?;
            let mut session = unsafe { runtime.start_repl_after_image() }
                .map_err(|error| format!("repl: {error:?}"))?;
            unsafe { runtime.run_repl(&mut session) }.map_err(|error| format!("repl: {error:?}"))
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
            let mut runtime = unsafe { start_from_environment() }
                .map_err(|error| format!("startup: {error:?}"))?;
            let mut anchor = 0usize;
            unsafe { runtime.prepare_execution(&mut anchor) };
            let _ = unsafe { load_image(&mut runtime, &path, false) }
                .map_err(|error| format!("image: {error:?}"))?;
            let mut session = unsafe { runtime.start_repl_after_image() }
                .map_err(|error| format!("repl: {error:?}"))?;
            unsafe { runtime.run_repl(&mut session) }.map_err(|error| format!("repl: {error:?}"))
        }
        CommandKind::Artifact(path) => {
            // Inspect the inert descriptor before opening the language stack.
            // This keeps target/ABI/identity diagnostics available even when
            // the artifact's own native closure is incomplete.
            preflight_artifact(&path, None).map_err(|error| format!("artifact: {error:?}"))?;
            let runtime = unsafe { start_from_environment() }
                .map_err(|error| format!("startup: {error:?}"))?;
            let mut anchor = 0usize;
            unsafe { runtime.prepare_execution(&mut anchor) };
            unsafe { birnam_host::activate_artifact(&runtime, &path, None) }
                .map_err(|error| format!("artifact: {error:?}"))
        }
        CommandKind::Repl => {
            if env::var_os("BIRNAM_NO_STDLIB").is_some() {
                // Keep the bare-floor diagnostic identical to the canonical
                // host. The REPL is a self-hosted tool and is deliberately
                // unavailable when the explicit no-stdlib policy is active.
                eprintln!("birnam: the REPL requires the stdlib (unset BIRNAM_NO_STDLIB)");
                std::process::exit(1);
            }
            let runtime = unsafe { start_from_environment() }
                .map_err(|error| format!("startup: {error:?}"))?;
            let mut anchor = 0usize;
            unsafe { runtime.prepare_execution(&mut anchor) };
            let mut session =
                unsafe { runtime.start_repl() }.map_err(|error| format!("repl: {error:?}"))?;
            unsafe { runtime.run_repl(&mut session) }.map_err(|error| format!("repl: {error:?}"))
        }
        CommandKind::Doc
        | CommandKind::Docgen
        | CommandKind::Init
        | CommandKind::Wrap
        | CommandKind::Listen => {
            let runtime = unsafe { start_from_environment() }
                .map_err(|error| format!("startup: {error:?}"))?;
            let mut anchor = 0usize;
            unsafe { runtime.prepare_execution(&mut anchor) };
            unsafe { runtime.run_cli(&args) }.map_err(|error| format!("cli: {error:?}"))
        }
        CommandKind::Check => check_command(&args),
        CommandKind::Build => build_command(&args),
        CommandKind::Install => install_command(&args),
        CommandKind::Run => run_project_command(&args),
        CommandKind::Test => test_command(&args),
        CommandKind::CompileModule => compile_module_command(&args),
        CommandKind::ProjectTestWorker => test_worker_command(&args),
        CommandKind::Worker => worker_command(&args),
        CommandKind::Lsp => {
            let runtime = unsafe { start_from_environment() }
                .map_err(|error| format!("startup: {error:?}"))?;
            let mut anchor = 0usize;
            unsafe { runtime.prepare_execution(&mut anchor) };
            unsafe { runtime.run_cli(&args) }.map_err(|error| format!("lsp: {error:?}"))
        }
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
