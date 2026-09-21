#!/usr/bin/env python3
"""74R correctness, isolated alias timing and production verification receipts."""
from measurement_archive import load_json
import argparse
import hashlib
import importlib.util
import itertools
import json
import os
from pathlib import Path
import signal
import statistics
import subprocess
import datetime
from compatibility_authority import baseline_path, unchanged_authority

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("production_recovery", ROOT / "scripts/production-recovery.py")
previous = importlib.util.module_from_spec(spec)
spec.loader.exec_module(previous)
BASE = "d15a40ce"
PREFIXES = previous.PREFIXES + ("SMALL_CALLEE ", "SMALL_CALLEE_GC ", "SMALL_CALLEE_REFUSAL ")


def sha(data):
    return hashlib.sha256(data).hexdigest()


def sources():
    result = previous.sources()
    for name in ("scripts/small-callee.py", "scripts/fixtures/small-callee-recovery.birnam", "scripts/bench-host-check.sh"):
        result[name] = sha((ROOT / name).read_bytes())
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--measure", action="store_true")
    parser.add_argument("--unqualified", action="store_true", help="explicitly allow exploratory timing without the host gate")
    parser.add_argument("--verify", action="store_true")
    parser.add_argument("--pairs", type=int, default=7)
    parser.add_argument("--count", type=int, default=10000)
    parser.add_argument("--pause-rk-pid", type=int, help="requires prior user authorization; only the verified rk executable may be paused")
    parser.add_argument("--reference-binary", type=Path, help="optional 74P binary, verified against its recorded hash")
    parser.add_argument("--output", type=Path, default=ROOT / ".cargo-target/measurements/74r-small-callee.json")
    parser.add_argument("--work-dir", type=Path, default=ROOT / ".cargo-target/74r/run")
    args = parser.parse_args()
    assert args.pairs > 0 and args.count > 0
    args.work_dir.mkdir(parents=True, exist_ok=True)
    env = {k:v for k,v in os.environ.items() if not k.startswith(("BIRNAM_", "DYLD_"))}
    env.update(CARGO_TARGET_DIR=str(ROOT / ".cargo-target"), MISE_TRUSTED_CONFIG_PATHS=str(ROOT))
    result = {"design":"74R", "status":"running", "timestamp_utc":datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "sources":sources(), "commands":[], "profiles":{}, "authorities":{}, "baseline_commit":BASE,
              "timing_qualification":"unqualified exploratory timing; host gate explicitly bypassed" if args.unqualified else "not measured",
              "comparison":"74p mode uses prior eligibility in current binary, including current preparation; optional reference runs use actual recorded 74P test binary"}

    def run(argv, label, additions=None, timeout=None):
        argv = [str(a) for a in argv]
        print("74R: " + label, flush=True)
        completed = subprocess.run(argv, cwd=ROOT, env=dict(env, **(additions or {})), text=True, capture_output=True, timeout=timeout)
        log = completed.stdout + completed.stderr
        (args.work_dir / (label + ".log")).write_text(log)
        result["commands"].append({"label":label,"argv":argv,"exit_code":completed.returncode,"environment_additions":additions or {},"output":log})
        if completed.returncode:
            raise RuntimeError(label + " failed; see " + str(args.work_dir / (label + ".log")))
        return completed.stdout

    def summarize(samples):
        summary = {}
        for case in sorted({s["case"] for s in samples}):
            selected = [s for s in samples if s["case"] == case]
            modes = sorted({s["mode"] for s in selected})
            assert len({s["bytes"] for s in selected}) == 1, case
            entry = {"allocation_bytes":selected[0]["bytes"], "comparisons":{}}
            for baseline, candidate in [("off","on"),("74p","on"),("reference-on","on"),("reference-off","off")]:
                if baseline not in modes or candidate not in modes:
                    continue
                a = sorted((s for s in selected if s["mode"] == baseline), key=lambda s:s["pair"])
                b = sorted((s for s in selected if s["mode"] == candidate), key=lambda s:s["pair"])
                assert len(a) == len(b) == args.pairs
                changes = [100*(1-y["elapsed_ns"]/x["elapsed_ns"]) for x,y in zip(a,b)]
                entry["comparisons"][baseline+"->"+candidate] = {"median_time_reduction_percent":statistics.median(changes),"paired_percent":changes}
            entry["modes"] = {mode:{key:statistics.median(s[key] for s in selected if s["mode"]==mode)
                for key in ("elapsed_ns","warm_ns","hits","recoveries")} for mode in modes}
            entry["preparation"] = {key:statistics.median(s[key] for s in selected if key in s)
                for key in ("prepare_ns","plan_ns","recipe_slots","boxed_indices")}
            summary[case] = entry
        return summary

    try:
        for name in previous.AUTHORITIES:
            data = (ROOT / name).read_bytes()
            assert unchanged_authority(name, data, subprocess.check_output(["git","show",f"{BASE}:{baseline_path(name)}"],cwd=ROOT)), name
            result["authorities"][name] = sha(data)
        for profile in ("debug","release"):
            flags = ["--release"] if profile == "release" else []
            output = run(["cargo","test","-p","birnam-codegen","--locked","--offline",*flags,"small_callee","--","--nocapture"],profile+"-compiler")
            assert "2 passed; 0 failed" in output
            output = run(["cargo","test","-p","birnam-runtime","--locked","--offline",*flags,"production_","--","--nocapture"],profile+"-runtime")
            rows = [{"kind":prefix.strip(),"data":json.loads(line[len(prefix):])} for line in output.splitlines() for prefix in PREFIXES if line.startswith(prefix)]
            assert len(rows)==17 and "6 passed; 0 failed" in output
            result["profiles"][profile] = sorted(rows,key=lambda row:json.dumps(row,sort_keys=True))
        assert result["profiles"]["debug"] == result["profiles"]["release"]
        if args.measure:
            output = run(["cargo","test","-p","birnam-runtime","--release","--locked","--offline","--no-run","--message-format=json"],"timing-build")
            binary = next(r["executable"] for line in output.splitlines() if (r:=json.loads(line)).get("reason")=="compiler-artifact" and r.get("executable") and r["profile"]["test"])
            result["binary_sha256"] = sha(Path(binary).read_bytes())
            if args.reference_binary:
                expected = load_json(ROOT / "docs/measurements/74p-unqualified-timing.json")["binary_sha256"]
                assert sha(args.reference_binary.read_bytes()) == expected
                result["reference_binary_sha256"] = expected
            result["power"] = subprocess.check_output(["pmset","-g","batt"],text=True)
            samples = []; result["samples"] = samples
            paused = False
            try:
                if args.pause_rk_pid:
                    pid = str(args.pause_rk_pid)
                    comm = subprocess.check_output(["ps","-p",pid,"-o","comm="],text=True).strip()
                    assert comm == "/Users/chazu/.local/bin/rk", comm
                    assert "T" not in subprocess.check_output(["ps","-p",pid,"-o","stat="],text=True)
                    os.kill(args.pause_rk_pid,signal.SIGSTOP); paused = True
                for pair in range(args.pairs):
                    if not args.unqualified:
                        run(["bash","scripts/bench-host-check.sh",f"74R-pair-{pair}"],f"host-{pair}")
                    log = (args.work_dir / f"host-interval-{pair}.log").open("w")
                    monitor = subprocess.Popen(["top","-l","0","-s","1","-n","0","-stats","pid"],stdout=log,stderr=subprocess.DEVNULL)
                    try:
                        cases = ["projection","alias1","alias","fallback","control"]
                        cases = cases[pair%5:] + cases[:pair%5]
                        modes = list(itertools.permutations(["off","74p","on"]))[pair%6]
                        for case in cases:
                            jobs = [(mode,False) for mode in modes]
                            if args.reference_binary and case in ("projection","alias1"):
                                reference = [(mode,True) for mode in ("off","on")]
                                jobs = reference + jobs if pair%2 == 0 else jobs + reference
                            for mode,old in jobs:
                                if old:
                                    argv = [args.reference_binary,"runtime::native_tier::leaf_tests::projection_timing","--exact","--ignored","--nocapture"]
                                    additions = {"BIRNAM_74P_MODE":mode,"BIRNAM_74P_CASE":"control" if case=="alias1" else case,"BIRNAM_74P_COUNT":str(args.count)}
                                    prefix = "PROJECTION_TIMING "; labelmode = "reference-"+mode
                                else:
                                    argv = [binary,"runtime::native_tier::leaf_tests::alias_tests::small_callee_timing","--exact","--ignored","--nocapture"]
                                    additions = {"BIRNAM_74R_MODE":mode,"BIRNAM_74R_CASE":case,"BIRNAM_74R_COUNT":str(args.count)}
                                    prefix = "SMALL_CALLEE_TIMING "; labelmode = mode
                                output = run(argv,f"pair-{pair}-{case}-{labelmode}",additions,timeout=30)
                                sample = next(json.loads(line[len(prefix):]) for line in output.splitlines() if line.startswith(prefix))
                                samples.append({**sample,"pair":pair,"case":case,"mode":labelmode})
                    finally:
                        monitor.terminate(); monitor.wait(); log.close()
                    if not args.unqualified:
                        run(["bash","scripts/bench-host-check.sh","--samples",args.work_dir/f"host-interval-{pair}.log",f"74R-pair-{pair}"],f"interval-{pair}")
                result["timing_summary"] = summarize(samples)
                if not args.unqualified:
                    result["timing_qualification"] = "host-gated"
            finally:
                if paused:
                    os.kill(args.pause_rk_pid,signal.SIGCONT)
                    result["rk_resumed"] = True
        if args.verify:
            run(["mise","run","verify"],"full-verification")
            result["full_verification"] = "passed"
        assert sources() == result["sources"], "sources changed during run"
        result["status"] = "passed"
    except Exception as error:
        result["status"] = "failed"; result["error"] = str(error)
        raise
    finally:
        args.output.parent.mkdir(parents=True,exist_ok=True)
        args.output.write_text(json.dumps(result,indent=2)+"\n")


if __name__ == "__main__":
    main()
