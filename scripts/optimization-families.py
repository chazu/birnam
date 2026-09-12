#!/usr/bin/env python3
"""74S per-family correctness and paired timing receipts."""
import argparse
import datetime
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import signal
import statistics
import subprocess
import time
import itertools

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("prior", ROOT / "scripts/production-recovery.py")
prior = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prior)
BASE = "d06f7558"


def sources():
    result = prior.sources()
    for name in ("scripts/optimization-families.py", "scripts/bench-host-check.sh", "scripts/fixtures/native-regions.birnam"):
        result[name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
    return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--family", choices=["dispatch", "reads"], default="dispatch")
    p.add_argument("--measure", action="store_true")
    p.add_argument("--matrix", choices=["focused", "dispatch", "native-controls"], default="focused")
    p.add_argument("--verify", action="store_true")
    p.add_argument("--pairs", type=int, default=8)
    p.add_argument("--count", type=int)
    p.add_argument("--pause-rk-pid", type=int)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--work-dir", type=Path, required=True)
    args = p.parse_args()
    if args.matrix == "dispatch": args.matrix = "focused"
    assert args.family == "dispatch" or args.matrix == "focused"
    modes = ["off", "on"] if args.family == "dispatch" else ["off", "plain", "reuse"]
    if args.count is None: args.count = 1000000 if args.family == "dispatch" else 10000
    assert args.pairs > 0 and args.count > 0
    args.work_dir.mkdir(parents=True, exist_ok=True)
    env = {k:v for k,v in os.environ.items() if not k.startswith(("BIRNAM_", "DYLD_", "BENCH_HOST_"))}
    env.update(CARGO_TARGET_DIR=str(ROOT / ".cargo-target"), MISE_TRUSTED_CONFIG_PATHS=str(ROOT))
    receipt = dict(design="74S", family=args.family, baseline=BASE, status="running",
                   matrix=args.matrix, timestamp_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                   sources=sources(), commands=[], samples=[], timing_qualification="not measured")

    def run(argv, label, extra=None):
        print(label, flush=True)
        result = subprocess.run([str(x) for x in argv], cwd=ROOT, env=dict(env, **(extra or {})),
                                text=True, capture_output=True)
        output = result.stdout + result.stderr
        (args.work_dir / (label + ".log")).write_text(output)
        receipt['commands'].append(dict(argv=[str(x) for x in argv], label=label,
                                       environment=extra or {}, exit_code=result.returncode, output=output))
        assert result.returncode == 0, label + " failed"
        return result.stdout

    try:
        receipt['authorities'] = {}
        for name in prior.AUTHORITIES:
            data = (ROOT / name).read_bytes()
            assert prior.unchanged_authority(name, data, subprocess.check_output(["git", "show", f"{BASE}:{name}"], cwd=ROOT)), name
            receipt['authorities'][name] = hashlib.sha256(data).hexdigest()
        for profile in ["debug", "release"]:
            flags = ["--release"] if profile == "release" else []
            if args.family == "reads":
                out = run(["cargo", "test", "-p", "birnam-codegen", "--locked", "--offline", *flags,
                           "native_region", "--", "--nocapture"], profile + "-region-compiler")
                assert "1 passed; 0 failed" in out
                out = run(["cargo", "test", "-p", "birnam-id", "--locked", "--offline", *flags,
                           "native_region_", "--", "--nocapture"], profile + "-regions")
                assert "3 passed; 0 failed" in out
            out = run(["cargo", "test", "-p", "birnam-id", "--locked", "--offline", *flags,
                       "dispatch_specialization", "--", "--nocapture"], profile)
            assert "2 passed; 0 failed" in out
            out = run(["cargo", "test", "-p", "birnam-id", "--locked", "--offline", *flags,
                       "guarded_lookup", "--", "--nocapture"], profile + "-lookup")
            assert "2 passed; 0 failed" in out
        if args.verify:
            run(["mise", "run", "verify"], "full-verification")
            receipt['full_verification'] = 'passed'
        if args.measure:
            out = run(["cargo", "test", "-p", "birnam-id", "--release", "--locked", "--offline",
                       "--no-run", "--message-format=json"], "timing-build")
            binary = next(row['executable'] for line in out.splitlines()
                          if (row := json.loads(line)).get('executable') and row['profile']['test'])
            receipt['binary_sha256'] = hashlib.sha256(Path(binary).read_bytes()).hexdigest()
            receipt['power'] = subprocess.check_output(["pmset", "-g", "batt"], text=True)
            paused = False
            try:
                if args.pause_rk_pid:
                    pid = str(args.pause_rk_pid)
                    assert subprocess.check_output(["ps", "-p", pid, "-o", "comm="], text=True).strip() == "/Users/chazu/.local/bin/rk"
                    assert 'T' not in subprocess.check_output(["ps", "-p", pid, "-o", "stat="], text=True)
                    os.kill(args.pause_rk_pid, signal.SIGSTOP)
                    paused = True
                for pair in range(args.pairs):
                    # Wait before a pair, never filter samples by their result.
                    # Interval failure still rejects the whole matrix.
                    attempt = 0
                    while True:
                        if args.pause_rk_pid and not paused:
                            pid = str(args.pause_rk_pid)
                            assert subprocess.check_output(["ps", "-p", pid, "-o", "comm="], text=True).strip() == "/Users/chazu/.local/bin/rk"
                            assert 'T' not in subprocess.check_output(["ps", "-p", pid, "-o", "stat="], text=True)
                            os.kill(args.pause_rk_pid, signal.SIGSTOP)
                            paused = True
                        try:
                            run(["bash", "scripts/bench-host-check.sh", f"74S-{args.family}-{pair}"], f"host-{pair}-attempt-{attempt}")
                            break
                        except AssertionError:
                            if paused:
                                os.kill(args.pause_rk_pid, signal.SIGCONT)
                                paused = False
                            attempt += 1
                            if attempt >= 40:
                                raise
                            print("Host busy; rk resumed; retrying preflight in 30 seconds", flush=True)
                            time.sleep(30)
                    path = args.work_dir / f"cpu-{pair}.log"
                    with path.open('w') as log:
                        monitor = subprocess.Popen(["top", "-l", "0", "-s", "1", "-n", "0", "-stats", "pid"], stdout=log, stderr=subprocess.DEVNULL)
                        try:
                            cases = (["inherited", "direct", "lookup", "churn", "inherited-churn"]
                                     if args.matrix == "focused" else ["projection", "alias1", "alias", "fallback", "control"])
                            if args.family == "reads": cases = ["repeat", "chain", "arguments", "fallback", "control"]
                            cases = cases[pair%len(cases):] + cases[:pair%len(cases)]
                            for case in cases:
                                order = list(itertools.permutations(modes))[pair % len(list(itertools.permutations(modes)))]
                                for mode in order:
                                    if args.family == "reads":
                                        name = "runtime::native_tier::leaf_tests::region_tests::native_region_timing"
                                        extra = dict(BIRNAM_74S_MODE=mode, BIRNAM_74S_CASE=case, BIRNAM_74S_COUNT=str(args.count))
                                        prefix = 'REGION_TIMING '
                                    elif args.matrix == "focused":
                                        name = "runtime::dispatch_cache::tests::dispatch_specialization_timing"
                                        extra = dict(BIRNAM_74S_MODE=mode, BIRNAM_74S_CASE=case, BIRNAM_74S_COUNT=str(args.count))
                                        prefix = 'DISPATCH_TIMING '
                                    else:
                                        name = "runtime::native_tier::leaf_tests::alias_tests::small_callee_timing"
                                        extra = dict(BIRNAM_74S_DISPATCH=mode, BIRNAM_74R_MODE="on", BIRNAM_74R_CASE=case, BIRNAM_74R_COUNT=str(args.count))
                                        prefix = 'SMALL_CALLEE_TIMING '
                                    out = run([binary, name, "--exact", "--ignored", "--nocapture"], f"pair-{pair}-{case}-{mode}", extra)
                                    sample = next(json.loads(line.removeprefix(prefix)) for line in out.splitlines() if line.startswith(prefix))
                                    sample.update(mode=mode, counter_kind="region" if args.family=="reads" else "dispatch" if args.matrix=="focused" else "small-callee")
                                    receipt['samples'].append(dict(pair=pair, **sample))
                        finally:
                            monitor.terminate()
                            monitor.wait()
                    run(["bash", "scripts/bench-host-check.sh", "--samples", path, f"74S-{args.family}-{pair}"], f"interval-{pair}")
                receipt['timing_summary'] = {}
                for case in sorted({r['case'] for r in receipt['samples']}):
                    rows = [r for r in receipt['samples'] if r['case'] == case]
                    assert len({r['bytes'] for r in rows}) == 1
                    comparisons = {}
                    for first, last in ([('off','on')] if args.family=='dispatch' else [('off','plain'),('plain','reuse'),('off','reuse')]):
                        left = sorted((r for r in rows if r['mode']==first), key=lambda r:r['pair'])
                        right = sorted((r for r in rows if r['mode']==last), key=lambda r:r['pair'])
                        assert len(left) == len(right) == args.pairs
                        reductions = [100*(1-b['elapsed_ns']/a['elapsed_ns']) for a,b in zip(left,right)]
                        comparisons[first+'->'+last] = dict(median_time_reduction_percent=statistics.median(reductions), paired_percent=reductions)
                    receipt['timing_summary'][case] = dict(comparisons=comparisons,
                        modes={mode:{key:statistics.median(r[key] for r in rows if r['mode']==mode)
                                    for key in ('elapsed_ns','prepare_ns','warm_ns','bytes','hits','misses','loads','recoveries','source_bytes','metadata_bytes','plan_storage_bytes','compile_ns') if key in rows[0]} for mode in modes})
                receipt['timing_qualification'] = 'host-gated'
            finally:
                if paused:
                    os.kill(args.pause_rk_pid, signal.SIGCONT)
                    receipt['rk_resumed'] = True
        assert receipt['sources'] == sources(), 'sources changed during run'
        receipt['status'] = 'passed'
    except Exception as error:
        receipt['status'] = 'failed'
        receipt['error'] = str(error)
        raise
    finally:
        receipt['host_interval_logs'] = {p.name:p.read_text() for p in args.work_dir.glob('cpu-*.log')}
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(receipt, indent=2)+'\n')


if __name__ == '__main__':
    main()
