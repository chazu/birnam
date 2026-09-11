#!/usr/bin/env python3
"""Design 74H correctness and optional controlled release measurements."""

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

ROOT = Path(__file__).resolve().parent.parent
BASELINE = "a10d1b55"
TEST = "runtime::generated_deopt_experiment::read_reuse::frame_read_reuse_experiment"
BENCH = "runtime::generated_deopt_experiment::read_reuse::frame_read_reuse_benchmark"
CONTRACTS = [
    "rust/birnam-abi/src/lib.rs", "rust/birnam-image/src/lib.rs",
    "rust/birnam-image/src/v8.rs", "rust/birnam-id/src/execution.rs",
    "rust/birnam-id/src/registration.rs", "rust/birnam-artifact/src/frame_descriptor.rs",
]
SOURCES = [
    "rust/birnam-codegen/Cargo.toml", "rust/birnam-codegen/src/resumable_c.rs",
    "rust/birnam-id/Cargo.toml", "rust/birnam-id/src/runtime.rs",
    "rust/birnam-id/src/runtime/generated_deopt_experiment.rs",
    "rust/birnam-id/src/runtime/generated_deopt_adapter.c",
    "rust/birnam-id/src/runtime/read_reuse_experiment.rs", "scripts/frame-read-reuse.py",
]


