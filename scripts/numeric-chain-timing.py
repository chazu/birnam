#!/usr/bin/env python3
"""Host-gated numeric-chain arithmetic intervals; collection is outside each batch."""
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
    parser.add_argument("--receipt", required=True, type=Path,
                        help="passed recovery-graphs.py --chains --verify receipt")
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--batches", type=int, default=50)
    parser.add_argument("--wait-seconds", type=int, default=120)
    parser.add_argument("--fused", action="store_true", help="measure whole-driver fusion instead of per-call prediction")
    args = parser.parse_args()
    if not 1 <= args.batches <= 500 or args.wait_seconds < 0:
        parser.error("batches must be 1-500 and wait-seconds nonnegative")
    proof = json.loads(args.receipt.read_text())
    if proof["status"] != "passed" or proof["production_verification"] != "passed":
        parser.error("correctness and production verification must pass first")
    for name, digest in proof["sources"].items():
        if sha(ROOT / name) != digest:
            parser.error("source changed since verification: " + name)
    binary = proof["binaries"]["release"]["birnam_id"]
    if sha(Path(binary["path"])) != binary["sha256"]:
        parser.error("release test binary changed since verification")
    env = {key: value for key, value in os.environ.items()
           if not key.startswith(("BIRNAM_", "DYLD_", "BENCH_NUMERIC_", "BENCH_HOST_"))}
    env.update(BENCH_HOST_WAIT_SECONDS=str(args.wait_seconds),
               BENCH_HOST_RETRY_SECONDS="10", BENCH_NUMERIC_BATCHES=str(args.batches))
    if args.fused:
        env["BENCH_NUMERIC_FUSED"] = "1"
    result = {
        "status": "running", "qualified": False,
        "mode": "fused" if args.fused else "per-call-prediction",
        "scope": "Paired warm arithmetic intervals with collection between batches; application and cold-start acceptance remain separate.",
        "correctness_receipt": str(args.receipt.resolve()),
        "correctness_receipt_sha256": sha(args.receipt),
        "driver_sha256": sha(Path(__file__)), "binary": binary,
        "batches": args.batches, "cases": [],
    }

    def save():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")

    def run(argv, additions=None):
        completed = subprocess.run([str(arg) for arg in argv], cwd=ROOT,
                                   env=dict(env, **(additions or {})), text=True, capture_output=True)
        return {"argv": [str(arg) for arg in argv], "environment": additions or {},
                "exit_code": completed.returncode,
                "output": completed.stdout + completed.stderr}

    save()
    try:
        for operator, varying in (("add", "0"), ("add", "1"), ("divide", "0"), ("divide", "1")):
            label = operator + "-" + varying
            case = {"operator": operator, "varying": varying == "1", "qualified": False}
            result["cases"].append(case)
            print("numeric-chain: waiting for host admission: " + label, flush=True)
            case["preflight"] = run(["bash", "scripts/bench-host-check.sh", "--wait", label])
            save()
            if case["preflight"]["exit_code"]:
                result["status"] = "not_qualified"
                return 2
            work = ROOT / ".cargo-target/74t-recovery"
            work.mkdir(parents=True, exist_ok=True)
            log = work / ("chain-interval-" + label + ".log")
            print("numeric-chain: measuring " + label, flush=True)
            with log.open("w") as stream:
                monitor = subprocess.Popen(["top", "-l", "0", "-s", "1", "-n", "0", "-stats", "pid"],
                                           stdout=stream, stderr=subprocess.DEVNULL)
                try:
                    case["command"] = run([binary["path"], "production_numeric_chain_timing",
                                           "--ignored", "--nocapture", "--test-threads=1"],
                                          {"BENCH_NUMERIC_OPERATOR": operator, "BENCH_NUMERIC_VARYING": varying})
                finally:
                    monitor.terminate()
                    monitor.wait()
            case["interval"] = run(["bash", "scripts/bench-host-check.sh", "--samples", log, label])
            case["interval"]["samples"] = log.read_text()
            rows = []
            for length, pair, enabled, calls, ns, allocated, hits in re.findall(
                r"numeric-chain length=(\d+) pair=(\d+) enabled=(true|false) calls=(\d+) ns=(\d+) allocation_bytes=(\d+) hits=(\d+)",
                case["command"]["output"],
            ):
                rows.append({"length": int(length), "pair": int(pair), "enabled": enabled == "true",
                             "calls": int(calls), "ns": int(ns), "allocation_bytes": int(allocated), "hits": int(hits)})
            case["rows"] = rows
            save()
            if case["command"]["exit_code"] or len(rows) != 16:
                raise RuntimeError("timing test did not finish its complete paired cases")
            if case["interval"]["exit_code"]:
                result["status"] = "not_qualified"
                return 2
            case["summary"] = []
            for length in (3, 16):
                ratios = []
                for pair in range(4):
                    off = next(row for row in rows if row["length"] == length and row["pair"] == pair and not row["enabled"])
                    on = next(row for row in rows if row["length"] == length and row["pair"] == pair and row["enabled"])
                    assert off["calls"] == on["calls"] == args.batches * 2000
                    assert off["allocation_bytes"] == on["allocation_bytes"]
                    assert off["hits"] == 0 and on["hits"] == length * on["calls"]
                    ratios.append(100 * (1 - on["ns"] / off["ns"]))
                case["summary"].append({"length": length, "median_improvement_percent": statistics.median(ratios),
                                        "paired_improvement_percent": ratios})
            case["qualified"] = True
            print(json.dumps(case["summary"]), flush=True)
            save()
        for name, digest in proof["sources"].items():
            if sha(ROOT / name) != digest:
                raise RuntimeError("source changed during timing: " + name)
        if sha(Path(binary["path"])) != binary["sha256"]:
            raise RuntimeError("binary changed during timing")
        result["status"] = "passed"
        result["qualified"] = True
        return 0
    except BaseException as error:
        result["status"] = "failed"
        result["error"] = str(error)
        raise
    finally:
        save()


if __name__ == "__main__":
    raise SystemExit(main())
