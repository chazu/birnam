#!/usr/bin/env python3
"""74J: compare ordinary installed floors with unchanged compiled Id sends."""
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
BASELINE = "5e242e70"
SIZES = (16384, 65536)
MODES = ("canonical", "fallback")
PAIRS = 7
AUTHORITIES = [
    "rust/birnam-abi/src/lib.rs", "abi/idapi-v1.json",
    "abi/fixtures/idapi-v1/idapi_v1.h", "rust/birnam-image/src/lib.rs",
    "rust/birnam-image/src/v8.rs", "rust/birnam-artifact/src/frame_descriptor.rs",
    "rust/birnam-id/src/registration.rs", "rust/birnam-codegen/src/resumable_c.rs",
]
SOURCES = [
    "rust/birnam-id/src/runtime.rs", "rust/birnam-id/src/dispatch.rs",
    "rust/birnam-id/src/execution.rs", "rust/birnam-id/src/id/vtable.rs",
    "rust/birnam-id/src/runtime/guarded_dispatch.rs",
    "rust/birnam-id/src/runtime/guarded_dispatch/tests.rs",
    "scripts/internal-guarded-sends.py", "scripts/fixtures/internal-guarded-sends.c",
    "bench/e1-slot-access.birnam", "bench/control-throughput-probe.birnam",
    "scripts/bench-host-check.sh",
]


def run(command, env=None, timeout=900):
    result = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True, timeout=timeout)
    if result.returncode:
        raise RuntimeError(f"command failed: {command}\n{result.stdout}{result.stderr}")
    return result.stdout


def sha(path):
    return hashlib.sha256(read_bytes(path)).hexdigest()


def exports(path):
    return sorted(line.split()[-1] for line in run(["nm", "-gU", str(path)]).splitlines() if line.split())


def prefix_evidence(prefix):
    receipt = load_json(prefix / "share/birnam/rust-install.json")
    if receipt["profile"] != "release":
        raise RuntimeError("74J requires ordinary release prefixes")
    floor = prefix / "lib/libbirnam_id.dylib"
    return {"receipt": receipt, "floor_sha256": sha(floor), "floor_exports": exports(floor)}


def scenario(prefix, source, directory, helper, env, mode, size):
    unit = directory / "unit.dylib"
    if unit.exists():
        unit.unlink()
    scoped = dict(env, E1_ITERATIONS=str(size), E1_UNIT_PATH=str(unit),
                  BIRNAM_74J_HELPER=str(helper), BIRNAM_74J_MODE=mode)
    output = run([str(prefix / "bin/birnam"), str(source)], scoped)
    if "e1-ready" not in output.splitlines() or f"74j-mode {mode}" not in output.splitlines():
        raise RuntimeError(f"compiled fixture or native lookup setup did not activate: {output}")
    keys = {"compile-us", "empty-us", "step-us", "warm-empty-us", "warm-step-us", "empty-answer", "checksum"}
    values = {}
    for line in output.splitlines():
        key, _, value = line.partition(" ")
        if key in keys:
            values[key] = int(value)
    if set(values) != keys or values["empty-answer"] != 7 or values["checksum"] != 7 * size + size * (size + 1) // 2:
        raise RuntimeError(f"unexpected {mode}/{size} observations: {values}")
    imports = sorted(set(run(["nm", "-u", str(unit)]).split()))
    if not any(name.endswith("birnam_send") for name in imports) or any("frame_slot_" in name or "dev_frame" in name for name in imports):
        raise RuntimeError("fixture does not use the ordinary send ABI")
    return {"mode": mode, "iterations": size, "values": values, "unit_imports": imports}


