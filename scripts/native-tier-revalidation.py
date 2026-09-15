#!/usr/bin/env python3
"""74N: verify dependency revalidation and repeat the unchanged 74M comparison."""
from measurement_archive import load_json

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import tempfile

from compatibility_authority import unchanged_authority

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("focus", ROOT / "scripts/native-tier-focus.py")
focus = importlib.util.module_from_spec(spec)
spec.loader.exec_module(focus)
BASE = "24e200a3"
CHANGES = (focus.NATIVE, "rust/birnam-id/src/runtime/native_tier/tests.rs")
SOURCES = (
    "scripts/native-tier-revalidation.py", "scripts/native-tier-focus.py",
    "scripts/runtime-native-tier.py", "scripts/fixtures/native-tier-profile.rs",
    "scripts/fixtures/internal-guarded-sends.c", "scripts/bench-host-check.sh",
    "bench/native-tier-focus.birnam", "bench/native-tier-steady.birnam",
    "bench/runtime-native-tier.birnam",
)


def clean_env():
    return {key: value for key, value in os.environ.items()
            if not key.startswith(("BIRNAM_", "DYLD_", "BENCH_HOST_", "NATIVE_TIER_"))}


focus.clean_env = clean_env


def prepare(args):
    # An isolated Git index makes an archiveable tree without staging or
    # committing the user's worktree. The complete patch is retained below.
    with tempfile.TemporaryDirectory(prefix="birnam-74n-index-") as temporary:
        env = dict(clean_env(), GIT_INDEX_FILE=str(Path(temporary) / "index"))
        focus.run(["git", "read-tree", BASE], env=env)
        focus.run(["git", "add", "--", *CHANGES], env=env)
        tree = focus.run(["git", "write-tree"], env=env).strip()
        patch = focus.run(["git", "diff", "--cached", "--binary", BASE], env=env)
    focus.BASE = tree
    builds = focus.prepare(args)
    builds["baseline"]["source_tree"] = builds["baseline"].pop("commit")
    builds["baseline"]["base_commit"] = focus.run(["git", "rev-parse", BASE]).strip()
    builds["baseline"]["candidate_patch"] = patch
    builds["baseline"]["candidate_patch_sha256"] = hashlib.sha256(patch.encode()).hexdigest()
    assert builds["prefixes"]["on"]["source_manifest"] == focus.reference.source_manifest(ROOT), "candidate snapshot differs from current production inputs"
    focus.save(args.work_dir / "builds.json", builds)


def correctness(args):
    env = dict(clean_env(), CARGO_TARGET_DIR=str(ROOT / ".cargo-target"))
    result = {"result": "pending", "profiles": {}, "authorities": {}}
    try:
        for name in focus.reference.AUTHORITIES:
            assert unchanged_authority(name, (ROOT / name).read_bytes(), subprocess.check_output(["git", "show", f"{BASE}:{name}"], cwd=ROOT))
            result["authorities"][name] = focus.sha(ROOT / name)
        for profile in ("debug", "release"):
            command = ["cargo", "test", "-p", "birnam-id", "--locked", "--offline"]
            if profile == "release":
                command.append("--release")
            output = focus.run(command + ["native_tier", "--", "--nocapture"], env=env)
            rows = [json.loads(line.split(" ", 1)[1]) for line in output.splitlines()
                    if line.startswith(("NATIVE_TIER_CASE ", "NATIVE_TIER_REVALIDATION_CASE "))]
            rows.sort(key=lambda row: (row["case"], row["optimized"]))
            assert len(rows) == 36 and len({row["case"] for row in rows}) == 18
            guarded = focus.run(command + ["guarded_", "--", "--nocapture"], env=env)
            assert "5 passed; 0 failed" in guarded
            result["profiles"][profile] = {"cases": rows, "native_output": output, "guarded_output": guarded}
        assert result["profiles"]["debug"]["cases"] == result["profiles"]["release"]["cases"]
        result["result"] = "passed"
    finally:
        focus.save(args.work_dir / "correctness.json", result)
    return result


