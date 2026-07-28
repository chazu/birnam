use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use birnam_host::build_driver::{
    capture_zig_dependencies, execute_shadow_graph, parse_and_validate,
};
use birnam_host::instantiated_graph::{
    parse_and_validate as parse_instantiated_graph, InstantiationIdentity,
};

fn usage() {
    eprintln!(
        "usage: birnam-build dry-run --manifest FILE --root DIR\n\
         \x20      birnam-build execute-shadow --manifest FILE --root DIR\n\
         \x20      birnam-build inspect-zig --build-file FILE\n\
         \x20      birnam-build inspect-instantiated --receipt FILE --root DIR \\
         \x20          --target TARGET --profile PROFILE --floor FLOOR \\
         \x20          --distribution DISTRIBUTION \\
         \x20          --provider-selection-sha256 DIGEST \\
         \x20          --host-options-sha256 DIGEST"
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
    if command != "dry-run"
        && command != "execute-shadow"
        && command != "inspect-zig"
        && command != "inspect-instantiated"
    {
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
    if command == "inspect-instantiated" {
        let mut receipt = None;
        let mut root = None;
        let mut target = None;
        let mut profile = None;
        let mut floor = None;
        let mut distribution = None;
        let mut provider_selection_sha256 = None;
        let mut host_options_sha256 = None;
        while let Some(option) = args.next() {
            let mut value = || args.next().map(PathBuf::from);
            match option.to_string_lossy().as_ref() {
                "--receipt" => receipt = value(),
                "--root" => root = value(),
                "--target" => {
                    target = args
                        .next()
                        .map(|value| value.to_string_lossy().into_owned())
                }
                "--profile" => {
                    profile = args
                        .next()
                        .map(|value| value.to_string_lossy().into_owned())
                }
                "--floor" => {
                    floor = args
                        .next()
                        .map(|value| value.to_string_lossy().into_owned())
                }
                "--distribution" => {
                    distribution = args
                        .next()
                        .map(|value| value.to_string_lossy().into_owned())
                }
                "--provider-selection-sha256" => {
                    provider_selection_sha256 = args
                        .next()
                        .map(|value| value.to_string_lossy().into_owned())
                }
                "--host-options-sha256" => {
                    host_options_sha256 = args
                        .next()
                        .map(|value| value.to_string_lossy().into_owned())
                }
                other => return fail(format!("unknown option: {other}")),
            }
        }
        let Some(receipt_path) = receipt else {
            return fail("--receipt is required");
        };
        let Some(root) = root else {
            return fail("--root is required");
        };
        let Some(target) = target else {
            return fail("--target is required");
        };
        let Some(profile) = profile else {
            return fail("--profile is required");
        };
        let Some(floor) = floor else {
            return fail("--floor is required");
        };
        let Some(distribution) = distribution else {
            return fail("--distribution is required");
        };
        let Some(provider_selection_sha256) = provider_selection_sha256 else {
            return fail("--provider-selection-sha256 is required");
        };
        let Some(host_options_sha256) = host_options_sha256 else {
            return fail("--host-options-sha256 is required");
        };
        let bytes = match fs::read(&receipt_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                return fail(format!("cannot read {}: {error}", receipt_path.display()));
            }
        };
        let expected = InstantiationIdentity {
            target,
            profile,
            floor,
            distribution,
            provider_selection_sha256,
            host_options_sha256,
        };
        let validated = match parse_instantiated_graph(&bytes, &root, &expected) {
            Ok(receipt) => receipt,
            Err(error) => return fail(error),
        };
        println!(
            "inspect-instantiated: target={} profile={} floor={} distribution={} actions={} edges={} raw_occurrences={}",
            validated.identity.target,
            validated.identity.profile,
            validated.identity.floor,
            validated.identity.distribution,
            validated.actions.len(),
            validated.edges.len(),
            validated.raw_occurrences.len(),
        );
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
        let spec = validated.action(action).expect("validated action order");
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
