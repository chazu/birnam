use std::env;
use std::ffi::OsString;
use std::fs;
use std::process::ExitCode;

use birnam_host::{
    CommandKind, ImageLoadOutcome, OutputFormat, ParseOutcome, bake_executable, baked_image,
    classify_command, eval_script, load_image, parse_bake_args, parse_build_args, parse_check_args,
    parse_install_args, parse_rebuild_args, parse_run_args, parse_test_args, parse_worker_args,
    start_from_environment,
};

const VERSION: &str = include_str!("../../../../VERSION");
const USAGE: &str = "birnam-rust-experimental — opt-in Rust host\n\n\
Supported: --help, --version, <file.birnam>, <file.image>, <artifact.dylib>,\n\
           --repl, --listen <port>, doc, docgen, init, wrap,\n\
           --worker <image> <receiver> <selector> --channel-fd <fd>,\n\
           --bake <image> <exe>, --rebuild <image>\n\
Build/install/run/test and --lsp remain on the canonical Zig host.\n";

const CHECK_USAGE: &str = "usage: birnam check [directory] [--format human|json]\n";
const BUILD_USAGE: &str = "usage: birnam build [directory] [--profile debug|release]\n";
const INSTALL_USAGE: &str =
    "usage: birnam install [directory] [--build] [--profile debug|release]\n";
const TEST_USAGE: &str = "usage: birnam test [directory] [--build] [--profile debug|release]\n";
const RUN_USAGE: &str = "usage: birnam run [directory] [--build] [--profile debug|release] [--repl|--no-repl] [-- args...]\n";

fn fail(message: impl std::fmt::Display) -> ExitCode {
    eprintln!("birnam-rust-experimental: {message}");
    ExitCode::from(1)
}

fn project_command(args: &[OsString], command: &str) -> Result<(), String> {
    let tail = &args[1..];
    let is_help = match command {
        "check" => parse_check_args(tail)
            .map(|outcome| matches!(outcome, ParseOutcome::Help))
            .map_err(|error| format!("{command}: {error:?}"))?,
        "build" => parse_build_args(tail)
            .map(|outcome| matches!(outcome, ParseOutcome::Help))
            .map_err(|error| format!("{command}: {error:?}"))?,
        "install" => parse_install_args(tail)
            .map(|outcome| matches!(outcome, ParseOutcome::Help))
            .map_err(|error| format!("{command}: {error:?}"))?,
        "test" => parse_test_args(tail)
            .map(|outcome| matches!(outcome, ParseOutcome::Help))
            .map_err(|error| format!("{command}: {error:?}"))?,
        "run" => parse_run_args(tail)
            .map(|outcome| matches!(outcome, ParseOutcome::Help))
            .map_err(|error| format!("{command}: {error:?}"))?,
        _ => unreachable!("unsupported project command parser: {command}"),
    };
    if is_help {
        let usage = match command {
            "check" => CHECK_USAGE,
            "build" => BUILD_USAGE,
            "install" => INSTALL_USAGE,
            "test" => TEST_USAGE,
            "run" => RUN_USAGE,
            _ => unreachable!("unsupported project command usage: {command}"),
        };
        print!("{usage}");
        Ok(())
    } else {
        Err(format!(
            "{command} is not implemented by the experimental host"
        ))
    }
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
                            if let (Some(line), Some(column)) = (diagnostic.line, diagnostic.column)
                            {
                                println!(
                                    "{}:{}:{}: compileFailed: {}",
                                    diagnostic.path, line, column, diagnostic.message
                                );
                            } else {
                                println!(
                                    "{}: compileFailed: {}",
                                    diagnostic.path, diagnostic.message
                                );
                            }
                        }
                    }
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

fn worker_command(args: &[OsString]) -> Result<(), String> {
    let options = parse_worker_args(&args[1..]).map_err(|error| format!("--worker: {error:?}"))?;
    let runtime =
        unsafe { start_from_environment() }.map_err(|error| format!("startup: {error:?}"))?;
    let mut anchor = 0usize;
    unsafe { runtime.prepare_execution(&mut anchor) };
    unsafe {
        runtime.run_worker_image(
            &options.image,
            &options.receiver,
            &options.selector,
            options.channel_fd,
        )
    }
    .map_err(|error| format!("worker: {error:?}"))
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
        CommandKind::Build => project_command(&args, "build"),
        CommandKind::Install => project_command(&args, "install"),
        CommandKind::Run => project_command(&args, "run"),
        CommandKind::Test => project_command(&args, "test"),
        CommandKind::Worker => worker_command(&args),
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
