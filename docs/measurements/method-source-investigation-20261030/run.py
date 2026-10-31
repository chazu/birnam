#!/usr/bin/env python3
"""Run the traversal diagnosis against explicitly selected native libraries."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--case")
    parser.add_argument("--repetitions", type=int, default=1)
    parser.add_argument("--mode", choices=["compile:", "compileSynchronous:"], default="compileSynchronous:")
    args = parser.parse_args()
    if args.repetitions < 1:
        parser.error("repetitions must be positive")
    prefix, output = args.prefix.resolve(), args.output.resolve()
    project = output / "probe"
    (project / "src").mkdir(parents=True, exist_ok=True)
    dependencies = "".join(
        f'{name} = {{ path = {json.dumps(str(ROOT / "rust" / name))} }}\n'
        for name in ["birnam-abi", "birnam-siphon", "birnam-host-support", "birnam-codegen"])
    (project / "Cargo.toml").write_text(
        '[package]\nname="walk-probe"\nversion="0.0.0"\nedition="2024"\n'
        '[workspace]\n[dependencies]\n' + dependencies +
        'serde_json="=1.0.149"\n[profile.release]\npanic="abort"\n')
    shutil.copyfile(HERE / "probe.rs", project / "src/main.rs")
    env = {k: v for k, v in os.environ.items()
           if not k.startswith(("BIRNAM_", "WALK_")) and k != "CARGO_TARGET_DIR"}
    env.update(MISE_TRUSTED_CONFIG_PATHS=str(ROOT), WALK_MODE=args.mode,
               WALK_REPETITIONS=str(args.repetitions))
    if args.case:
        env["WALK_CASE"] = args.case
    with (output / "build.log").open("w") as log:
        subprocess.run(["mise", "exec", "--", "cargo", "build", "--release", "--offline",
                        "--manifest-path", str(project / "Cargo.toml")],
                       cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    command = [str(project / "target/release/walk-probe"), str(prefix),
               str(HERE.parent / "method-source-inventory-20261030/inventory.birnam"),
               str(HERE / "walks.birnam"), str(output / "samples.json")]
    with (output / "run.log").open("w") as log:
        subprocess.run(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    metadata = {
        "command": command, "mode": args.mode, "case": args.case,
        "repetitions": args.repetitions,
        "source_revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "library_sha256": {
            name: hashlib.sha256((prefix / "lib" / f"libbirnam_{name}.dylib").read_bytes()).hexdigest()
            for name in ["id", "root_registry", "collect", "execution", "integer", "native_backend", "codegen_runtime", "tcc"]},
        "experiment_sha256": {name: hashlib.sha256((HERE / name).read_bytes()).hexdigest()
                              for name in ["probe.rs", "walks.birnam", "instrument.py"]},
    }
    (output / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
    print(f"Diagnosis recorded in {output}")


if __name__ == "__main__":
    main()
