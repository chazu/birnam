#!/usr/bin/env python3
"""Measure existing arithmetic paths without changing their implementations."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import statistics
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    prefix, output = args.prefix.resolve(), args.output.resolve()
    project = output / "probe"
    (project / "src").mkdir(parents=True, exist_ok=True)
    dependencies = "".join(
        f'{name} = {{ path = {json.dumps(str(ROOT / "rust" / name))} }}\n'
        for name in ["birnam-abi", "birnam-siphon", "birnam-host-support"])
    (project / "Cargo.toml").write_text(
        '[package]\nname="arithmetic-layers"\nversion="0.0.0"\nedition="2024"\n'
        '[workspace]\n[dependencies]\n' + dependencies +
        'serde_json="=1.0.149"\n[profile.release]\npanic="abort"\n')
    shutil.copyfile(HERE / "probe.rs", project / "src/main.rs")
    env = {k: v for k, v in os.environ.items()
           if not k.startswith(("BIRNAM_", "ARITHMETIC_")) and k != "CARGO_TARGET_DIR"}
    env["MISE_TRUSTED_CONFIG_PATHS"] = str(ROOT)
    manifest = ["--manifest-path", str(project / "Cargo.toml")]
    with (output / "build.log").open("w") as log:
        for command in [
            ["mise", "exec", "--", "cargo", "generate-lockfile", "--offline", *manifest],
            ["mise", "exec", "--", "cargo", "build", "--release", "--offline", "--locked", *manifest],
        ]:
            subprocess.run(command, cwd=ROOT, env=env, stdout=log,
                           stderr=subprocess.STDOUT, check=True)
    binary = project / "target/release/arithmetic-layers"
    rows, commands = [], []
    for order in ["forward", "reverse"]:
        samples = output / f"{order}.json"
        command = [str(binary), str(prefix), str(samples)]
        run_env = dict(env)
        if order == "reverse":
            run_env["ARITHMETIC_REVERSE"] = "1"
        with (output / f"{order}.log").open("w") as log:
            subprocess.run(command, cwd=ROOT, env=run_env, stdout=log,
                           stderr=subprocess.STDOUT, check=True)
        rows.extend(json.loads(samples.read_text()))
        commands.append({"argv": command, "reverse": order == "reverse"})
    summary = []
    for label in dict.fromkeys(row["case"] for row in rows):
        for automatic in [True, False]:
            samples = [s for row in rows if row["case"] == label
                       and row["automatic_gc"] == automatic for s in row["samples"]]
            summary.append({
                "case": label, "automatic_gc": automatic, "samples": len(samples),
                "median_us": statistics.median(s["us"] for s in samples),
                "allocations_per_call": sorted({s["allocations"] for s in samples}),
                "id_bytes_per_call": sorted({s["id_bytes"] for s in samples}),
            })
    receipt = {
        "source_revision": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "prefix": str(prefix), "commands": commands, "summary": summary,
        "probe_sha256": digest(HERE / "probe.rs"), "runner_sha256": digest(HERE / "run.py"),
        "binary_sha256": digest(binary), "lock_sha256": digest(project / "Cargo.lock"),
        "libraries_sha256": {p.name: digest(p) for p in sorted((prefix / "lib").iterdir())
                             if p.is_file() and p.suffix in [".dylib", ".image"]},
        "raw_sha256": {name: digest(output / name) for name in
                       ["forward.json", "reverse.json", "forward.log", "reverse.log", "build.log"]},
    }
    (output / "result.json").write_text(json.dumps(receipt, indent=2) + "\n")
    for row in summary:
        if row["automatic_gc"]:
            print(f'{row["case"]}: {row["median_us"]:.3f} us, '
                  f'{row["allocations_per_call"]} managed allocations')


if __name__ == "__main__":
    main()
