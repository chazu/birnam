use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use birnam_host::build_driver::parse_and_validate;

fn usage() {
    eprintln!("usage: birnam-build dry-run --manifest FILE --root DIR");
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
    if command != "dry-run" {
        return fail(format!("unknown command: {}", command.to_string_lossy()));
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
    println!(
        "dry-run: target={} profile={} distribution={} actions={}",
        validated.manifest.target,
        validated.manifest.profile,
        validated.manifest.selection.distribution,
        validated.action_order.len()
    );
    for (index, action) in validated.action_order.iter().enumerate() {
        let spec = validated
            .manifest
            .actions
            .iter()
            .find(|candidate| candidate.name == *action)
            .expect("validated action order");
        println!(
            "action[{index}]: {} tool={} argv={}",
            spec.name,
            spec.tool,
            spec.command.join(" ")
        );
    }
    ExitCode::SUCCESS
}
