#!/usr/bin/env python3
"""Design 74 E3 driver: independent baseline and candidate prefixes, host-gated.

    python3 scripts/e3-slot-access.py --baseline PREFIX --candidate DEV_PREFIX \
        [--control DEV_PREFIX] [--sizes 0,1,32,1024,16384] [--pairs 5] \
        [--objc-dir DIR] --out RECORD.json

Follows the protocol frozen in Design 74C §2: alternating fresh-process pairs
after a bench-host-check preflight, cold and warm phases from the fixture, all
samples retained, one instrumented run per size for guard counters, peak RSS
from one extra unmeasured run, counting overhead, generated-code inspection
of one candidate unit, the retained no-auto-GC control probe on both prefixes,
and the Objective-C / C references when --objc-dir is given.
"""
import argparse
import json
import os
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
KEYS = ["compile-us", "empty-us", "step-us", "warm-empty-us", "warm-step-us", "empty-answer", "checksum"]


def preflight(context):
    subprocess.run([str(ROOT / "scripts/bench-host-check.sh"), "--wait", context], check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def fixture_env(iterations, unit, guarded, counting):
    env = {k: v for k, v in os.environ.items() if not k.startswith("BIRNAM_DEV_")}
    env["E1_ITERATIONS"] = str(iterations)
    env["E1_UNIT_PATH"] = str(unit)
    if guarded:
        env["BIRNAM_DEV_GUARDED_SLOTS"] = "1"
    if counting:
        env["BIRNAM_DEV_GUARDED_SLOTS_COUNT"] = "1"
    return env


def run_fixture(binary, iterations, guarded, counting, workdir, keep_unit=False):
    unit = Path(workdir) / f"e3-{os.getpid()}-{'g' if guarded else 'b'}-{iterations}.dylib"
    if unit.exists():
        unit.unlink()
    result = subprocess.run([str(binary), "bench/e1-slot-access.birnam"], cwd=ROOT,
                            env=fixture_env(iterations, unit, guarded, counting),
                            stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=900)
    lines = result.stdout.splitlines()
    if result.returncode != 0 or "e1-ready" not in lines:
        raise RuntimeError(f"fixture failed ({result.returncode}): {result.stdout!r} {result.stderr!r}")
    values = {k: int(v) for k, v in (line.split(" ", 1) for line in lines if " " in line) if k in KEYS}
    values["counters"] = next((l for l in result.stderr.splitlines() if l.startswith("birnam-dev-guarded-slots")), None)
    if not keep_unit and unit.exists():
        unit.unlink()
    values["unit"] = str(unit) if keep_unit else None
    return values


def peak_rss(binary, iterations, guarded, workdir):
    unit = Path(workdir) / f"e3-rss-{'g' if guarded else 'b'}-{iterations}.dylib"
    result = subprocess.run(["/usr/bin/time", "-l", str(binary), "bench/e1-slot-access.birnam"], cwd=ROOT,
                            env=fixture_env(iterations, unit, guarded, False), stdin=subprocess.DEVNULL,
                            capture_output=True, text=True, timeout=900)
    if unit.exists():
        unit.unlink()
    for line in result.stderr.splitlines():
        if "maximum resident set size" in line:
            return int(line.split()[0])
    return None


def control_probe(binary):
    env = {k: v for k, v in os.environ.items() if not k.startswith("BIRNAM_")}
    with subprocess.Popen([str(binary), "bench/control-throughput-probe.birnam"], cwd=ROOT, env=env,
                          stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, text=True) as child:
        output = child.communicate(timeout=300)[0].splitlines()
    if len(output) != 3 or output[2] != "control-probe-ok":
        raise RuntimeError(f"control probe failed: {output!r}")
    return int(output[1])


def reference(binary, iterations):
    out = subprocess.run([str(binary), str(iterations)], capture_output=True, text=True, check=True).stdout.splitlines()
    values = {k: int(v) for k, v in (line.split(" ", 1) for line in out if " " in line)}
    return values


def inspect_unit(unit):
    imports = subprocess.run(["nm", "-u", str(unit)], capture_output=True, text=True).stdout.split()
    return {"dev_symbols": sorted(s for s in imports if "dev_frame" in s),
            "imports_birnam_send": any(s.endswith("birnam_send") for s in imports),
            "import_count": len(imports)}


def receipt(prefix):
    with open(Path(prefix) / "share/birnam/rust-install.json", encoding="utf-8") as handle:
        r = json.load(handle)
    return {k: r.get(k) for k in ("source_manifest_sha256", "cargo_invocation", "profile", "toolchain", "source_identity")}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--baseline", required=True)
    parser.add_argument("--candidate", required=True)
    parser.add_argument("--control")
    parser.add_argument("--sizes", default="0,1,32,1024,16384")
    parser.add_argument("--pairs", type=int, default=5)
    parser.add_argument("--objc-dir")
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    sizes = [int(v) for v in args.sizes.split(",")]
    configs = {"baseline": (Path(args.baseline) / "bin/birnam", False),
               "candidate": (Path(args.candidate) / "bin/birnam", True)}
    if args.control:
        configs["control"] = (Path(args.control) / "bin/birnam", False)
    record = {"schema": "birnam-design-74c-e3-v1", "sizes": sizes, "pairs": args.pairs,
              "commit": subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True).stdout.strip(),
              "machine": subprocess.run(["sysctl", "-n", "hw.model"], capture_output=True, text=True).stdout.strip(),
              "prefixes": {name: {"path": str(Path(p).parent.parent), "receipt": receipt(Path(p).parent.parent)} for name, (p, _) in configs.items()},
              "samples": {}, "medians": {}, "rss": {}, "counters": {}, "counting_overhead": {}, "inspection": {},
              "control_probe": {}, "references": {}, "host_gate": "scripts/bench-host-check.sh --wait before every pair"}
    with tempfile.TemporaryDirectory() as workdir:
        for size in sizes:
            expected = 7 * size + size * (size + 1) // 2
            samples = {name: [] for name in configs}
            order_names = [n for n in ("baseline", "candidate", "control") if n in configs]
            for pair in range(args.pairs):
                preflight(f"e3 n={size} pair {pair + 1}")
                order = order_names if pair % 2 == 0 else list(reversed(order_names))
                for name in order:
                    binary, guarded = configs[name]
                    s = run_fixture(binary, size, guarded, False, workdir)
                    if s["empty-answer"] != 7 or s["checksum"] != expected:
                        raise RuntimeError(f"{name} n={size}: answer {s['empty-answer']} checksum {s['checksum']} expected {expected}")
                    samples[name].append(s)
            record["samples"][str(size)] = samples
            record["medians"][str(size)] = {name: {k: statistics.median(s[k] for s in samples[name]) for k in KEYS[:5]}
                                             | {k + "-spread": max(s[k] for s in samples[name]) - min(s[k] for s in samples[name]) for k in KEYS[:5]}
                                             for name in configs}
            record["rss"][str(size)] = {name: peak_rss(configs[name][0], size, configs[name][1], workdir) for name in configs}
            counted = run_fixture(configs["candidate"][0], size, True, True, workdir)
            record["counters"][str(size)] = counted["counters"]
            if size in (1024, 16384):
                on = [run_fixture(configs["candidate"][0], size, True, True, workdir)["warm-step-us"] for _ in range(3)]
                off = [run_fixture(configs["candidate"][0], size, True, False, workdir)["warm-step-us"] for _ in range(3)]
                record["counting_overhead"][str(size)] = {"warm_step_us_counting": on, "warm_step_us_plain": off}
            print(f"n={size}: " + "  ".join(f"{name} warm empty {record['medians'][str(size)][name]['warm-empty-us']:.0f} step {record['medians'][str(size)][name]['warm-step-us']:.0f}" for name in configs) + f"  | {counted['counters']}")
        kept = run_fixture(configs["candidate"][0], 32, True, False, workdir, keep_unit=True)
        record["inspection"]["candidate_unit"] = inspect_unit(kept["unit"])
        kept_base = run_fixture(configs["baseline"][0], 32, False, False, workdir, keep_unit=True)
        record["inspection"]["baseline_unit"] = inspect_unit(kept_base["unit"])
        for name in ("baseline", "candidate"):
            preflight(f"e3 control probe {name}")
            record["control_probe"][name] = [control_probe(configs[name][0]) for _ in range(3)]
        if args.objc_dir:
            for ref in ("e3-objc-message", "e3-objc-message-boxed", "e3-c-primitive"):
                binary = Path(args.objc_dir) / ref
                if binary.exists():
                    record["references"][ref] = {str(size): [reference(binary, size) for _ in range(5)] for size in sizes}
    with open(args.out, "w", encoding="utf-8") as handle:
        json.dump(record, handle, indent=1)
    print("record", args.out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
