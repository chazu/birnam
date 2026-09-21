#!/usr/bin/env python3
"""Run Design 74F's real-driver recovery experiment and retain a JSON receipt.

This runs hand-authored native StepMethods in the Rust test harness. It does
not build/install a runtime tier or measure application performance.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys


from compatibility_authority import unchanged_authority

ROOT = Path(__file__).resolve().parent.parent
FILTER = "runtime::deopt_experiment::same_frame_deopt_experiment"
CASES = {
    "canonical", "before-entry", "mutation-restored", "custom-lookup",
    "machine-override", "replace-active", "precise-gc", "nested-entry",
    "missing-recovery-pc", "descriptor-collision", "repeated-trials",
}
# These are compatibility authorities, not a list of every source dependency.
CONTRACTS = [
    "rust/birnam-abi/src/lib.rs",
    "rust/birnam-image/src/lib.rs",
    "rust/birnam-image/src/v8.rs",
    "rust/birnam-id/src/registration.rs",
    "rust/birnam-artifact/src/frame_descriptor.rs",
]
# 74G changes the development emitter. Its ordinary output is checked against
# the committed C golden by the generated experiment; it is not an ABI authority.
# 74J permits internal runtime changes. Frozen ABI/image/descriptor authorities
# remain above; execution implementation is recorded as source evidence below.
SOURCES = [
    "rust/birnam-id/src/runtime/guarded_lookup.rs",
    "rust/birnam-id/src/runtime/guarded_lookup/tests.rs",
    "rust/birnam-id/src/runtime/native_tier.rs",
    "rust/birnam-id/src/runtime/native_tier/shims.c",
    "rust/birnam-id/src/runtime/native_tier/tests.rs",
    "rust/birnam-native-compiler/Cargo.toml", "rust/birnam-native-compiler/build.rs",
    "rust/birnam-native-compiler/src/lib.rs", "rust/birnam-native-compiler/src/unit.rs",
    "rust/birnam-id/src/execution.rs", "rust/birnam-id/src/dispatch.rs",
    "rust/birnam-id/src/id/vtable.rs", "rust/birnam-id/src/runtime/guarded_dispatch.rs",
    "rust/birnam-id/src/runtime/guarded_dispatch/tests.rs",
    "rust/birnam-id/src/runtime.rs",
    "rust/birnam-id/src/runtime/deopt_experiment.rs",
    "scripts/same-frame-deopt.py",
]


def command(args, **kwargs):
    return subprocess.run(args, cwd=ROOT, check=True, text=True,
                          capture_output=True, **kwargs).stdout.strip()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    contracts = {}
    for name in CONTRACTS:
        before = subprocess.run(["git", "show", f"HEAD:{name}"], cwd=ROOT,
                                check=True, capture_output=True).stdout
        current = (ROOT / name).read_bytes()
        if not unchanged_authority(name, current, before):
            raise RuntimeError(f"compatibility authority changed: {name}")
        contracts[name] = digest(current)
    fixture_changes = command(["git", "diff", "HEAD", "--name-only", "--", "abi/fixtures"])
    if fixture_changes:
        raise RuntimeError(f"ABI fixtures changed: {fixture_changes}")
    records = {}
    env = os.environ.copy()
    env.setdefault("CARGO_TARGET_DIR", str(ROOT / ".cargo-target"))
    for profile in ("debug", "release"):
        cmd = ["cargo", "test", "-p", "birnam-id", "--locked", "--offline"]
        if profile == "release":
            cmd.append("--release")
        cmd.extend([FILTER, "--", "--nocapture", "--test-threads=1"])
        print(f"same-frame-deopt: running {profile}", file=sys.stderr, flush=True)
        result = subprocess.run(cmd, cwd=ROOT, env=env, text=True, capture_output=True)
        if result.returncode:
            sys.stderr.write(result.stdout + result.stderr)
            raise RuntimeError(f"{profile} experiment failed ({result.returncode})")
        rows = []
        # libtest may prepend the test name when --test-threads=1 is used.
        for line in result.stdout.splitlines():
            if "DEOPT_EXPERIMENT " in line:
                rows.append(json.loads(line.split("DEOPT_EXPERIMENT ", 1)[1]))
        if len(rows) != len(CASES) or {row["case"] for row in rows} != CASES:
            raise RuntimeError(f"{profile} experiment did not report every case")
        records[profile] = rows
        print(f"same-frame-deopt: {profile} passed ({len(rows)} records)", file=sys.stderr)
    if records["debug"] != records["release"]:
        raise RuntimeError("debug and release observations differ")
    receipt = {
        "schema": "birnam-same-frame-deopt-v1",
        "design": "74F",
        "baseline_commit": command(["git", "rev-parse", "HEAD"]),
        "rustc": command(["rustc", "--version"]),
        "host": {"system": platform.system(), "machine": platform.machine()},
        "scope": "test-only static native entries; real Runtime driver; descriptor-v2",
        "result": "passed",
        "performance_claim": None,
        "compatibility_authorities_unchanged_from_head": contracts,
        "source_sha256": {name: digest((ROOT / name).read_bytes()) for name in SOURCES},
        "profiles": records,
    }
    output = json.dumps(receipt, indent=2) + "\n"
    if args.output:
        args.output.write_text(output)
        print(f"same-frame-deopt: receipt {args.output}", file=sys.stderr)
    else:
        sys.stdout.write(output)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, subprocess.CalledProcessError) as error:
        print(f"same-frame-deopt: {error}", file=sys.stderr)
        sys.exit(1)
