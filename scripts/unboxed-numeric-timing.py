#!/usr/bin/env python3
"""Measure real source unboxed arithmetic; qualification uses unchanged host gates."""
from validation_proof import load_proof
from measurement_archive import load_json, read_bytes
import argparse
import hashlib
from itertools import product
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
import tempfile
import time
from benchmark_support import Commands, source_checker, host_monitor, save_json, clean_environment
from unboxed_numeric_timing_common import source, parse_output, check_pair, peak_rss

ROOT = Path(__file__).resolve().parent.parent


def sha(path):
    return hashlib.sha256(read_bytes(Path(path))).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--receipt", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--batches", type=int, default=500)
    parser.add_argument("--pairs", type=int, default=4)
    parser.add_argument("--operations", type=int, nargs="+", choices=(3, 16, 32), default=[3, 16, 32])
    parser.add_argument("--wait-seconds", type=int, default=120)
    parser.add_argument("--exploratory", action="store_true", help="retain unqualified diagnostic timings when host admission fails")
    parser.add_argument("--deadline", type=int, default=900, help="Total measurement/build budget in seconds")
    parser.add_argument("--timeout", type=int, default=300, help="Per-command timeout in seconds")
    args = parser.parse_args()
    if args.deadline <= 0 or args.timeout <= 0: parser.error("deadlines must be positive")
    args.operations = list(dict.fromkeys(args.operations))
    if not 1 <= args.batches <= 500 or not 1 <= args.pairs <= 7 or args.wait_seconds < 0:
        parser.error("invalid batches, pairs or wait interval")
    proof = load_proof(args.receipt, measurement='unboxed-measure-release')
    if proof["status"] != "passed" or (proof.get("production_verification") != "passed" and not proof.get("phase_verified")):
        parser.error("required correctness phase must pass first")
    output = proof["commands"][-1]["output"]
    if "unboxed-measurement-smoke:" not in output:
        parser.error("receipt must include measurement-counter correctness")
    measurement = proof.get("unboxed_source_matrices", {}).get("unboxed-measure-release.json")
    if not measurement or measurement["status"] != "passed":
        parser.error("receipt must retain the release measurement matrix")
    for operations in args.operations:
        digest = hashlib.sha256(source(operations).encode()).hexdigest()
        rows = [r for r in measurement["rows"] if r["operations"] == operations and r["source_sha256"] == digest]
        if {(r["shape"], r["enabled"]) for r in rows} != set(product(range(5), (0, 1))):
            parser.error("requested numeric source lacks counter verification")
    floor = ROOT / ".cargo-target/74t-closure-control-target/release/libbirnam_id.dylib"
    digests = ({measurement["floor_sha256"]} if proof.get("phase_verified") else
               set(re.findall(r"profile=release .*?floor_sha256=([a-f0-9]{64})", output)))
    if digests != {sha(floor)} or measurement["floor_sha256"] != sha(floor):
        parser.error("instrumented floor differs from verification")

    measurement_sources = dict(proof['sources'])
    for path in (Path(__file__).resolve(), ROOT/'scripts/benchmark_support.py', ROOT/'scripts/validation_proof.py'):
        measurement_sources[str(path.relative_to(ROOT))] = sha(path)
    check_sources = source_checker(ROOT, measurement_sources, {floor: sha(floor)})

    check_sources()
    env = clean_environment()
    env.update(BENCH_HOST_WAIT_SECONDS=str(args.wait_seconds), BENCH_HOST_RETRY_SECONDS="10")
    workspace = tempfile.TemporaryDirectory(prefix="birnam-unboxed-numeric-timing-")
    work = Path(workspace.name)
    prefix = work / "prefix"
    fixtures = {operations: work / f"operations-{operations}.birnam" for operations in args.operations}
    for operations, fixture in fixtures.items():
        fixture.write_text(source(operations))
    result = {"status": "running", "qualified": False, "exploratory_requested": args.exploratory,
              "measurement_sources": measurement_sources,
              "correctness_receipt": str(args.receipt.resolve()), "correctness_sha256": sha(args.receipt),
              "floor": {"path": str(floor), "sha256": sha(floor)}, "prefix": str(prefix),
              "batches": args.batches, "pairs": args.pairs, "operations": args.operations, "rows": [],
              "fixture_sha256": {str(operations): sha(path) for operations, path in fixtures.items()},
              "scope": "Instrumented test floor with fresh release installed Codegen. Shapes: 0 tail numeric chain, 1 warmed Integer operator replaced then restored, 2 mixed Float input, 3 compiler-refused Cell construction, 4 first numeric result returned after the chain. Precise GC; 128 calls then separately timed collection per batch. Id allocation charges match. Saved boxes are before collection. Cold time includes the first 128 calls and hot-kernel preparation; Codegen materialization excludes source parsing and CodeObject creation. Map bytes include owned vector capacity and Rc header, excluding allocator overhead and other native-tier records. Native unit counts are not executable byte sizes. Journal bytes include retained capacity. Darwin time -l records whole-process peak RSS in bytes, including startup and GC. Counter and result checks remain inside execution timing. These are fixture measurements, not application-wide speedups."}

    def save():
        save_json(args.output, result)

    run = Commands(ROOT, env, deadline=args.deadline, timeout=args.timeout)

    save()
    try:
        print("unboxed-numeric-timing: building a fresh release installation", flush=True)
        result["install"] = run(["mise", "exec", "--", "bash", "scripts/rust-install.sh", "install",
                                 "--root", ROOT, "--prefix", prefix, "--profile", "release"])
        save()
        if result["install"]["exit_code"]:
            raise RuntimeError("fresh installation failed")
        binaries = [prefix / "bin/birnam", prefix / "share/birnam/rust-install.json"] + sorted((prefix / "lib").glob("*.dylib"))
        result["installed_sha256"] = {str(p): sha(p) for p in binaries}
        check_sources()
        print("unboxed-numeric-timing: checking host admission", flush=True)
        result["preflight"] = run(["bash", "scripts/bench-host-check.sh", "--wait", "unboxed-numeric-timing"])
        save()
        if result["preflight"]["exit_code"] and not args.exploratory:
            result["status"] = "not_qualified"
            return 2
        monitor_log = work / "interval.log"
        with host_monitor(monitor_log):
            for operations, shape in product(args.operations, range(5)):
                for pair in range(args.pairs):
                    rows = {}
                    for enabled in ([0, 1] if pair % 2 == 0 else [1, 0]):
                        print(f"unboxed-numeric-timing: operations={operations} shape={shape} pair={pair} enabled={enabled}", flush=True)
                        command = run(["/usr/bin/time", "-l", prefix / "bin/birnam", fixtures[operations]],
                            {"BIRNAM_ID_DYLIB": str(floor), "BIRNAM_TEST_UNBOXED_NUMERIC": "1",
                             "BIRNAM_DEV_UNBOXED_NUMERIC": str(enabled), "BIRNAM_NUMERIC_SHAPE": str(shape),
                             "BIRNAM_NUMERIC_BATCHES": str(args.batches)})
                        if command["exit_code"]:
                            result["failed_command"] = command
                            raise RuntimeError("measurement process failed")
                        row = dict(parse_output(command["output"]), shape=shape, operations=operations, pair=pair,
                                   enabled=bool(enabled), process_wall_ns=command["wall_ns"],
                                   peak_rss_bytes=peak_rss(command["output"]), output=command["output"])
                        assert row["measure"]["operations"] == operations and row["measure"]["shape"] == shape
                        result["rows"].append(row)
                        rows[enabled] = row
                        save()
                    check_pair(rows[0], rows[1], args.batches)
        result["interval"] = run(["bash", "scripts/bench-host-check.sh", "--samples", monitor_log, "unboxed-numeric-timing"])
        result["interval"]["samples"] = monitor_log.read_text()
        result["shapes"] = []
        for operations, shape in product(args.operations, range(5)):
            summary = {"operations": operations, "shape": shape}
            for metric in ("ns", "including_gc_ns", "cold_ns", "materialization_ns", "process_wall_ns", "peak_rss_bytes"):
                def value(row):
                    if metric == "including_gc_ns": return row["measure"]["ns"] + row["measure"]["gc_ns"]
                    if metric == "materialization_ns": return row["prepare"]["ns"]
                    if metric in ("process_wall_ns", "peak_rss_bytes"): return row[metric]
                    return row["measure"][metric]
                changes = []
                for pair in range(args.pairs):
                    off = next(r for r in result["rows"] if r["operations"] == operations and r["shape"] == shape and r["pair"] == pair and not r["enabled"])
                    on = next(r for r in result["rows"] if r["operations"] == operations and r["shape"] == shape and r["pair"] == pair and r["enabled"])
                    changes.append(100 * (1 - value(on) / value(off)))
                summary[metric] = {"paired_improvement_percent": changes, "median_improvement_percent": statistics.median(changes)}
            result["shapes"].append(summary)
        check_sources()
        assert all(sha(p) == h for p, h in result["installed_sha256"].items())
        assert all(sha(path) == result["fixture_sha256"][str(operations)] for operations, path in fixtures.items())
        result["qualified"] = result["preflight"]["exit_code"] == result["interval"]["exit_code"] == 0
        result["status"] = "passed" if result["qualified"] else "not_qualified"
        print(json.dumps({"status": result["status"], "shapes": result["shapes"]}), flush=True)
        return 0 if result["qualified"] or args.exploratory else 2
    except BaseException as error:
        result["status"] = "failed"
        result["error"] = str(error)
        raise
    finally:
        try: save()
        finally: workspace.cleanup()


if __name__ == "__main__":
    raise SystemExit(main())