def summarize(pairs):
    result = []
    for mode, size, metric in [(mode, size, metric) for mode in MODES for size in SIZES for metric in ("warm-empty-us", "warm-step-us")] + [("control", 100000, "elapsed-us")]:
        samples = {}
        for variant in ("baseline", "candidate"):
            samples[variant] = [next(row["values"][metric] for row in pair[variant] if row["mode"] == mode and row["iterations"] == size) for pair in pairs]
        medians = {key: statistics.median(values) for key, values in samples.items()}
        ranges = {key: max(values) - min(values) for key, values in samples.items()}
        difference = medians["baseline"] - medians["candidate"]
        spread = max(ranges.values())
        classification = "improvement" if difference > spread else "regression" if -difference > spread else "inconclusive"
        result.append({"mode": mode, "iterations": size, "metric": metric, "samples_us": samples,
                       "medians_us": medians, "ranges_us": ranges, "difference_us": difference,
                       "classification": classification})
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", required=True, type=Path)
    parser.add_argument("--candidate", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    prefixes = {name: getattr(args, name).resolve() for name in ("baseline", "candidate")}
    authority = {}
    for name in AUTHORITIES:
        if not unchanged_authority(name, (ROOT / name).read_bytes(), subprocess.check_output(["git", "show", f"{BASELINE}:{name}"], cwd=ROOT)):
            raise RuntimeError(f"compatibility authority changed: {name}")
        authority[name] = sha(ROOT / name)
    receipt = {"schema": "birnam-internal-guarded-sends-v1", "design": "74J", "status": "pending",
               "host": {"system": platform.system(), "machine": platform.machine()},
               "baseline_commit": run(["git", "rev-parse", BASELINE]).strip(),
               "compatibility_authorities_unchanged": authority,
               "source_sha256": {name: sha(ROOT / name) for name in SOURCES},
               "prefixes": {name: prefix_evidence(prefix) for name, prefix in prefixes.items()},
               "protocol": {"pairs": PAIRS, "sizes": SIZES, "warm_runs_per_process": 5,
                            "warmup": "one cold empty and step call before five warm repetitions",
                            "criterion": "median difference exceeds both within-variant ranges; any above-spread regression prevents promotion"},
               "pairs": [], "comparisons": []}
    if receipt["prefixes"]["baseline"]["floor_exports"] != receipt["prefixes"]["candidate"]["floor_exports"]:
        raise RuntimeError("floor export ABI changed")
    env = {key: value for key, value in os.environ.items() if not key.startswith("BIRNAM_DEV_")}
    try:
        with tempfile.TemporaryDirectory(prefix="birnam-74j-measure-") as directory:
            directory = Path(directory)
            helper = directory / "lookup.dylib"
            run(["cc", "-dynamiclib", "-O2", "-std=c11", "-Wall", "-Wextra", "-Werror", "-I", str(ROOT / "abi/fixtures/idapi-v1"), str(ROOT / "scripts/fixtures/internal-guarded-sends.c"), "-o", str(helper)])
            source = directory / "fixture.birnam"
            original = (ROOT / "bench/e1-slot-access.birnam").read_text()
            anchor = '[System println: "e1-ready"];'
            assert original.count(anchor) == 1
            setup = '[System loadModule: ["BIRNAM_74J_HELPER" e1Getenv]];\n'
            source.write_text(original.replace(anchor, setup + anchor))
            receipt["fixture_sha256"] = sha(source)
            receipt["lookup_helper_sha256"] = sha(helper)
            receipt["cc"] = run(["cc", "--version"]).strip()
            for number in range(PAIRS):
                order = ["baseline", "candidate"] if number % 2 == 0 else ["candidate", "baseline"]
                print(f"internal-guarded-sends: host gate and pair {number + 1}/{PAIRS}", file=sys.stderr, flush=True)
                pair = {"number": number + 1, "order": order, "preflight": run(["bash", "scripts/bench-host-check.sh", "--wait", f"74J pair {number + 1}"], env)}
                receipt["pairs"].append(pair)
                monitor_path = directory / "monitor.txt"
                with monitor_path.open("w") as output:
                    monitor = subprocess.Popen(["top", "-l", "0", "-s", "1", "-n", "0"], stdout=output, stderr=subprocess.DEVNULL)
                    try:
                        for variant in order:
                            rows = []
                            pair[variant] = rows
                            for mode in MODES:
                                for size in SIZES:
                                    print(f"internal-guarded-sends: {variant} {mode} n={size}", file=sys.stderr, flush=True)
                                    rows.append(scenario(prefixes[variant], source, directory, helper, env, mode, size))
                            control = run([str(prefixes[variant] / "bin/birnam"), "bench/control-throughput-probe.birnam"], env).splitlines()
                            if len(control) != 3 or control[0] != "control-probe-ready" or control[2] != "control-probe-ok":
                                raise RuntimeError(f"ordinary-send control failed: {control}")
                            rows.append({"mode": "control", "iterations": 100000, "values": {"elapsed-us": int(control[1])}})
                    finally:
                        monitor.terminate()
                        monitor.wait(timeout=10)
                pair["monitor"] = monitor_path.read_text()
                pair["interval_gate"] = run(["bash", "scripts/bench-host-check.sh", "--samples", str(monitor_path), f"74J pair {number + 1}"], env)
            receipt["comparisons"] = summarize(receipt["pairs"])
            classifications = [row["classification"] for row in receipt["comparisons"]]
            receipt["status"] = "regression-prevents-promotion" if "regression" in classifications else "supports-promotion" if "improvement" in classifications else "inconclusive"
    except Exception as error:
        receipt["status"] = "incomplete-or-rejected"
        receipt["error"] = str(error)
        raise
    finally:
        args.output.write_text(json.dumps(receipt, indent=2) + "\n")
    print(f"internal-guarded-sends: {receipt['status']}; {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
