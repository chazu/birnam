#!/usr/bin/env python3
"""74K/74L: portable native recovery, guarded lookup, and prefix comparison."""
from measurement_archive import load_json, read_bytes

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import tempfile

from compatibility_authority import unchanged_authority

ROOT = Path(__file__).resolve().parent.parent
SIZES = (1024, 4096)
PAIRS = 7
VARIANTS = ("baseline", "slots", "candidate")
AUTHORITIES = (
    "rust/birnam-abi/src/lib.rs", "abi/idapi-v1.json",
    "abi/fixtures/idapi-v1/idapi_v1.h", "rust/birnam-image/src/lib.rs",
    "rust/birnam-image/src/v8.rs", "rust/birnam-artifact/src/frame_descriptor.rs",
    "rust/birnam-id/src/registration.rs", "rust/birnam-codegen/src/resumable_c.rs",
)


def run(command, env=None, timeout=1200):
    result = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True, timeout=timeout)
    if result.returncode:
        raise RuntimeError(f"command failed: {command}\n{result.stdout}{result.stderr}")
    return result.stdout


def sha(path):
    return hashlib.sha256(read_bytes(path)).hexdigest()


def source_manifest(root):
    paths = [root / "Cargo.toml", root / "Cargo.lock"]
    paths += [path for path in (root / "rust").rglob("*") if path.is_file()]
    for directory, suffix in (("src/id", "*.birnam"), ("modules", "*.birnam"), ("docs/guide", "*.md")):
        paths += [path for path in (root / directory).rglob(suffix) if path.is_file()]
    paths += [path for path in (root / "wrapped/linenoise/liblinenoise.dylib", root / "wrapped/termbox2/libtermbox2.dylib") if path.is_file()]
    return {str(path.relative_to(root)): sha(path) for path in sorted(paths)}


def prefix(path):
    receipt = load_json(path / "share/birnam/rust-install.json")
    assert receipt["profile"] == "release"
    assert receipt["cargo_invocation"] == ["cargo", "build", "--workspace", "--locked", "--offline", "--release"]
    floor = path / "lib/libbirnam_id.dylib"
    exports = sorted(line.split()[-1] for line in run(["nm", "-gU", str(floor)]).splitlines() if line.split())
    return {"receipt": receipt, "floor_sha256": sha(floor), "floor_bytes": floor.stat().st_size, "exports": exports}


def correctness(env):
    profiles = {}
    for profile in ("debug", "release"):
        command = ["cargo", "test", "-p", "birnam-id", "--offline", "--locked"]
        if profile == "release":
            command.append("--release")
        output = run(command + ["native_tier", "--", "--nocapture"], env)
        rows = [json.loads(line.removeprefix("NATIVE_TIER_CASE ")) for line in output.splitlines() if line.startswith("NATIVE_TIER_CASE ")]
        assert len(rows) == 24, f"expected twelve pairs, got {len(rows)}"
        assert sum(row["entries"] for row in rows if row["optimized"]) > 0
        profiles[profile] = rows
    assert profiles["debug"] == profiles["release"]
    return profiles


def scenario(binary, helper, size, env):
    output = run([str(binary), "bench/runtime-native-tier.birnam"], dict(env,
        NATIVE_TIER_ITERATIONS=str(size), BIRNAM_74J_HELPER=str(helper), BIRNAM_74J_MODE="fallback"))
    lines = output.splitlines()
    assert "tier-ready" in lines and "tier-mutation-and-eviction-ok" in lines and "74j-mode fallback" in lines
    keys = {"materialize-us", "first-us", "warmup-us", "warm-us", "fallback-us"}
    values = {key: int(value) for line in lines for key, _, value in [line.partition(" ")] if key in keys}
    assert set(values) == keys
    return {"iterations": size, "values": values, "output": output}


