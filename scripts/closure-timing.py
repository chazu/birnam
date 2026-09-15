#!/usr/bin/env python3
"""Measure real source closure execution; qualification uses unchanged host gates."""
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
from closure_timing_common import source, parse_output, check_pair

ROOT = Path(__file__).resolve().parent.parent


def sha(path):
    return hashlib.sha256(read_bytes(Path(path))).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--receipt", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--batches", type=int, default=500)
    parser.add_argument("--pairs", type=int, default=4)
    parser.add_argument("--uses", type=int, nargs="+", choices=range(1, 33), default=[1, 3, 4, 8, 16, 32])
    parser.add_argument("--wait-seconds", type=int, default=120)
    parser.add_argument("--numeric-body", action="store_true")
    parser.add_argument("--exploratory", action="store_true", help="retain unqualified diagnostic timings when host admission fails")
    parser.add_argument("--deadline", type=int, default=900, help="Total measurement/build budget in seconds")
    parser.add_argument("--timeout", type=int, default=300, help="Per-command timeout in seconds")
    args = parser.parse_args()
    if args.deadline <= 0 or args.timeout <= 0: parser.error("deadlines must be positive")
    args.uses = list(dict.fromkeys(args.uses))
    if not 1 <= args.batches <= 500 or not 1 <= args.pairs <= 7 or args.wait_seconds < 0:
        parser.error("invalid batches, pairs or wait interval")
    proof = load_proof(args.receipt, measurement=('numeric-measure-release' if args.numeric_body else 'measure-release'))
    if proof["status"] != "passed" or (proof.get("production_verification") != "passed" and not proof.get("phase_verified")):
        parser.error("required correctness phase must pass first")
    output = proof["commands"][-1]["output"]
    if "closure-timing-smoke:" not in output:
        parser.error("receipt must include measurement-counter correctness")
    measurement = proof.get("closure_source_matrices", {}).get(("numeric-" if args.numeric_body else "") + "measure-release.json")
    if not measurement or measurement["status"] != "passed":
        parser.error("receipt must retain the release measurement matrix")
    for uses in args.uses:
        digest = hashlib.sha256(source(uses, numeric=args.numeric_body).encode()).hexdigest()
        rows = [r for r in measurement["rows"] if r["uses"] == uses and r["source_sha256"] == digest]
        if len(rows) != 8 or {(r["shape"], r["enabled"]) for r in rows} != set(product(range(4), (0, 1))):
            parser.error("requested repeated-use source lacks counter verification")
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
    workspace = tempfile.TemporaryDirectory(prefix="birnam-closure-timing-")
    work = Path(workspace.name)
    prefix = work / "prefix"
    fixtures = {uses: work / f"uses-{uses}.birnam" for uses in args.uses}
    for uses, fixture in fixtures.items():
        fixture.write_text(source(uses, numeric=args.numeric_body))
    result = {"status": "running", "qualified": False, "numeric_body": args.numeric_body, "exploratory_requested": args.exploratory,
              "measurement_sources": measurement_sources,
              "correctness_receipt": str(args.receipt.resolve()), "correctness_sha256": sha(args.receipt),
              "floor": {"path": str(floor), "sha256": sha(floor)}, "prefix": str(prefix),
              "batches": args.batches, "pairs": args.pairs, "uses": args.uses, "rows": [],
              "fixture_sha256": {str(uses): sha(path) for uses, path in fixtures.items()},
              "scope": "Instrumented test floor with real release installed Codegen and source factories. Each factory invokes one closure uses times (1 through 32); completed_calls counts optimized invocations. Shapes: 0 captured Cell return with eliminated closure, 1 escaping closure, 2 warmed then replaced callee, 3 observing callee refused by compiler. Precise GC; 128 factory calls then separately timed collection per batch. Id allocation charges must match; physical_saved counts deferred Block allocations before collection. Materialization timing excludes source parsing and CodeObject creation; cold_ns covers first 128 factory calls. Floor preparation time is a subset of materialization. Metadata is process-wide inline record/plan plus used element payload, excluding Vec spare capacity, BTree nodes, Rc and allocator/loader costs. Journal bytes include retained capacity and shared snapshots. No RSS or application-wide claim. Probe counters and per-factory result checks are inside measured execution; process wall time is separately retained."}

    def save():
        save_json(args.output, result)

    run = Commands(ROOT, env, deadline=args.deadline, timeout=args.timeout)

    save()
    try:
        print("closure-timing: building a fresh release installation", flush=True)
        result["install"] = run(["mise", "exec", "--", "bash", "scripts/rust-install.sh", "install",
                                 "--root", ROOT, "--prefix", prefix, "--profile", "release"])
        save()
        if result["install"]["exit_code"]:
            raise RuntimeError("fresh installation failed")
        binaries = [prefix / "bin/birnam", prefix / "share/birnam/rust-install.json"] + sorted((prefix / "lib").glob("*.dylib"))
        result["installed_sha256"] = {str(p): sha(p) for p in binaries}
        check_sources()
        print("closure-timing: checking host admission", flush=True)
        result["preflight"] = run(["bash", "scripts/bench-host-check.sh", "--wait", "closure-timing"])
        save()
        if result["preflight"]["exit_code"] and not args.exploratory:
            result["status"] = "not_qualified"
            return 2
        monitor_log = work / "interval.log"
        with host_monitor(monitor_log):
            for uses, shape in product(args.uses, range(4)):
                for pair in range(args.pairs):
                    rows = {}
                    for enabled in ([0, 1] if pair % 2 == 0 else [1, 0]):
                        print(f"closure-timing: uses={uses} shape={shape} pair={pair} enabled={enabled}", flush=True)
                        command = run([prefix / "bin/birnam", fixtures[uses]],
                            {"BIRNAM_ID_DYLIB": str(floor), "BIRNAM_TEST_CLOSURE_CONTROL": "1",
                             "BIRNAM_DEV_CLOSURE_ELISION": str(enabled), "BIRNAM_CLOSURE_SHAPE": str(shape),
                             "BIRNAM_CLOSURE_BATCHES": str(args.batches), "BIRNAM_CLOSURE_USES": str(uses)})
                        if command["exit_code"]:
                            result["failed_command"] = command
                            raise RuntimeError("measurement process failed")
                        row = dict(parse_output(command["output"]), shape=shape, uses=uses, pair=pair,
                                   enabled=bool(enabled), process_wall_ns=command["wall_ns"])
                        assert row["measure"]["uses"] == uses and row["measure"]["shape"] == shape
                        result["rows"].append(row)
                        rows[enabled] = row
                        save()
                    check_pair(rows[0], rows[1], args.batches, numeric=args.numeric_body)
        result["interval"] = run(["bash", "scripts/bench-host-check.sh", "--samples", monitor_log, "closure-timing"])
        result["interval"]["samples"] = monitor_log.read_text()
        result["shapes"] = []
        for uses, shape in product(args.uses, range(4)):
            summary = {"uses": uses, "shape": shape}
            for metric in ("ns", "including_gc_ns", "cold_ns", "materialization_ns", "process_wall_ns"):
                def value(row):
                    if metric == "including_gc_ns": return row["measure"]["ns"] + row["measure"]["gc_ns"]
                    if metric == "materialization_ns": return row["prepare"]["ns"]
                    if metric == "process_wall_ns": return row[metric]
                    return row["measure"][metric]
                changes = []
                for pair in range(args.pairs):
                    off = next(r for r in result["rows"] if r["uses"] == uses and r["shape"] == shape and r["pair"] == pair and not r["enabled"])
                    on = next(r for r in result["rows"] if r["uses"] == uses and r["shape"] == shape and r["pair"] == pair and r["enabled"])
                    changes.append(100 * (1 - value(on) / value(off)))
                summary[metric] = {"paired_improvement_percent": changes, "median_improvement_percent": statistics.median(changes)}
            result["shapes"].append(summary)
        check_sources()
        assert all(sha(p) == h for p, h in result["installed_sha256"].items())
        assert all(sha(path) == result["fixture_sha256"][str(uses)] for uses, path in fixtures.items())
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
