#!/usr/bin/env python3
"""74P production recovery correctness and isolated projection-call timings."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
from compatibility_authority import baseline_path, unchanged_authority

ROOT = Path(__file__).resolve().parent.parent
BASE = "ecb1262a"
AUTHORITIES = ("rust/birnam-abi/src/lib.rs", "abi/idapi-v1.json", "abi/fixtures/idapi-v1/idapi_v1.h",
               "rust/birnam-artifact/src/frame_descriptor.rs", "rust/birnam-runtime/src/registration.rs",
               "rust/birnam-codegen/src/resumable_c.rs", "rust/birnam-image/src/lib.rs", "rust/birnam-image/src/v8.rs")
PREFIXES = ("PROJECTION_INLINE ", "PRODUCTION_RECOVERY ", "PROJECTION_BOUNDARY ")


def sha(data):
    return hashlib.sha256(data).hexdigest()


def sources():
    paths = [ROOT / "Cargo.toml", ROOT / "Cargo.lock", Path(__file__).resolve(), ROOT / "scripts/compatibility_authority.py", ROOT / "scripts/rust-driver-native-smoke.sh", ROOT / "scripts/fixtures/projection-recovery.birnam"]
    for folder in ("rust", "src/id", "modules"):
        paths.extend(p for p in (ROOT / folder).rglob("*") if p.is_file())
    return {str(p.relative_to(ROOT)): sha(p.read_bytes()) for p in sorted(paths)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--measure", action="store_true")
    parser.add_argument("--verify", action="store_true")
    parser.add_argument("--pairs", type=int, default=7)
    parser.add_argument("--count", type=int, default=10000)
    parser.add_argument("--work-dir", type=Path, default=ROOT / ".cargo-target/74p-recovery")
    parser.add_argument("--output", type=Path, default=ROOT / ".cargo-target/measurements/74p-production-recovery.json")
    args = parser.parse_args()
    args.work_dir.mkdir(parents=True, exist_ok=True)
    env = {k:v for k,v in os.environ.items() if not k.startswith(("BIRNAM_", "DYLD_"))}
    env.update(CARGO_TARGET_DIR=str(ROOT / ".cargo-target"), MISE_TRUSTED_CONFIG_PATHS=str(ROOT))
    result = {"design":"74P", "status":"running", "sources":sources(), "commands":[], "authorities":{}, "profiles":{},
              "scope":"Production one-child recovery and entry-guarded projection inline; general inlining is not implemented"}

    def run(argv, label, additions=None):
        argv = [str(a) for a in argv]
        print("74P: " + label, flush=True)
        completed = subprocess.run(argv, cwd=ROOT, env=dict(env, **(additions or {})), text=True, capture_output=True)
        log = completed.stdout + completed.stderr
        (args.work_dir / (label + ".log")).write_text(log)
        result["commands"].append({"label":label,"argv":argv,"exit_code":completed.returncode,"environment_additions":additions or {},"output":log})
        if completed.returncode:
            raise RuntimeError(label + " failed; see " + str(args.work_dir / (label + ".log")))
        return completed.stdout

    try:
        for name in AUTHORITIES:
            data = (ROOT / name).read_bytes()
            assert unchanged_authority(name, data, subprocess.check_output(["git","show",f"{BASE}:{baseline_path(name)}"],cwd=ROOT)), name
            result["authorities"][name] = sha(data)
        for profile in ("debug", "release"):
            output = run(["cargo","test","-p","birnam-runtime","--locked","--offline"] + (["--release"] if profile == "release" else []) + ["runtime::native_tier::leaf_tests::production_","--","--nocapture"],profile)
            rows = [{"kind":prefix.strip(),"data":json.loads(line[len(prefix):])} for line in output.splitlines() for prefix in PREFIXES if line.startswith(prefix)]
            assert len(rows) == 10 and "3 passed; 0 failed" in output
            result["profiles"][profile] = sorted(rows, key=lambda row:json.dumps(row,sort_keys=True))
        assert result["profiles"]["debug"] == result["profiles"]["release"]
        if args.measure:
            output = run(["cargo","test","-p","birnam-runtime","--release","--locked","--offline","--no-run","--message-format=json"],"timing-build")
            binary = next(row["executable"] for line in output.splitlines() if (row:=json.loads(line)).get("reason")=="compiler-artifact" and row.get("executable") and row["profile"]["test"])
            result["test_binary_sha256"] = sha(Path(binary).read_bytes())
            samples = []
            for pair in range(args.pairs):
                run(["bash","scripts/bench-host-check.sh",f"74P-pair-{pair}"], f"host-{pair}")
                monitor_path = args.work_dir / f"host-interval-{pair}.log"
                with monitor_path.open("w") as log:
                    monitor = subprocess.Popen(["top","-l","0","-s","1","-n","0","-stats","pid"],stdout=log,stderr=subprocess.DEVNULL)
                    try:
                        for case in ("projection","fallback","control"):
                            for mode in (("off","on") if pair%2 == 0 else ("on","off")):
                                output = run([binary,"runtime::native_tier::leaf_tests::projection_timing","--exact","--ignored","--nocapture"],f"pair-{pair}-{case}-{mode}", {"BIRNAM_74P_MODE":mode,"BIRNAM_74P_CASE":case,"BIRNAM_74P_COUNT":str(args.count)})
                                sample = next(json.loads(line.removeprefix("PROJECTION_TIMING ")) for line in output.splitlines() if line.startswith("PROJECTION_TIMING "))
                                samples.append({"pair":pair,**sample})
                    finally:
                        monitor.terminate(); monitor.wait()
                run(["bash","scripts/bench-host-check.sh","--samples",monitor_path,f"74P-pair-{pair}"],f"interval-{pair}")
            result["timing_samples"] = samples
            summaries = {}
            for case in ("projection","fallback","control"):
                off = [s for s in samples if s["case"]==case and not s["optimized"]]
                on = [s for s in samples if s["case"]==case and s["optimized"]]
                assert {s["bytes"] for s in off} == {s["bytes"] for s in on}
                ratios = [(b["elapsed_ns"]-a["elapsed_ns"])/b["elapsed_ns"]*100 for b,a in zip(off,on)]
                summaries[case] = {"median_improvement_percent":statistics.median(ratios),"paired_improvement_percent":ratios,
                    "baseline_ns":[s["elapsed_ns"] for s in off],"optimized_ns":[s["elapsed_ns"] for s in on],
                    "baseline_warm_ns":[s["warm_ns"] for s in off],"optimized_warm_ns":[s["warm_ns"] for s in on],
                    "allocation_bytes":on[0]["bytes"]}
            result["timing_summary"] = summaries
        if args.verify:
            run(["mise","run","verify"],"full-verification")
            result["full_verification"] = "passed"
        assert sources() == result["sources"], "sources changed during the run"
        result["status"] = "passed"
    except Exception as error:
        result["status"] = "failed"
        result["error"] = str(error)
        raise
    finally:
        args.output.parent.mkdir(parents=True,exist_ok=True)
        args.output.write_text(json.dumps(result,indent=2)+"\n")


if __name__ == "__main__":
    main()
