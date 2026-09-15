#!/usr/bin/env python3
"""Qualify live-field timings, recovery/refusal cost and retained plan/journal storage."""
from validation_support import save_json
from measurement_archive import load_json, read_bytes
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
    return hashlib.sha256(read_bytes(path)).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--receipt", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--projections", type=int, choices=(1, 3, 16), nargs="+", default=[1, 3, 16])
    parser.add_argument("--batches", type=int, default=100)
    parser.add_argument("--wait-seconds", type=int, default=120)
    args = parser.parse_args()
    if not 1 <= args.batches <= 500 or args.wait_seconds < 0:
        parser.error("batches must be 1-500 and wait-seconds nonnegative")
    args.projections = sorted(set(args.projections))
    proof = load_json(args.receipt)
    if proof["status"] != "passed" or proof["production_verification"] != "passed":
        parser.error("full verification must pass first")
    if not any(c["label"] == "release-virtual-field-driver" and c["exit_code"] == 0 for c in proof["commands"]):
        parser.error("receipt must include virtual-field correctness")
    binary = proof["binaries"]["release"]["birnam_id"]

    def check_sources():
        for name, digest in proof["sources"].items():
            if sha(ROOT / name) != digest:
                raise RuntimeError("source changed since verification: " + name)
        if sha(Path(binary["path"])) != binary["sha256"]:
            raise RuntimeError("release test binary changed since verification")

    check_sources()
    env = {k: v for k, v in os.environ.items()
           if not k.startswith(("BIRNAM_", "DYLD_", "BENCH_HOST_", "BENCH_FIELD_"))}
    env.update(BENCH_HOST_WAIT_SECONDS=str(args.wait_seconds), BENCH_HOST_RETRY_SECONDS="10",
               BENCH_FIELD_BATCHES=str(args.batches), BENCH_FIELD_PROJECTIONS=",".join(map(str, args.projections)))
    result = {"status": "running", "qualified": False, "batches": args.batches, "projections": args.projections,
              "scope": "Live clone/field bodies with selected counts of 1/3/16 field projections (minimum 3 for eligibility; shorter plans carry no live-field metadata; refusals retry within 64 invocations or immediately after an epoch change): shape 0 field return, shape 1 escaping clone, shape 2 withdrawn cached projection declaration. Payloads 16/1024 bytes; GC separately timed after 128 calls. Cold time covers first 128 calls including candidate preparation, before declaration withdrawal. Retained journal/plan/cache storage includes cached untraced scratch buffers and excludes allocator/loader bookkeeping and active nested scratch; physical bytes include heap headers. Native fixture compile and setup excluded from timed samples.",
              "correctness_receipt": str(args.receipt.resolve()),
              "correctness_receipt_sha256": sha(args.receipt), "binary": binary,
              "driver_sha256": sha(Path(__file__))}

    def save():
        save_json(args.output, result)

    def run(argv):
        completed = subprocess.run(argv, cwd=ROOT, env=env, capture_output=True, text=True)
        return {"argv": argv, "exit_code": completed.returncode,
                "output": completed.stdout + completed.stderr}

    save()
    try:
        print("virtual-field: waiting for host admission", flush=True)
        result["preflight"] = run(["bash", "scripts/bench-host-check.sh", "--wait", "virtual-field"])
        save()
        if result["preflight"]["exit_code"]:
            result["status"] = "not_qualified"
            return 2
        log = ROOT / ".cargo-target/74t-recovery/virtual-field-interval.log"
        print(f"virtual-field: measuring {6 * len(args.projections)} shapes, four alternating pairs each", flush=True)
        with log.open("w") as stream:
            monitor = subprocess.Popen(["top", "-l", "0", "-s", "1", "-n", "0", "-stats", "pid"],
                                       stdout=stream, stderr=subprocess.DEVNULL)
            try:
                result["command"] = run([binary["path"], "virtual_field_timing", "--ignored", "--nocapture", "--test-threads=1"])
            finally:
                monitor.terminate()
                monitor.wait()
        result["interval"] = run(["bash", "scripts/bench-host-check.sh", "--samples", str(log), "virtual-field"])
        result["interval"]["samples"] = log.read_text()
        rows = []
        for line in result["command"]["output"].splitlines():
            if "virtual-field shape=" not in line:
                continue
            values = dict(re.findall(r"(\w+)=(\w+)", line))
            rows.append({key: (value == "true" if key == "enabled" else int(value)) for key, value in values.items()})
        result["rows"] = rows
        if result["command"]["exit_code"] or len(rows) != 48 * len(args.projections):
            raise RuntimeError("allocation timing did not complete all pairs")
        result["shapes"] = []
        for projections in args.projections:
            for shape in (0, 1, 2):
                for payload in (16, 1024):
                    selected = [r for r in rows if r["projections"] == projections and r["shape"] == shape and r["payload"] == payload]
                    execution, including_gc, cold = [], [], []
                    for pair in range(4):
                        off = next(r for r in selected if r["pair"] == pair and not r["enabled"])
                        on = next(r for r in selected if r["pair"] == pair and r["enabled"])
                        assert on["calls"] == off["calls"] == args.batches * 128
                        assert on["allocation_bytes"] == off["allocation_bytes"]
                        assert on["scratch_misses"] == off["scratch_misses"] == 0
                        assert on["hits"] == (on["calls"] if projections >= 3 and shape != 2 else 0)
                        assert on["materialized"] == (on["calls"] if projections >= 3 and shape == 1 else 0)
                        assert on["physical_saved"] == (on["calls"] if projections >= 3 and shape == 0 else 0)
                        assert off["attempts"] == off["retry_skips"] == 0
                        assert on["attempts"] + on["retry_skips"] == (on["calls"] if projections >= 3 else 0)
                        if projections == 1:
                            assert on["metadata_bytes"] == off["metadata_bytes"] == 0
                        elif shape == 2:
                            assert on["attempts"] <= (on["calls"] + 63) // 64 + args.batches
                        else:
                            assert on["retry_skips"] == 0
                        assert off["materialized"] == 0
                        assert off["hits"] == off["physical_saved"] == off["physical_saved_bytes"] == 0
                        cold.append(100 * (1 - on["cold_ns"] / off["cold_ns"]))
                        execution.append(100 * (1 - on["ns"] / off["ns"]))
                        including_gc.append(100 * (1 - (on["ns"] + on["gc_ns"]) / (off["ns"] + off["gc_ns"])))
                    result["shapes"].append({"shape": shape, "payload": payload, "projections": projections,
                                             "paired_cold_improvement_percent": cold,
                                             "median_cold_improvement_percent": statistics.median(cold),
                                             "paired_execution_improvement_percent": execution,
                                             "median_execution_improvement_percent": statistics.median(execution),
                                             "paired_including_gc_improvement_percent": including_gc,
                                             "median_including_gc_improvement_percent": statistics.median(including_gc)})
        check_sources()
        result["qualified"] = result["interval"]["exit_code"] == 0
        result["status"] = "passed" if result["qualified"] else "not_qualified"
        print(json.dumps({k: result[k] for k in ("status", "shapes")}), flush=True)
        return 0 if result["qualified"] else 2
    except BaseException as error:
        result["status"] = "failed"
        result["error"] = str(error)
        raise
    finally:
        save()


if __name__ == "__main__":
    raise SystemExit(main())