def symbol_probe(args, builds):
    baseline = (ROOT / "bench/native-tier-steady.birnam").read_text()
    prefix = Path(builds["prefixes"]["profile"]["measurement_path"])
    rows = []
    with tempfile.TemporaryDirectory(prefix="birnam-74n-symbol-") as temporary:
        for name, symbol in (("existing", "oopAt:"), ("new", "notPreviouslyInterned74mProbeSymbol")):
            source = focus.replace_once(baseline, "let portable :=", f'let symbolName := "{symbol}";\nlet portable :=')
            source = focus.replace_once(source, "[samples set: #[]];\n[run value];", "[samples set: #[]];\n[symbolName asSymbol];\n[run value];")
            path = Path(temporary) / f"{name}.birnam"
            path.write_text(source)
            result = subprocess.run([prefix / "bin/birnam", path], cwd=ROOT, env=clean_env(), capture_output=True, text=True, timeout=120)
            assert result.returncode == 0 and "focus-ok steady-chain" in result.stdout, result.stdout + result.stderr
            profiles = [json.loads(line.removeprefix("NATIVE_TIER_PROFILE ")) for line in result.stderr.splitlines() if line.startswith("NATIVE_TIER_PROFILE ")]
            assert len(profiles) == 1
            rows.append({"case": name, "source": source, "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
                         "profile": profiles[0], "stdout": result.stdout, "stderr": result.stderr})
    # Same source/one interning send as 74M. Continued admission may also cover
    # its final post-measurement call, so retain the original coverage as a floor.
    assert all(row["profile"]["entries"] >= 122817 and row["profile"]["compiles"] == 1 for row in rows), rows
    assert rows[0]["profile"]["entries"] == rows[1]["profile"]["entries"]
    return {"scope": "diagnostic counts and checked answers only", "rows": rows}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work-dir", type=Path, required=True)
    parser.add_argument("--on-prefix", type=Path)
    for name in ("prepare", "correctness", "diagnose", "measure", "probe"):
        parser.add_argument(f"--{name}", action="store_true")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    args.work_dir = args.work_dir.resolve()
    args.work_dir.mkdir(parents=True, exist_ok=True)
    if args.on_prefix:
        args.on_prefix = args.on_prefix.resolve()
    assert platform.system() == "Darwin"
    assert any(getattr(args, name) for name in ("prepare", "correctness", "diagnose", "measure", "probe"))
    receipt = {"schema": "birnam-native-tier-revalidation-v1", "design": "74N", "result": "pending",
               "source_sha256": {name: focus.sha(ROOT / name) for name in SOURCES},
               "predeclared_protocol": (ROOT / "docs/design/74n-native-tier-dependency-revalidation.md").read_text(),
               "protocol": {"groups": focus.GROUPS, "scenarios": focus.SCENARIOS, "timed_repetitions": 5,
                            "criterion": "median difference exceeds both process sample ranges; setup, wall and RSS classified separately",
                            "host_gates": "unchanged 74M helper and labels", "diagnostics": "separate instrumented runs before timing"}}
    try:
        if args.correctness:
            receipt["correctness"] = correctness(args)
        if args.prepare:
            prepare(args)
        if args.prepare or args.diagnose or args.measure or args.probe:
            builds = load_json(args.work_dir / "builds.json")
            receipt["builds"] = builds
            for evidence in builds["prefixes"].values():
                assert focus.sha(Path(evidence["path"]) / "lib/libbirnam_id.dylib") == evidence["floor_sha256"]
                for name, digest in evidence["measurement_binary_sha256"].items():
                    assert focus.sha(Path(evidence["measurement_path"]) / name) == digest
            assert builds["prefixes"]["on"]["source_manifest"] == focus.reference.source_manifest(ROOT)
        if args.diagnose:
            receipt["diagnostics"] = focus.diagnose(args, builds)
        if args.probe:
            receipt["symbol_probe"] = symbol_probe(args, builds)
        if args.measure:
            focus.measure(args, builds, receipt)
        receipt["result"] = "completed"
    except Exception as error:
        receipt["result"] = "incomplete-or-rejected"
        receipt["error"] = str(error)
        raise
    finally:
        focus.save(args.output or args.work_dir / "receipt.json", receipt)
    print(f"native-tier-revalidation: {receipt['result']}", file=sys.stderr)


if __name__ == "__main__":
    main()