def comparisons(pairs):
    rows = []
    for before, after in (("baseline", "slots"), ("baseline", "candidate"), ("slots", "candidate")):
        for size, metric in [(size, metric) for size in SIZES for metric in ("warm-us", "fallback-us")] + [(100000, "control-us")]:
            samples = {variant: [next(row["values"][metric] for row in pair[variant] if row["iterations"] == size) for pair in pairs] for variant in (before, after)}
            medians = {variant: statistics.median(values) for variant, values in samples.items()}
            ranges = {variant: max(values) - min(values) for variant, values in samples.items()}
            difference = medians[before] - medians[after]
            spread = max(ranges.values())
            classification = "improvement" if difference > spread else "regression" if -difference > spread else "inconclusive"
            rows.append({"before": before, "after": after, "iterations": size, "metric": metric,
                         "samples_us": samples, "medians_us": medians, "ranges_us": ranges,
                         "difference_us": difference, "classification": classification})
    return rows


def measure(args, receipt, env):
    prefixes = {name: getattr(args, name).resolve() for name in VARIANTS}
    evidence = {name: prefix(path) for name, path in prefixes.items()}
    assert all(evidence[name]["exports"] == evidence["baseline"]["exports"] for name in VARIANTS)
    manifest_hash = hashlib.sha256(json.dumps(source_manifest(ROOT), ensure_ascii=False, separators=(",", ":"), sort_keys=True).encode()).hexdigest()
    assert evidence["candidate"]["receipt"]["source_manifest_sha256"] == manifest_hash, "candidate prefix does not match current source"
    reference = ROOT / ("docs/measurements/74k-runtime-owned-native-tier.json" if args.design == "74L" else "docs/measurements/74j-internal-guarded-sends-rejected-1.json")
    historical = load_json(reference)["prefixes"]
    for name, old in (("baseline", "baseline"), ("slots", "candidate")):
        assert evidence[name]["floor_sha256"] == historical[old]["floor_sha256"], f"{name} does not match the preserved reference"
    receipt["prefixes"] = evidence
    receipt["reference_receipt"] = {"path": str(reference.relative_to(ROOT)), "sha256": sha(reference)}
    receipt["variant_labels"] = {"baseline": "5e242e70", "slots": "74J+74K" if args.design == "74L" else "74J", "candidate": args.design}
    receipt["protocol"] = {"pairs": PAIRS, "sizes": SIZES, "variants": VARIANTS,
        "warm_runs_per_process": 5, "warmup_calls": 32,
        "criterion": "median difference exceeds both sample ranges; any above-spread candidate regression prevents promotion" if args.design == "74L" else "median difference exceeds both sample ranges; any above-spread regression prevents promotion",
        "cold_costs": "materialization, first call, and warmup reported separately",
        "scope": "portable subset plus real native custom-lookup fallback; separate from 74J's longer E1 protocol"}
    receipt["pairs"] = []
    with tempfile.TemporaryDirectory(prefix=f"birnam-{args.design.lower()}-measure-") as temporary:
        temporary = Path(temporary)
        helper = temporary / "lookup.dylib"
        run(["cc", "-dynamiclib", "-O2", "-std=c11", "-Wall", "-Wextra", "-Werror", "-I", str(ROOT / "abi/fixtures/idapi-v1"), "scripts/fixtures/internal-guarded-sends.c", "-o", str(helper)])
        receipt["lookup_helper_sha256"] = sha(helper)
        for number in range(PAIRS):
            order = list(VARIANTS[number % 3:] + VARIANTS[:number % 3])
            if number % 2:
                order.reverse()
            print(f"runtime-native-tier: host gate and pair {number + 1}/{PAIRS}", file=sys.stderr, flush=True)
            pair = {"number": number + 1, "order": order, "preflight": run(["bash", "scripts/bench-host-check.sh", "--wait", f"{args.design} pair {number + 1}"], env)}
            receipt["pairs"].append(pair)
            monitor_path = temporary / "monitor.txt"
            with monitor_path.open("w") as output:
                monitor = subprocess.Popen(["top", "-l", "0", "-s", "1", "-n", "0"], stdout=output, stderr=subprocess.DEVNULL)
                try:
                    for variant in order:
                        rows = []
                        pair[variant] = rows
                        binary = prefixes[variant] / "bin/birnam"
                        for size in SIZES:
                            print(f"runtime-native-tier: {variant} n={size}", file=sys.stderr, flush=True)
                            rows.append(scenario(binary, helper, size, env))
                        control = run([str(binary), "bench/control-throughput-probe.birnam"], env).splitlines()
                        assert len(control) == 3 and control[0] == "control-probe-ready" and control[2] == "control-probe-ok"
                        rows.append({"iterations": 100000, "values": {"control-us": int(control[1])}})
                finally:
                    monitor.terminate()
                    monitor.wait(timeout=10)
            pair["monitor"] = monitor_path.read_text()
            pair["interval_gate"] = run(["bash", "scripts/bench-host-check.sh", "--samples", str(monitor_path), f"{args.design} pair {number + 1}"], env)
        receipt["comparisons"] = comparisons(receipt["pairs"])
        classes = [row["classification"] for row in receipt["comparisons"] if args.design != "74L" or row["after"] == "candidate"]
        receipt["measurement"] = "regression-prevents-promotion" if "regression" in classes else "supports-promotion" if "improvement" in classes else "inconclusive"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--design", choices=("74K", "74L"), default="74K")
    parser.add_argument("--output", type=Path)
    for variant in VARIANTS:
        parser.add_argument(f"--{variant}", type=Path)
    args = parser.parse_args()
    if args.output is None:
        args.output = ROOT / (".cargo-target/measurements/74l-guarded-default-lookup.json" if args.design == "74L" else ".cargo-target/measurements/74k-runtime-owned-native-tier.json")
    assert all(getattr(args, name) is not None for name in VARIANTS) or all(getattr(args, name) is None for name in VARIANTS)
    env = {key: value for key, value in os.environ.items() if not key.startswith("BIRNAM_DEV_")}
    env["CARGO_TARGET_DIR"] = str(ROOT / ".cargo-target")
    authority = {}
    for name in AUTHORITIES:
        assert unchanged_authority(name, (ROOT / name).read_bytes(), subprocess.check_output(["git", "show", f"5e242e70:{name}"], cwd=ROOT))
        authority[name] = sha(ROOT / name)
    sources = source_manifest(ROOT)
    for name in ("scripts/runtime-native-tier.py", "bench/runtime-native-tier.birnam", "bench/control-throughput-probe.birnam", "scripts/fixtures/internal-guarded-sends.c", "scripts/bench-host-check.sh"):
        sources[name] = sha(ROOT / name)
    receipt = {"schema": "birnam-runtime-native-tier-v1", "design": args.design, "correctness": "pending", "measurement": "not-requested",
        "host": {"system": platform.system(), "machine": platform.machine()}, "source_sha256": sources,
        "compatibility_authorities_unchanged": authority, "profiles": {}}
    try:
        receipt["profiles"] = correctness(env)
        if args.design == "74L":
            receipt["guarded_lookup_tests"] = {}
            for profile in ("debug", "release"):
                command = ["cargo", "test", "-p", "birnam-id", "--offline", "--locked"]
                if profile == "release":
                    command.append("--release")
                output = run(command + ["guarded_lookup", "--", "--nocapture"], env)
                assert "2 passed; 0 failed" in output
                receipt["guarded_lookup_tests"][profile] = "passed"
        receipt["correctness"] = "passed"
        if args.candidate:
            measure(args, receipt, env)
    except Exception as error:
        receipt["error"] = str(error)
        if args.candidate:
            receipt["measurement"] = "incomplete-or-rejected"
        raise
    finally:
        args.output.write_text(json.dumps(receipt, indent=2) + "\n")
    print(f"runtime-native-tier: correctness {receipt['correctness']}; measurement {receipt['measurement']}", file=sys.stderr)


if __name__ == "__main__":
    main()
