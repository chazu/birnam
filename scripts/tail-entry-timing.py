#!/usr/bin/env python3
"""Qualify paired registered tail-loop timings against a verified source receipt."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import statistics
import subprocess

ROOT = Path(__file__).resolve().parent.parent


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--receipt", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--batches", type=int, default=100)
    parser.add_argument("--wait-seconds", type=int, default=120)
    args = parser.parse_args()
    if not 1 <= args.batches <= 500 or args.wait_seconds < 0:
        parser.error("batches must be 1-500 and wait-seconds nonnegative")
    proof = json.loads(args.receipt.read_text())
    if proof["status"] != "passed" or proof["production_verification"] != "passed":
        parser.error("full verification must pass first")
    if not any(c["label"] == "release-tail-entry-loop" and c["exit_code"] == 0 for c in proof["commands"]):
        parser.error("receipt must include tail-loop correctness")
    binary = proof["binaries"]["release"]["birnam_id"]

    def check_sources():
        for name, digest in proof["sources"].items():
            if sha(ROOT / name) != digest:
                raise RuntimeError("source changed since verification: " + name)
        if sha(Path(binary["path"])) != binary["sha256"]:
            raise RuntimeError("release test binary changed since verification")

    check_sources()
    env = {k: v for k, v in os.environ.items()
           if not k.startswith(("BIRNAM_", "DYLD_", "BENCH_HOST_", "BENCH_TAIL_"))}
    env.update(BENCH_HOST_WAIT_SECONDS=str(args.wait_seconds), BENCH_HOST_RETRY_SECONDS="10",
               BENCH_TAIL_BATCHES=str(args.batches))
    result = {"status": "running", "qualified": False, "batches": args.batches,
              "scope": "Registered tail-recursive loop, ordinary callback each iteration; collection outside 1000-transition batches.",
              "correctness_receipt": str(args.receipt.resolve()),
              "correctness_receipt_sha256": sha(args.receipt), "binary": binary,
              "driver_sha256": sha(Path(__file__))}

    def save():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")

    def run(argv):
        completed = subprocess.run(argv, cwd=ROOT, env=env, capture_output=True, text=True)
        return {"argv": argv, "exit_code": completed.returncode,
                "output": completed.stdout + completed.stderr}

    save()
    try:
        print("tail-entry: waiting for host admission", flush=True)
        result["preflight"] = run(["bash", "scripts/bench-host-check.sh", "--wait", "tail-entry"])
        save()
        if result["preflight"]["exit_code"]:
            result["status"] = "not_qualified"
            return 2
        log = ROOT / ".cargo-target/74t-recovery/tail-entry-interval.log"
        print("tail-entry: measuring four alternating pairs", flush=True)
        with log.open("w") as stream:
            monitor = subprocess.Popen(["top", "-l", "0", "-s", "1", "-n", "0", "-stats", "pid"],
                                       stdout=stream, stderr=subprocess.DEVNULL)
            try:
                result["command"] = run([binary["path"], "tail_loop_entry_timing", "--ignored", "--nocapture", "--test-threads=1"])
            finally:
                monitor.terminate()
                monitor.wait()
        result["interval"] = run(["bash", "scripts/bench-host-check.sh", "--samples", str(log), "tail-entry"])
        result["interval"]["samples"] = log.read_text()
        rows = []
        for pair, enabled, transitions, ns, allocation, hits, cold, metadata in re.findall(
                r"tail-entry pair=(\d+) enabled=(true|false) transitions=(\d+) ns=(\d+) allocation_bytes=(\d+) hits=(\d+) cold_ns=(\d+) metadata_bytes=(\d+)",
                result["command"]["output"]):
            rows.append(dict(pair=int(pair), enabled=enabled == "true", transitions=int(transitions),
                             ns=int(ns), allocation_bytes=int(allocation), hits=int(hits),
                             cold_ns=int(cold), metadata_bytes=int(metadata)))
        result["rows"] = rows
        if result["command"]["exit_code"] or len(rows) != 8:
            raise RuntimeError("loop timing did not complete all pairs")
        ratios = []
        for pair in range(4):
            off = next(row for row in rows if row["pair"] == pair and not row["enabled"])
            on = next(row for row in rows if row["pair"] == pair and row["enabled"])
            assert on["transitions"] == off["transitions"] == args.batches * 1000
            assert on["allocation_bytes"] == off["allocation_bytes"]
            assert on["hits"] == on["transitions"] and off["hits"] == 0
            ratios.append(100 * (1 - on["ns"] / off["ns"]))
        result["paired_improvement_percent"] = ratios
        result["median_improvement_percent"] = statistics.median(ratios)
        check_sources()
        result["qualified"] = result["interval"]["exit_code"] == 0
        result["status"] = "passed" if result["qualified"] else "not_qualified"
        print(json.dumps({k: result[k] for k in ("status", "median_improvement_percent", "paired_improvement_percent")}), flush=True)
        return 0 if result["qualified"] else 2
    except BaseException as error:
        result["status"] = "failed"
        result["error"] = str(error)
        raise
    finally:
        save()


if __name__ == "__main__":
    raise SystemExit(main())