def run(args, env=None):
    result = subprocess.run(args, cwd=ROOT, env=env, capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(f"command failed ({result.returncode}): {' '.join(map(str, args))}\n{result.stdout}{result.stderr}")
    return result.stdout


def rows(output, prefix):
    return [json.loads(line.split(prefix, 1)[1]) for line in output.splitlines() if prefix in line]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def measure(env):
    built = run(["cargo", "test", "-p", "birnam-id", "--release", "--no-run", "--offline", "--locked", "--message-format=json"], env)
    artifacts = [json.loads(line) for line in built.splitlines() if line.startswith("{")]
    executable = next(row["executable"] for row in artifacts if row.get("reason") == "compiler-artifact" and row.get("executable") and row["profile"]["test"])
    measurement = {"status": "pending", "pairs": [], "classifications": [], "iterations": 4096, "warmup": 64}
    with tempfile.TemporaryDirectory(prefix="birnam-74h-measure-") as directory:
        directory = Path(directory)
        prepared = directory / "native"
        preparation = run([executable, TEST, "--exact", "--nocapture", "--test-threads=1"], dict(env, BIRNAM_74H_PREPARE=str(prepared)))
        measurement["preparation_costs"] = rows(preparation, "READ_REUSE_COST ")
        measurement["prepared_sha256"] = {p.name: sha(p.read_bytes()) for p in sorted(prepared.iterdir())}
        try:
            for pair in range(7):
                order = ["control", "reuse"] if pair % 2 == 0 else ["reuse", "control"]
                print(f"frame-read-reuse: host gate and pair {pair + 1}/7 ({', '.join(order)})", file=sys.stderr, flush=True)
                gate = run(["bash", "scripts/bench-host-check.sh", "--wait", f"74H pair {pair + 1}"], env)
                item = {"pair": pair + 1, "order": order, "host_preflight": gate.strip(), "runs": []}
                measurement["pairs"].append(item)
                monitor_path = directory / f"host-{pair}.txt"
                with monitor_path.open("w") as output:
                    monitor = subprocess.Popen(["top", "-l", "0", "-s", "1", "-n", "0", "-stats", "pid"], stdout=output, stderr=subprocess.DEVNULL)
                    try:
                        for mode in order:
                            result = run([executable, BENCH, "--exact", "--ignored", "--nocapture", "--test-threads=1"], dict(env, BIRNAM_74H_ARTIFACTS=str(prepared), BIRNAM_74H_MODE=mode))
                            samples = rows(result, "READ_REUSE_TIME ")
                            expected = {(n, s) for n in (0, 2, 4, 16) for s in ("ordinary", "invalidate-between-reads", "before-entry")}
                            if len(samples) != 12 or {(r["repeats"], r["scenario"]) for r in samples} != expected:
                                raise RuntimeError("incomplete timing observations")
                            for row in samples:
                                if row["mode"] != mode or row["iterations"] != 4096 or row["warmup"] != 64:
                                    raise RuntimeError("unexpected timing protocol")
                                expected_optimized = 0 if row["scenario"] == "before-entry" else 8192 if row["scenario"] == "invalidate-between-reads" and row["repeats"] else 12288
                                if row["optimized_entries"] != expected_optimized:
                                    raise RuntimeError(f"candidate selection did not remain active as expected: {row}")
                            item["runs"].append({"mode": mode, "samples": samples})
                    finally:
                        monitor.terminate()
                        monitor.wait(timeout=10)
                item["host_samples"] = monitor_path.read_text()
                item["host_interval_gate"] = run(["bash", "scripts/bench-host-check.sh", "--samples", str(monitor_path), f"74H pair {pair + 1}"], env).strip()
        except RuntimeError as error:
            measurement["status"] = "unqualified"
            measurement["failure"] = str(error)
            return measurement
    for repeats in (0, 2, 4, 16):
        for scenario in ("ordinary", "invalidate-between-reads", "before-entry"):
            samples = {mode: [row["elapsed_ns"] / row["iterations"] for pair in measurement["pairs"] for variant in pair["runs"] if variant["mode"] == mode
                             for row in variant["samples"] if row["repeats"] == repeats and row["scenario"] == scenario] for mode in ("control", "reuse")}
            medians = {mode: statistics.median(values) for mode, values in samples.items()}
            ranges = {mode: max(values) - min(values) for mode, values in samples.items()}
            improvement = medians["control"] - medians["reuse"]
            spread = max(ranges.values())
            verdict = "improvement" if improvement > spread else "regression" if -improvement > spread else "inconclusive"
            if repeats == 0 and verdict != "inconclusive":
                verdict = "control-shift"
            measurement["classifications"].append({"repeats": repeats, "scenario": scenario, "samples_ns_per_call": samples,
                "medians_ns_per_call": medians, "ranges_ns_per_call": ranges, "improvement_ns_per_call": improvement,
                "improvement_percent": 100 * improvement / medians["control"], "verdict": verdict})
    verdicts = measurement["classifications"]
    has_gain = any(row["verdict"] == "improvement" and row["repeats"] > 0 and row["scenario"] == "ordinary" for row in verdicts)
    has_regression = any(row["verdict"] in ("regression", "control-shift") for row in verdicts)
    measurement["status"] = "supports-integration" if has_gain and not has_regression else "does-not-support-integration"
    return measurement


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--measure", action="store_true")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if platform.system() != "Darwin":
        raise RuntimeError("74H native experiment requires macOS")
    env = os.environ.copy()
    env.setdefault("CARGO_TARGET_DIR", str(ROOT / ".cargo-target"))
    for name in ("BIRNAM_DEV_GUARDED_SLOTS", "BIRNAM_DEV_ELIDE_SLOT_BOX", "BIRNAM_74H_PREPARE", "BIRNAM_74H_ARTIFACTS", "BIRNAM_74H_MODE"):
        env.pop(name, None)
    contracts = {}
    for name in CONTRACTS:
        current = (ROOT / name).read_bytes()
        baseline = subprocess.check_output(["git", "show", f"{BASELINE}:{name}"], cwd=ROOT)
        if current != baseline:
            raise RuntimeError(f"compatibility authority changed: {name}")
        contracts[name] = sha(current)
    if run(["git", "diff", BASELINE, "--name-only", "--", "abi/fixtures"]).strip():
        raise RuntimeError("ABI fixtures changed")
    profiles = {}
    for profile in ("debug", "release"):
        print(f"frame-read-reuse: correctness {profile}", file=sys.stderr, flush=True)
        cmd = ["cargo", "test", "-p", "birnam-id", "--locked", "--offline"]
        if profile == "release":
            cmd.append("--release")
        result = run(cmd + [TEST, "--", "--exact", "--nocapture", "--test-threads=1"], env)
        cases = rows(result, "READ_REUSE_CASE ")
        repeated = rows(result, "READ_REUSE_REPEAT ")
        costs = rows(result, "READ_REUSE_COST ")
        if len(cases) != 36 or len(repeated) != 5 or len(costs) != 12:
            raise RuntimeError(f"{profile} missing observations")
        profiles[profile] = {"cases": cases, "repeated": repeated, "costs": costs}
    for key in ("cases", "repeated"):
        if profiles["debug"][key] != profiles["release"][key]:
            raise RuntimeError(f"debug/release {key} differ")
    receipt = {"schema": "birnam-frame-read-reuse-v1", "design": "74H", "correctness": "passed",
        "baseline_commit": run(["git", "rev-parse", BASELINE]).strip(), "rustc": run(["rustc", "--version"]).strip(), "cc": run(["cc", "--version"]).strip(),
        "host": {"system": platform.system(), "machine": platform.machine()}, "profiles": profiles,
        "compatibility_authorities_unchanged": contracts, "source_sha256": {name: sha((ROOT / name).read_bytes()) for name in SOURCES},
        "scope": "test-local generated PC entries; repeated argument staging; counters compiled out of timed reads; no production performance claim",
        "measurement": measure(env) if args.measure else {"status": "not-requested"}}
    output = json.dumps(receipt, indent=2) + "\n"
    if args.output:
        args.output.write_text(output)
        print(f"frame-read-reuse: receipt {args.output}; {receipt['measurement']['status']}", file=sys.stderr)
    else:
        sys.stdout.write(output)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, subprocess.CalledProcessError) as error:
        print(f"frame-read-reuse: {error}", file=sys.stderr)
        sys.exit(1)
