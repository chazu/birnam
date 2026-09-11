#!/usr/bin/env python3
"""Run Design 74G's generated-code recovery and unload experiment on macOS.

Timings include fixture instrumentation and are diagnostic, not a performance
acceptance result. No runtime installation or ABI modification is performed.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
BASELINE = "43935092"
CASES = {
    "canonical", "invalidate", "replace-active", "custom-lookup", "precise-gc",
    "descriptor-mismatch", "descriptor-collision", "invalid-pc", "duplicate-pc",
    "missing-pc", "retire-candidate-on-stack", "retire-baseline-active",
    "repeated-load-retire",
}
CONTRACTS = [
    "rust/birnam-abi/src/lib.rs", "rust/birnam-image/src/lib.rs",
    "rust/birnam-image/src/v8.rs", "rust/birnam-id/src/execution.rs",
    "rust/birnam-id/src/registration.rs", "rust/birnam-artifact/src/frame_descriptor.rs",
]
SOURCES = [
    "Cargo.lock", "rust/birnam-id/Cargo.toml", "rust/birnam-codegen/Cargo.toml",
    "rust/birnam-codegen/src/resumable_c.rs", "rust/birnam-id/src/runtime.rs",
    "rust/birnam-id/src/runtime/generated_deopt_experiment.rs",
    "rust/birnam-id/src/runtime/generated_deopt_program.rs",
    "rust/birnam-id/src/runtime/generated_deopt_adapter.c",
    "rust/birnam-id/src/runtime/generated_deopt_baseline.c",
    "scripts/generated-frame-deopt.py",
]


def command(args):
    return subprocess.run(args, cwd=ROOT, check=True, capture_output=True).stdout


def digest(data):
    return hashlib.sha256(data).hexdigest()


def records(output, prefix):
    return [json.loads(line.split(prefix, 1)[1])
            for line in output.splitlines() if prefix in line]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if platform.system() != "Darwin":
        raise RuntimeError("74G native unload probe requires macOS")
    contracts = {}
    for name in CONTRACTS:
        current = (ROOT / name).read_bytes()
        if command(["git", "show", f"{BASELINE}:{name}"]) != current:
            raise RuntimeError(f"compatibility authority changed: {name}")
        contracts[name] = digest(current)
    if command(["git", "diff", BASELINE, "--name-only", "--", "abi/fixtures"]).strip():
        raise RuntimeError("ABI fixtures changed")
    env = os.environ.copy()
    env.setdefault("CARGO_TARGET_DIR", str(ROOT / ".cargo-target"))
    profiles = {}
    for profile in ("debug", "release"):
        cmd = ["cargo", "test", "-p", "birnam-id", "--offline", "--locked"]
        if profile == "release":
            cmd.append("--release")
        cmd.extend(["generated_same_frame_deopt_experiment", "--", "--nocapture", "--test-threads=1"])
        print(f"generated-frame-deopt: running {profile}", file=sys.stderr, flush=True)
        result = subprocess.run(cmd, cwd=ROOT, env=env, text=True, capture_output=True)
        if result.returncode:
            sys.stderr.write(result.stdout + result.stderr)
            raise RuntimeError(f"{profile} experiment failed ({result.returncode})")
        cases = records(result.stdout, "GENERATED_DEOPT_EXPERIMENT ")
        shape = records(result.stdout, "GENERATED_DEOPT_SHAPE ")
        costs = records(result.stdout, "GENERATED_DEOPT_COST ")
        if len(cases) != len(CASES) or {row["case"] for row in cases} != CASES:
            raise RuntimeError(f"{profile} missing case observations")
        if len(shape) != 1 or len(costs) != 4:
            raise RuntimeError(f"{profile} missing compiler shape or cost observations")
        profiles[profile] = {"cases": cases, "shape": shape[0], "diagnostic_costs": costs}
        print(f"generated-frame-deopt: {profile} passed ({len(cases)} records)", file=sys.stderr, flush=True)
    for key in ("cases", "shape"):
        if profiles["debug"][key] != profiles["release"][key]:
            raise RuntimeError(f"debug/release {key} differ")
    receipt = {
        "schema": "birnam-generated-frame-deopt-v1", "design": "74G", "result": "passed",
        "baseline_commit": command(["git", "rev-parse", BASELINE]).decode().strip(),
        "tested_head": command(["git", "rev-parse", "HEAD"]).decode().strip(),
        "rustc": command(["rustc", "--version"]).decode().strip(),
        "cc": command(["cc", "--version"]).decode().strip(),
        "host": {"system": platform.system(), "machine": platform.machine()},
        "native_compile_flags": ["-dynamiclib", "-O2", "-Wall", "-Wextra", "-Werror", "-Wno-unused-function"],
        "scope": "canonical CodeProgram; generated per-PC entries; real Runtime driver; test-local dylib adapter",
        "performance_claim": None, "host_gates_run": False,
        "timing_scope": "single diagnostic sample per variant; includes instrumentation; no speedup qualification",
        "compatibility_authorities_unchanged_from_baseline": contracts,
        "source_sha256": {name: digest((ROOT / name).read_bytes()) for name in SOURCES},
        "profiles": profiles,
    }
    output = json.dumps(receipt, indent=2) + "\n"
    if args.output:
        args.output.write_text(output)
        print(f"generated-frame-deopt: receipt {args.output}", file=sys.stderr)
    else:
        sys.stdout.write(output)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, subprocess.CalledProcessError) as error:
        print(f"generated-frame-deopt: {error}", file=sys.stderr)
        sys.exit(1)
