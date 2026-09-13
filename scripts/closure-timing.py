#!/usr/bin/env python3
"""Measure real source closure execution; qualification uses unchanged host gates."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
import tempfile
import time
from closure_timing_common import parse_output, check_pair

ROOT = Path(__file__).resolve().parent.parent


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--receipt", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--batches", type=int, default=500)
    parser.add_argument("--pairs", type=int, default=4)
    parser.add_argument("--wait-seconds", type=int, default=120)
    parser.add_argument("--exploratory", action="store_true", help="retain unqualified diagnostic timings when host admission fails")
    args = parser.parse_args()
    if not 1 <= args.batches <= 500 or not 1 <= args.pairs <= 7 or args.wait_seconds < 0:
        parser.error("invalid batches, pairs or wait interval")
    proof = json.loads(args.receipt.read_text())
    if proof["status"] != "passed" or proof.get("production_verification") != "passed":
        parser.error("full correctness verification must pass first")
    output = proof["commands"][-1]["output"]
    if "closure-timing-smoke:" not in output:
        parser.error("receipt must include measurement-counter correctness")
    floor = ROOT / ".cargo-target/74t-closure-control-target/release/libbirnam_id.dylib"
    digests = set(re.findall(r"profile=release .*?floor_sha256=([a-f0-9]{64})", output))
    if digests != {sha(floor)}:
        parser.error("instrumented floor differs from verification")

    def check_sources():
        for name, digest in proof["sources"].items():
            if sha(ROOT / name) != digest:
                raise RuntimeError("source changed since verification: " + name)
        assert sha(floor) in digests

    check_sources()
    env = {k: v for k, v in os.environ.items() if not k.startswith(("BIRNAM_", "DYLD_", "BENCH_HOST_"))}
    env.update(BENCH_HOST_WAIT_SECONDS=str(args.wait_seconds), BENCH_HOST_RETRY_SECONDS="10")
    work = Path(tempfile.mkdtemp(prefix="birnam-closure-timing-"))
    prefix = work / "prefix"
    result = {"status": "running", "qualified": False, "exploratory_requested": args.exploratory,
              "correctness_receipt": str(args.receipt.resolve()), "correctness_sha256": sha(args.receipt),
              "floor": {"path": str(floor), "sha256": sha(floor)}, "prefix": str(prefix),
              "batches": args.batches, "pairs": args.pairs, "rows": [],
              "scope": "Instrumented test floor with real release installed Codegen and source factories. Shapes: 0 captured Cell return with eliminated closure, 1 escaping closure, 2 warmed then replaced callee, 3 observing callee refused by compiler. Precise GC; 128 calls then separately timed collection per batch. Id allocation charges must match; physical_saved counts deferred Block allocations before collection. Materialization timing excludes source parsing and CodeObject creation; cold_ns covers first 128 calls. Floor preparation time is a subset of materialization. Metadata is process-wide inline record/plan plus used element payload, excluding Vec spare capacity, BTree nodes, Rc and allocator/loader costs. Journal bytes include retained capacity and shared snapshots. No RSS or application-wide claim. Probe counters and per-call result checks are inside measured execution; process wall time is separately retained."}

    def save():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")

    def run(argv, extra=None):
        start = time.monotonic_ns()
        p = subprocess.run([str(a) for a in argv], cwd=ROOT, env=dict(env, **(extra or {})),
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        return {"argv": [str(a) for a in argv], "exit_code": p.returncode,
                "wall_ns": time.monotonic_ns() - start, "output": p.stdout + p.stderr}

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
        with monitor_log.open("w") as stream:
            monitor = subprocess.Popen(["top", "-l", "0", "-s", "1", "-n", "0", "-stats", "pid"],
                                       stdout=stream, stderr=subprocess.DEVNULL)
            try:
                for shape in range(4):
                    for pair in range(args.pairs):
                        rows = {}
                        for enabled in ([0, 1] if pair % 2 == 0 else [1, 0]):
                            print(f"closure-timing: shape={shape} pair={pair} enabled={enabled}", flush=True)
                            command = run([prefix / "bin/birnam", "scripts/fixtures/closure-timing.birnam"],
                                {"BIRNAM_ID_DYLIB": str(floor), "BIRNAM_TEST_CLOSURE_CONTROL": "1",
                                 "BIRNAM_DEV_CLOSURE_ELISION": str(enabled), "BIRNAM_CLOSURE_SHAPE": str(shape),
                                 "BIRNAM_CLOSURE_BATCHES": str(args.batches)})
                            if command["exit_code"]:
                                result["failed_command"] = command
                                raise RuntimeError("measurement process failed")
                            row = dict(parse_output(command["output"]), shape=shape, pair=pair,
                                       enabled=bool(enabled), process_wall_ns=command["wall_ns"])
                            result["rows"].append(row)
                            rows[enabled] = row
                            save()
                        check_pair(rows[0], rows[1], args.batches)
            finally:
                monitor.terminate()
                monitor.wait()
        result["interval"] = run(["bash", "scripts/bench-host-check.sh", "--samples", monitor_log, "closure-timing"])
        result["interval"]["samples"] = monitor_log.read_text()
        result["shapes"] = []
        for shape in range(4):
            summary = {"shape": shape}
            for metric in ("ns", "including_gc_ns", "cold_ns", "materialization_ns", "process_wall_ns"):
                def value(row):
                    if metric == "including_gc_ns": return row["measure"]["ns"] + row["measure"]["gc_ns"]
                    if metric == "materialization_ns": return row["prepare"]["ns"]
                    if metric == "process_wall_ns": return row[metric]
                    return row["measure"][metric]
                changes = []
                for pair in range(args.pairs):
                    off = next(r for r in result["rows"] if r["shape"] == shape and r["pair"] == pair and not r["enabled"])
                    on = next(r for r in result["rows"] if r["shape"] == shape and r["pair"] == pair and r["enabled"])
                    changes.append(100 * (1 - value(on) / value(off)))
                summary[metric] = {"paired_improvement_percent": changes, "median_improvement_percent": statistics.median(changes)}
            result["shapes"].append(summary)
        check_sources()
        assert all(sha(p) == h for p, h in result["installed_sha256"].items())
        result["qualified"] = result["preflight"]["exit_code"] == result["interval"]["exit_code"] == 0
        result["status"] = "passed" if result["qualified"] else "not_qualified"
        print(json.dumps({"status": result["status"], "shapes": result["shapes"]}), flush=True)
        return 0 if result["qualified"] or args.exploratory else 2
    except BaseException as error:
        result["status"] = "failed"
        result["error"] = str(error)
        raise
    finally:
        save()


if __name__ == "__main__":
    raise SystemExit(main())
