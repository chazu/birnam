//! Pure command classification and option parsing.
use std::ffi::c_int;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EarlyAction {
    Continue,
    Help,
    Version,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandKind {
    Help,
    Version,
    Repl,
    Listen,
    Lsp,
    Doc,
    Docgen,
    Init,
    Fetch,
    Check,
    Build,
    Install,
    Run,
    Test,
    Wrap,
    Bake,
    Rebuild,
    CompileModule,
    Worker,
    ProjectTestWorker,
    Script(PathBuf),
    Image(PathBuf),
    Artifact(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    UnknownOption(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Human,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckOptions {
    pub directory: PathBuf,
    pub format: OutputFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchOptions {
    pub directory: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildOptions {
    pub directory: PathBuf,
    pub profile: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallOptions {
    pub directory: PathBuf,
    pub build: bool,
    pub profile: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestOptions {
    pub directory: PathBuf,
    pub build: bool,
    pub profile: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTestWorkerOptions {
    pub project_root: PathBuf,
    pub profile: String,
    pub test_source: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectTestWorkerParseError {
    WrongArity,
    NonUtf8Profile,
    InvalidProfile(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplMode {
    None,
    Tty,
    Socket,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOptions {
    pub directory: PathBuf,
    pub build: bool,
    pub profile: String,
    pub repl: Option<ReplMode>,
    pub socket_bind: Option<String>,
    pub socket_port: Option<u16>,
    pub arguments: Vec<std::ffi::OsString>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunParseError {
    UnknownOption(String),
    MissingValue(&'static str),
    InvalidValue { option: &'static str, value: String },
    DuplicateDirectory,
    UnexpectedAfterHelp,
    NonUtf8Value(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BakeOptions {
    pub image: PathBuf,
    pub executable: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebuildOptions {
    pub image: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileModuleOptions {
    pub source: PathBuf,
    pub output: PathBuf,
    pub initial_generation_resumable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolingParseError {
    Arity {
        command: &'static str,
        expected: usize,
        actual: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerOptions {
    pub image: PathBuf,
    pub receiver: String,
    pub selector: String,
    pub channel_fd: c_int,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerParseError {
    Arity { expected: usize, actual: usize },
    MissingChannelFlag(String),
    InvalidChannelFd(String),
    NonUtf8Receiver,
    NonUtf8Selector,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseOutcome<T> {
    Parsed(T),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectParseError {
    UnknownOption(String),
    MissingValue(&'static str),
    InvalidValue { option: &'static str, value: String },
    DuplicateDirectory,
    UnexpectedAfterHelp,
    NonUtf8Value(&'static str),
}

fn option_value<T: AsRef<std::ffi::OsStr>>(
    option: &'static str,
    value: &T,
) -> Result<String, ProjectParseError> {
    value
        .as_ref()
        .to_str()
        .map(str::to_owned)
        .ok_or(ProjectParseError::NonUtf8Value(option))
}

fn parse_profile(value: String) -> Result<String, ProjectParseError> {
    if value == "debug" || value == "release" {
        Ok(value)
    } else {
        Err(ProjectParseError::InvalidValue {
            option: "--profile",
            value,
        })
    }
}

pub fn parse_check_args<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<ParseOutcome<CheckOptions>, ProjectParseError> {
    let mut directory = None;
    let mut format = OutputFormat::Human;
    let mut index = 0;
    while index < args.len() {
        let token = args[index].as_ref();
        if token == std::ffi::OsStr::new("--help") || token == std::ffi::OsStr::new("-h") {
            if index + 1 != args.len() {
                return Err(ProjectParseError::UnexpectedAfterHelp);
            }
            return Ok(ParseOutcome::Help);
        }
        let token_text = token.to_string_lossy();
        if token == std::ffi::OsStr::new("--format") {
            index += 1;
            let value = args
                .get(index)
                .ok_or(ProjectParseError::MissingValue("--format"))
                .and_then(|value| option_value("--format", value))?;
            format = match value.as_str() {
                "human" => OutputFormat::Human,
                "json" => OutputFormat::Json,
                _ => {
                    return Err(ProjectParseError::InvalidValue {
                        option: "--format",
                        value,
                    });
                }
            };
        } else if let Some(value) = token_text.strip_prefix("--format=") {
            format = match value {
                "human" => OutputFormat::Human,
                "json" => OutputFormat::Json,
                _ => {
                    return Err(ProjectParseError::InvalidValue {
                        option: "--format",
                        value: value.to_owned(),
                    });
                }
            };
        } else if token_text.starts_with('-') {
            return Err(ProjectParseError::UnknownOption(token_text.into_owned()));
        } else if directory.is_some() {
            return Err(ProjectParseError::DuplicateDirectory);
        } else {
            directory = Some(PathBuf::from(token));
        }
        index += 1;
    }
    Ok(ParseOutcome::Parsed(CheckOptions {
        directory: directory.unwrap_or_else(|| PathBuf::from(".")),
        format,
    }))
}

pub fn parse_fetch_args<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<ParseOutcome<FetchOptions>, ProjectParseError> {
    let mut directory = None;
    for (index, argument) in args.iter().enumerate() {
        let token = argument.as_ref();
        if token == std::ffi::OsStr::new("--help") || token == std::ffi::OsStr::new("-h") {
            if index + 1 != args.len() {
                return Err(ProjectParseError::UnexpectedAfterHelp);
            }
            return Ok(ParseOutcome::Help);
        }
        let token_text = token.to_string_lossy();
        if token_text.starts_with('-') {
            return Err(ProjectParseError::UnknownOption(token_text.into_owned()));
        }
        if directory.is_some() {
            return Err(ProjectParseError::DuplicateDirectory);
        }
        directory = Some(PathBuf::from(token));
    }
    Ok(ParseOutcome::Parsed(FetchOptions {
        directory: directory.unwrap_or_else(|| PathBuf::from(".")),
    }))
}

pub fn parse_build_args<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<ParseOutcome<BuildOptions>, ProjectParseError> {
    let mut directory = None;
    let mut profile = String::from("debug");
    let mut index = 0;
    while index < args.len() {
        let token = args[index].as_ref();
        if token == std::ffi::OsStr::new("--help") || token == std::ffi::OsStr::new("-h") {
            if index + 1 != args.len() {
                return Err(ProjectParseError::UnexpectedAfterHelp);
            }
            return Ok(ParseOutcome::Help);
        }
        let token_text = token.to_string_lossy();
        if token == std::ffi::OsStr::new("--profile") {
            index += 1;
            let value = args
                .get(index)
                .ok_or(ProjectParseError::MissingValue("--profile"))
                .and_then(|value| option_value("--profile", value))?;
            profile = parse_profile(value)?;
        } else if let Some(value) = token_text.strip_prefix("--profile=") {
            profile = parse_profile(value.to_owned())?;
        } else if token_text.starts_with('-') {
            return Err(ProjectParseError::UnknownOption(token_text.into_owned()));
        } else if directory.is_some() {
            return Err(ProjectParseError::DuplicateDirectory);
        } else {
            directory = Some(PathBuf::from(token));
        }
        index += 1;
    }
    Ok(ParseOutcome::Parsed(BuildOptions {
        directory: directory.unwrap_or_else(|| PathBuf::from(".")),
        profile,
    }))
}

fn parse_buildish_args<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<ParseOutcome<(PathBuf, bool, String)>, ProjectParseError> {
    let mut directory = None;
    let mut build = false;
    let mut profile = String::from("debug");
    let mut index = 0;
    while index < args.len() {
        let token = args[index].as_ref();
        if token == std::ffi::OsStr::new("--help") || token == std::ffi::OsStr::new("-h") {
            if index + 1 != args.len() {
                return Err(ProjectParseError::UnexpectedAfterHelp);
            }
            return Ok(ParseOutcome::Help);
        }
        let token_text = token.to_string_lossy();
        if token == std::ffi::OsStr::new("--build") {
            build = true;
        } else if token == std::ffi::OsStr::new("--profile") {
            index += 1;
            let value = args
                .get(index)
                .ok_or(ProjectParseError::MissingValue("--profile"))
                .and_then(|value| option_value("--profile", value))?;
            profile = parse_profile(value)?;
        } else if let Some(value) = token_text.strip_prefix("--profile=") {
            profile = parse_profile(value.to_owned())?;
        } else if token_text.starts_with('-') {
            return Err(ProjectParseError::UnknownOption(token_text.into_owned()));
        } else if directory.is_some() {
            return Err(ProjectParseError::DuplicateDirectory);
        } else {
            directory = Some(PathBuf::from(token));
        }
        index += 1;
    }
    Ok(ParseOutcome::Parsed((
        directory.unwrap_or_else(|| PathBuf::from(".")),
        build,
        profile,
    )))
}

pub fn parse_install_args<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<ParseOutcome<InstallOptions>, ProjectParseError> {
    Ok(match parse_buildish_args(args)? {
        ParseOutcome::Help => ParseOutcome::Help,
        ParseOutcome::Parsed((directory, build, profile)) => ParseOutcome::Parsed(InstallOptions {
            directory,
            build,
            profile,
        }),
    })
}

pub fn parse_test_args<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<ParseOutcome<TestOptions>, ProjectParseError> {
    Ok(match parse_buildish_args(args)? {
        ParseOutcome::Help => ParseOutcome::Help,
        ParseOutcome::Parsed((directory, build, profile)) => ParseOutcome::Parsed(TestOptions {
            directory,
            build,
            profile,
        }),
    })
}

/// Parse the private project-test worker triplet emitted by the Rust test
/// driver. The worker receives no user flags and preserves exact paths.
pub fn parse_project_test_worker_args<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<ProjectTestWorkerOptions, ProjectTestWorkerParseError> {
    if args.len() != 3 {
        return Err(ProjectTestWorkerParseError::WrongArity);
    }
    let profile = args[1]
        .as_ref()
        .to_str()
        .ok_or(ProjectTestWorkerParseError::NonUtf8Profile)?;
    if profile != "debug" && profile != "release" {
        return Err(ProjectTestWorkerParseError::InvalidProfile(
            profile.to_owned(),
        ));
    }
    Ok(ProjectTestWorkerOptions {
        project_root: PathBuf::from(args[0].as_ref()),
        profile: profile.to_owned(),
        test_source: PathBuf::from(args[2].as_ref()),
    })
}

fn run_value<T: AsRef<std::ffi::OsStr>>(
    option: &'static str,
    value: &T,
) -> Result<String, RunParseError> {
    value
        .as_ref()
        .to_str()
        .map(str::to_owned)
        .ok_or(RunParseError::NonUtf8Value(option))
}

fn run_profile(value: String) -> Result<String, RunParseError> {
    if value == "debug" || value == "release" {
        Ok(value)
    } else {
        Err(RunParseError::InvalidValue {
            option: "--profile",
            value,
        })
    }
}

fn run_repl(value: String) -> Result<ReplMode, RunParseError> {
    match value.as_str() {
        "none" => Ok(ReplMode::None),
        "tty" => Ok(ReplMode::Tty),
        "socket" => Ok(ReplMode::Socket),
        _ => Err(RunParseError::InvalidValue {
            option: "--repl",
            value,
        }),
    }
}

pub fn parse_run_args<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<ParseOutcome<RunOptions>, RunParseError> {
    let mut directory = None;
    let mut build = false;
    let mut profile = String::from("debug");
    let mut repl = None;
    let mut socket_bind = None;
    let mut socket_port = None;
    let mut arguments = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let token = args[index].as_ref();
        if token == std::ffi::OsStr::new("--") {
            arguments.extend(args[index + 1..].iter().map(|arg| arg.as_ref().to_owned()));
            break;
        }
        if token == std::ffi::OsStr::new("--help") || token == std::ffi::OsStr::new("-h") {
            if index + 1 != args.len() {
                return Err(RunParseError::UnexpectedAfterHelp);
            }
            return Ok(ParseOutcome::Help);
        }
        let token_text = token.to_string_lossy();
        if token == std::ffi::OsStr::new("--build") {
            build = true;
        } else if token == std::ffi::OsStr::new("--profile") {
            index += 1;
            let value = args
                .get(index)
                .ok_or(RunParseError::MissingValue("--profile"))
                .and_then(|value| run_value("--profile", value))?;
            profile = run_profile(value)?;
        } else if let Some(value) = token_text.strip_prefix("--profile=") {
            profile = run_profile(value.to_owned())?;
        } else if token == std::ffi::OsStr::new("--no-repl") {
            repl = Some(ReplMode::None);
        } else if token == std::ffi::OsStr::new("--repl") {
            repl = Some(ReplMode::Tty);
        } else if let Some(value) = token_text.strip_prefix("--repl=") {
            repl = Some(run_repl(value.to_owned())?);
        } else if token == std::ffi::OsStr::new("--repl-bind") {
            index += 1;
            let value = args
                .get(index)
                .ok_or(RunParseError::MissingValue("--repl-bind"))
                .and_then(|value| run_value("--repl-bind", value))?;
            if value.is_empty() {
                return Err(RunParseError::MissingValue("--repl-bind"));
            }
            socket_bind = Some(value);
        } else if let Some(value) = token_text.strip_prefix("--repl-bind=") {
            if value.is_empty() {
                return Err(RunParseError::MissingValue("--repl-bind"));
            }
            socket_bind = Some(value.to_owned());
        } else if token == std::ffi::OsStr::new("--repl-port") {
            index += 1;
            let value = args
                .get(index)
                .ok_or(RunParseError::MissingValue("--repl-port"))
                .and_then(|value| run_value("--repl-port", value))?;
            socket_port = Some(value.parse().map_err(|_| RunParseError::InvalidValue {
                option: "--repl-port",
                value,
            })?);
        } else if let Some(value) = token_text.strip_prefix("--repl-port=") {
            socket_port = Some(value.parse().map_err(|_| RunParseError::InvalidValue {
                option: "--repl-port",
                value: value.to_owned(),
            })?);
        } else if token_text.starts_with('-') {
            return Err(RunParseError::UnknownOption(token_text.into_owned()));
        } else if directory.is_some() {
            return Err(RunParseError::DuplicateDirectory);
        } else {
            directory = Some(PathBuf::from(token));
        }
        index += 1;
    }
    Ok(ParseOutcome::Parsed(RunOptions {
        directory: directory.unwrap_or_else(|| PathBuf::from(".")),
        build,
        profile,
        repl,
        socket_bind,
        socket_port,
        arguments,
    }))
}

pub fn parse_bake_args<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<BakeOptions, ToolingParseError> {
    if args.len() != 2 {
        return Err(ToolingParseError::Arity {
            command: "--bake",
            expected: 2,
            actual: args.len(),
        });
    }
    Ok(BakeOptions {
        image: PathBuf::from(args[0].as_ref()),
        executable: PathBuf::from(args[1].as_ref()),
    })
}

pub fn parse_rebuild_args<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<RebuildOptions, ToolingParseError> {
    if args.len() != 1 {
        return Err(ToolingParseError::Arity {
            command: "--rebuild",
            expected: 1,
            actual: args.len(),
        });
    }
    Ok(RebuildOptions {
        image: PathBuf::from(args[0].as_ref()),
    })
}

pub fn parse_compile_module_args<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<CompileModuleOptions, ToolingParseError> {
    if (args.len() != 2 && args.len() != 3)
        || args.get(2).is_some_and(|value| {
            value.as_ref() != std::ffi::OsStr::new("--initial-generation-resumable")
        })
    {
        return Err(ToolingParseError::Arity {
            command: "--compile-module",
            expected: 2,
            actual: args.len(),
        });
    }
    Ok(CompileModuleOptions {
        source: PathBuf::from(args[0].as_ref()),
        output: PathBuf::from(args[1].as_ref()),
        initial_generation_resumable: args.get(2).is_some_and(|value| {
            value.as_ref() == std::ffi::OsStr::new("--initial-generation-resumable")
        }),
    })
}

/// Parse the private isolate-worker route without opening the image or floor.
/// The channel descriptor remains an inherited integer; ownership and worker
/// protocol execution stay with the isolate hook.
pub fn parse_worker_args<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<WorkerOptions, WorkerParseError> {
    if args.len() != 5 {
        return Err(WorkerParseError::Arity {
            expected: 5,
            actual: args.len(),
        });
    }
    if args[3].as_ref() != std::ffi::OsStr::new("--channel-fd") {
        return Err(WorkerParseError::MissingChannelFlag(
            args[3].as_ref().to_string_lossy().into_owned(),
        ));
    }
    let receiver = args[1]
        .as_ref()
        .to_str()
        .ok_or(WorkerParseError::NonUtf8Receiver)?
        .to_owned();
    let selector = args[2]
        .as_ref()
        .to_str()
        .ok_or(WorkerParseError::NonUtf8Selector)?
        .to_owned();
    let channel_fd = args[4]
        .as_ref()
        .to_str()
        .ok_or_else(|| WorkerParseError::InvalidChannelFd("non-UTF-8".to_owned()))?
        .parse::<c_int>()
        .map_err(|_| {
            WorkerParseError::InvalidChannelFd(args[4].as_ref().to_string_lossy().into_owned())
        })?;
    Ok(WorkerOptions {
        image: PathBuf::from(args[0].as_ref()),
        receiver,
        selector,
        channel_fd,
    })
}

/// Classify only the top-level command family. Subcommand option validation
/// remains a later parser seam; this function therefore cannot consume a
/// project directory or reinterpret `--` arguments as host flags.
pub fn classify_command<T: AsRef<std::ffi::OsStr>>(
    args: &[T],
) -> Result<CommandKind, CommandError> {
    let Some(first) = args.first().map(AsRef::as_ref) else {
        return Ok(CommandKind::Repl);
    };
    let known = [
        ("--help", CommandKind::Help),
        ("-h", CommandKind::Help),
        ("--version", CommandKind::Version),
        ("-V", CommandKind::Version),
        ("--repl", CommandKind::Repl),
        ("-i", CommandKind::Repl),
        ("repl", CommandKind::Repl),
        ("--listen", CommandKind::Listen),
        ("--serve", CommandKind::Listen),
        ("--lsp", CommandKind::Lsp),
        ("doc", CommandKind::Doc),
        ("docgen", CommandKind::Docgen),
        ("init", CommandKind::Init),
        ("fetch", CommandKind::Fetch),
        ("check", CommandKind::Check),
        ("build", CommandKind::Build),
        ("install", CommandKind::Install),
        ("run", CommandKind::Run),
        ("test", CommandKind::Test),
        ("wrap", CommandKind::Wrap),
        ("--bake", CommandKind::Bake),
        ("--rebuild", CommandKind::Rebuild),
        ("--compile-module", CommandKind::CompileModule),
        ("--worker", CommandKind::Worker),
        ("--project-test-worker", CommandKind::ProjectTestWorker),
    ];
    for (name, command) in known {
        if first == std::ffi::OsStr::new(name) {
            return Ok(command);
        }
    }
    if first.to_string_lossy().starts_with('-') {
        return Err(CommandError::UnknownOption(
            first.to_string_lossy().into_owned(),
        ));
    }
    let path = PathBuf::from(first);
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("image") => Ok(CommandKind::Image(path)),
        Some("dylib") => Ok(CommandKind::Artifact(path)),
        _ => Ok(CommandKind::Script(path)),
    }
}

/// Classify the pure CLI exits that must run before any runtime-resource or
/// floor access. Remaining arguments stay opaque until the host dispatch slice
/// ports the project/REPL command grammar.
pub fn classify_early_args<T: AsRef<std::ffi::OsStr>>(args: &[T]) -> EarlyAction {
    match args.first().map(AsRef::as_ref).and_then(|arg| arg.to_str()) {
        Some("--help" | "-h") => EarlyAction::Help,
        Some("--version" | "-V") => EarlyAction::Version,
        _ => EarlyAction::Continue,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn project_test_worker_parser_preserves_paths_and_profile() {
        let parsed = parse_project_test_worker_args(&[
            "/tmp/project",
            "release",
            "/tmp/project/tests/app.birnam",
        ])
        .expect("worker arguments parse");
        assert_eq!(parsed.project_root, Path::new("/tmp/project"));
        assert_eq!(parsed.profile, "release");
        assert_eq!(
            parsed.test_source,
            Path::new("/tmp/project/tests/app.birnam")
        );
        assert!(matches!(
            parse_project_test_worker_args(&["/tmp/project", "fast", "test.birnam"]),
            Err(ProjectTestWorkerParseError::InvalidProfile(value)) if value == "fast"
        ));
    }

    #[test]
    fn pure_help_and_version_exits_precede_resource_resolution() {
        assert_eq!(classify_early_args(&["--help"]), EarlyAction::Help);
        assert_eq!(classify_early_args(&["-V"]), EarlyAction::Version);
        assert_eq!(
            classify_early_args(&["project.birnam"]),
            EarlyAction::Continue
        );
        assert_eq!(classify_early_args::<&str>(&[]), EarlyAction::Continue);
    }

    #[test]
    fn top_level_command_classification_preserves_file_kinds() {
        assert_eq!(classify_command::<&str>(&[]), Ok(CommandKind::Repl));
        assert_eq!(
            classify_command(&["--lsp"]).expect("lsp is a top-level command"),
            CommandKind::Lsp
        );
        assert_eq!(
            classify_command(&["program.birnam"]).expect("source is a script"),
            CommandKind::Script(PathBuf::from("program.birnam"))
        );
        assert_eq!(
            classify_command(&["saved.image"]).expect("image extension is explicit"),
            CommandKind::Image(PathBuf::from("saved.image"))
        );
        assert_eq!(
            classify_command(&["libbirnam_app.dylib"]).expect("dylib extension is explicit"),
            CommandKind::Artifact(PathBuf::from("libbirnam_app.dylib"))
        );
    }

    #[test]
    fn top_level_unknown_flags_are_rejected_without_runtime_access() {
        assert_eq!(
            classify_command(&["--not-a-command"]),
            Err(CommandError::UnknownOption("--not-a-command".to_owned()))
        );
        assert_eq!(
            classify_command(&["--rebuild"]).expect("rebuild is top-level"),
            CommandKind::Rebuild
        );
        assert_eq!(
            classify_command(&["--project-test-worker"]).expect("worker is internal"),
            CommandKind::ProjectTestWorker
        );
        assert_eq!(
            classify_command(&["--compile-module"]).expect("compile module is internal"),
            CommandKind::CompileModule
        );
    }

    #[test]
    fn check_parser_preserves_defaults_formats_and_help() {
        assert_eq!(
            parse_check_args::<&str>(&[]).expect("defaults parse"),
            ParseOutcome::Parsed(CheckOptions {
                directory: PathBuf::from("."),
                format: OutputFormat::Human,
            })
        );
        assert_eq!(
            parse_check_args(&["--format=json", "project"]).expect("format and directory parse"),
            ParseOutcome::Parsed(CheckOptions {
                directory: PathBuf::from("project"),
                format: OutputFormat::Json,
            })
        );
        assert_eq!(
            parse_check_args(&["--help"]).expect("help parses"),
            ParseOutcome::Help
        );
    }

    #[test]
    fn fetch_parser_accepts_only_one_project_directory() {
        assert_eq!(
            classify_command(&["fetch"]).expect("fetch is top-level"),
            CommandKind::Fetch
        );
        assert_eq!(
            parse_fetch_args::<&str>(&[]).expect("defaults parse"),
            ParseOutcome::Parsed(FetchOptions {
                directory: PathBuf::from("."),
            })
        );
        assert_eq!(
            parse_fetch_args(&["project"]).expect("directory parses"),
            ParseOutcome::Parsed(FetchOptions {
                directory: PathBuf::from("project"),
            })
        );
        assert_eq!(
            parse_fetch_args(&["one", "two"]),
            Err(ProjectParseError::DuplicateDirectory)
        );
    }

    #[test]
    fn build_parser_preserves_profile_aliases_and_validation() {
        assert_eq!(
            parse_build_args(&["--profile", "release", "project"])
                .expect("profile and directory parse"),
            ParseOutcome::Parsed(BuildOptions {
                directory: PathBuf::from("project"),
                profile: "release".to_owned(),
            })
        );
        assert_eq!(
            parse_build_args(&["--profile=fast"]),
            Err(ProjectParseError::InvalidValue {
                option: "--profile",
                value: "fast".to_owned(),
            })
        );
        assert_eq!(
            parse_build_args(&["one", "two"]),
            Err(ProjectParseError::DuplicateDirectory)
        );
    }

    #[test]
    fn install_and_test_parsers_share_build_and_profile_policy() {
        assert_eq!(
            parse_install_args(&["--build", "--profile=release"]).expect("install options parse"),
            ParseOutcome::Parsed(InstallOptions {
                directory: PathBuf::from("."),
                build: true,
                profile: "release".to_owned(),
            })
        );
        assert_eq!(
            parse_test_args(&["project", "--profile", "debug"]).expect("test options parse"),
            ParseOutcome::Parsed(TestOptions {
                directory: PathBuf::from("project"),
                build: false,
                profile: "debug".to_owned(),
            })
        );
        assert_eq!(
            parse_install_args(&["--help"]).expect("help parses"),
            ParseOutcome::Help
        );
    }

    #[test]
    fn buildish_parser_rejects_missing_profile_and_unknown_flags() {
        assert_eq!(
            parse_test_args(&["--profile"]),
            Err(ProjectParseError::MissingValue("--profile"))
        );
        assert_eq!(
            parse_install_args(&["--wat"]),
            Err(ProjectParseError::UnknownOption("--wat".to_owned()))
        );
    }

    #[test]
    fn run_parser_preserves_repl_and_socket_options() {
        assert_eq!(
            parse_run_args(&[
                "project",
                "--build",
                "--profile=release",
                "--repl=socket",
                "--repl-bind",
                "127.0.0.1",
                "--repl-port=4400",
                "--",
                "--user-flag",
                "value",
            ])
            .expect("run options parse"),
            ParseOutcome::Parsed(RunOptions {
                directory: PathBuf::from("project"),
                build: true,
                profile: "release".to_owned(),
                repl: Some(ReplMode::Socket),
                socket_bind: Some("127.0.0.1".to_owned()),
                socket_port: Some(4400),
                arguments: vec![
                    std::ffi::OsString::from("--user-flag"),
                    std::ffi::OsString::from("value"),
                ],
            })
        );
    }

    #[test]
    fn run_parser_preserves_default_and_rejects_invalid_repl_values() {
        assert_eq!(
            parse_run_args::<&str>(&[]).expect("run defaults parse"),
            ParseOutcome::Parsed(RunOptions {
                directory: PathBuf::from("."),
                build: false,
                profile: "debug".to_owned(),
                repl: None,
                socket_bind: None,
                socket_port: None,
                arguments: Vec::new(),
            })
        );
        assert_eq!(
            parse_run_args(&["--repl=web"]),
            Err(RunParseError::InvalidValue {
                option: "--repl",
                value: "web".to_owned(),
            })
        );
        assert_eq!(
            parse_run_args(&["--repl-port=70000"]),
            Err(RunParseError::InvalidValue {
                option: "--repl-port",
                value: "70000".to_owned(),
            })
        );
    }

    #[test]
    fn tooling_parsers_preserve_exact_path_operands() {
        assert_eq!(
            parse_bake_args(&["saved.image", "standalone-birnam"]).expect("bake operands parse"),
            BakeOptions {
                image: PathBuf::from("saved.image"),
                executable: PathBuf::from("standalone-birnam"),
            }
        );
        assert_eq!(
            parse_rebuild_args(&["saved.image"]).expect("rebuild operand parses"),
            RebuildOptions {
                image: PathBuf::from("saved.image"),
            }
        );
        assert_eq!(
            parse_compile_module_args(&["module.birnam", "module.dylib"])
                .expect("compile operands parse"),
            CompileModuleOptions {
                source: PathBuf::from("module.birnam"),
                output: PathBuf::from("module.dylib"),
                initial_generation_resumable: false,
            }
        );
    }

    #[test]
    fn tooling_parsers_reject_wrong_arity_before_runtime_access() {
        assert_eq!(
            parse_bake_args::<&str>(&["only-image"]),
            Err(ToolingParseError::Arity {
                command: "--bake",
                expected: 2,
                actual: 1,
            })
        );
        assert_eq!(
            parse_rebuild_args::<&str>(&[]),
            Err(ToolingParseError::Arity {
                command: "--rebuild",
                expected: 1,
                actual: 0,
            })
        );
    }

    #[test]
    fn worker_parser_preserves_channel_and_named_dispatch_operands() {
        assert_eq!(
            parse_worker_args(&["saved.image", "Receiver", "handle:", "--channel-fd", "17",])
                .expect("worker operands parse"),
            WorkerOptions {
                image: PathBuf::from("saved.image"),
                receiver: "Receiver".to_owned(),
                selector: "handle:".to_owned(),
                channel_fd: 17,
            }
        );
        assert_eq!(
            parse_worker_args::<&str>(&["image", "receiver"]),
            Err(WorkerParseError::Arity {
                expected: 5,
                actual: 2,
            })
        );
        assert_eq!(
            parse_worker_args(&["image", "receiver", "handle:", "--wrong", "17"]),
            Err(WorkerParseError::MissingChannelFlag("--wrong".to_owned()))
        );
    }
}
