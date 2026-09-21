#!/usr/bin/env python3
"""Design 74I: test guard substitutes through the unchanged IdApi contract."""
from measurement_archive import read_bytes

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import tempfile

from compatibility_authority import unchanged_authority

ROOT = Path(__file__).resolve().parent.parent
BASELINE = "5e242e70"
CLIENT = "scripts/fixtures/deopt-public-api.c"
HEADER = "abi/fixtures/idapi-v1/idapi_v1.h"
AUTHORITIES = [
    "rust/birnam-abi/src/lib.rs", "abi/idapi-v1.json", HEADER,
    "rust/birnam-image/src/lib.rs", "rust/birnam-image/src/v8.rs",
    "rust/birnam-runtime/src/registration.rs",
    "rust/birnam-artifact/src/frame_descriptor.rs",
]
CASES = {
    "ordinary_floor", "parent_change", "shadow_addition", "method_replacement",
    "observer_replaced", "reflective_guard", "metatable_lookup", "gc_observation",
}


def run(command, env=None):
    result = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(f"command failed: {command}\n{result.stdout}{result.stderr}")
    return result.stdout


def digest(path):
    return hashlib.sha256(read_bytes(path)).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if platform.system() != "Darwin":
        raise RuntimeError("74I currently tests the supported macOS floor")
    authorities = {}
    for name in AUTHORITIES:
        current = (ROOT / name).read_bytes()
        baseline = subprocess.check_output(["git", "show", f"{BASELINE}:{name}"], cwd=ROOT)
        if not unchanged_authority(name, current, baseline):
            raise RuntimeError(f"compatibility authority changed: {name}")
        authorities[name] = digest(ROOT / name)
    if run(["git", "diff", BASELINE, "--name-only", "--", "abi/fixtures"]).strip():
        raise RuntimeError("ABI fixtures changed")
    sources = {"Cargo.toml", "Cargo.lock", CLIENT, "scripts/deopt-public-api.py"}
    # Runtime internals can now depend on the native compiler and canonical
    # emitter. Record the complete Rust source closure for current reruns.
    sources.update(str(path.relative_to(ROOT)) for path in (ROOT / "rust").rglob("*") if path.is_file())
    receipt = {
        "schema": "birnam-deopt-public-api-v1", "design": "74I",
        "baseline_commit": run(["git", "rev-parse", BASELINE]).strip(),
        "host": {"system": platform.system(), "machine": platform.machine()},
        "rustc": run(["rustc", "--version"]).strip(),
        "cc": run(["cc", "--version"]).strip(),
        "compatibility_authorities_unchanged": authorities,
        "source_sha256": {name: digest(ROOT / name) for name in sorted(sources)},
        "scope": "external frozen-IdApi C client; ordinary floor; guard-boundary regression without a timing claim",
        "runs": [],
    }
    expected = None
    with tempfile.TemporaryDirectory(prefix="birnam-74i-") as directory:
        directory = Path(directory)
        env = dict(os.environ, CARGO_TARGET_DIR=str(directory / "target"))
        for profile in ("debug", "release"):
            print(f"deopt-public-api: ordinary {profile} floor", file=sys.stderr, flush=True)
            command = ["cargo", "build", "-p", "birnam-id", "--lib", "--no-default-features", "--offline", "--locked"]
            if profile == "release":
                command.append("--release")
            run(command, env)
            floor = directory / "target" / profile / "libbirnam_id.dylib"
            exports = sorted(line.split()[-1] for line in run(["nm", "-gU", str(floor)]).splitlines() if line.split())
            for optimization in ("-O0", "-O2"):
                executable = directory / f"probe-{profile}{optimization}"
                compile_command = ["cc", "-std=c11", optimization, "-Wall", "-Wextra", "-Werror", "-I", str((ROOT / HEADER).parent), str(ROOT / CLIENT), "-o", str(executable)]
                run(compile_command)
                for repetition in range(3):
                    observations = [json.loads(line) for line in run([str(executable), str(floor)]).splitlines()]
                    if len(observations) != len(CASES) or {row["case"] for row in observations} != CASES:
                        raise RuntimeError("missing or duplicate boundary observations")
                    if expected is None:
                        expected = observations
                    if observations != expected:
                        raise RuntimeError("profile, optimization or repetition changed observations")
                    receipt["runs"].append({
                        "floor_profile": profile, "c_optimization": optimization,
                        "repetition": repetition + 1, "floor_sha256": digest(floor),
                        "client_sha256": digest(executable), "floor_exports": exports,
                        "observations": observations,
                    })
    receipt["status"] = "boundary-cases-confirmed"
    output = json.dumps(receipt, indent=2) + "\n"
    if args.output:
        args.output.write_text(output)
        print(f"deopt-public-api: 12 fresh processes agree; receipt {args.output}", file=sys.stderr)
    else:
        print(output, end="")


if __name__ == "__main__":
    main()
