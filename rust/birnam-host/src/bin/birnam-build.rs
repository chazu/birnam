use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use birnam_host::build_driver::{
    capture_zig_dependencies, execute_shadow_graph, parse_and_validate,
};

fn usage() {
    eprintln!(
        "usage: birnam-build dry-run --manifest FILE --root DIR\n\
         \x20      birnam-build execute-shadow --manifest FILE --root DIR\n\
         \x20      birnam-build inspect-zig --build-file FILE"
    );
}

fn fail(message: impl std::fmt::Display) -> ExitCode {
    eprintln!("birnam-build: {message}");
    usage();
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let mut args = env::args_os().skip(1);
    let Some(command) = args.next() else {
        return fail("missing command");
    };
    if command != "dry-run" && command != "execute-shadow" && command != "inspect-zig" {
        return fail(format!("unknown command: {}", command.to_string_lossy()));
    }
    if command == "inspect-zig" {
        let Some(option) = args.next() else {
            return fail("--build-file is required");
        };
        if option != "--build-file" {
            return fail(format!("unknown option: {}", option.to_string_lossy()));
        }
        let Some(path) = args.next() else {
            return fail("--build-file is required");
        };
        if args.next().is_some() {
            return fail("inspect-zig accepts only --build-file");
        }
        let path = PathBuf::from(path);
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) => return fail(format!("cannot read {}: {error}", path.display())),
        };
        let capture = match capture_zig_dependencies(&source) {
            Ok(capture) => capture,
            Err(error) => return fail(error),
        };
        println!("dependOn-occurrences: {}", capture.occurrence_count);
        for edge in capture.edges {
            println!(
                "line {}: {} -> {}",
                edge.source_line, edge.source, edge.dependency
            );
        }
        return ExitCode::SUCCESS;
    }
    let mut manifest = None;
    let mut root = None;
    while let Some(option) = args.next() {
        match option.to_string_lossy().as_ref() {
            "--manifest" => manifest = args.next().map(PathBuf::from),
            "--root" => root = args.next().map(PathBuf::from),
            other => return fail(format!("unknown option: {other}")),
        }
    }
    let Some(manifest) = manifest else {
        return fail("--manifest is required");
    };
    let Some(root) = root else {
        return fail("--root is required");
    };
    let bytes = match fs::read(&manifest) {
        Ok(bytes) => bytes,
        Err(error) => return fail(format!("cannot read {}: {error}", manifest.display())),
    };
    let validated = match parse_and_validate(&bytes, &root) {
        Ok(validated) => validated,
        Err(error) => return fail(error),
    };
    if command == "execute-shadow" {
        let executions = match execute_shadow_graph(&validated) {
            Ok(executions) => executions,
            Err(error) => return fail(error),
        };
        println!("execute-shadow: actions={}", executions.len());
        for (index, execution) in executions.iter().enumerate() {
            println!(
                "action[{index}]: name={} status={} tool={} tool_sha256={} cwd={} argv={:?} inputs={:?} outputs={:?} output_digests={:?} stdout={:?} stderr={:?}",
                execution.action,
                execution.status,
                execution.tool.display(),
                execution.tool_sha256,
                execution.cwd.display(),
                execution.argv,
                execution.inputs,
                execution.outputs,
                execution.output_digests,
                execution.stdout,
                execution.stderr
            );
        }
        return ExitCode::SUCCESS;
    }
    println!(
        "dry-run: target={} profile={} distribution={} actions={}",
        validated.target(),
        validated.profile(),
        validated.distribution(),
        validated.action_order().len()
    );
    for (index, action) in validated.action_order().iter().enumerate() {
        let spec = validated
            .action(action)
            .expect("validated action order");
        println!(
            "action[{index}]: name={} context={} authority={} delegation={} tool={} tool_sha256={} cwd={} argv={:?} depends_on={:?} inputs={:?} outputs={:?} environment={:?}",
            spec.name,
            spec.context,
            spec.authority,
            spec.delegation,
            spec.tool,
            spec.tool_sha256,
            spec.cwd.display(),
            spec.command,
            spec.depends_on,
            spec.inputs,
            spec.outputs,
            spec.environment,
        );
    }
    ExitCode::SUCCESS
}
