#!/usr/bin/env python3
"""Design 74 E1 diagnostic driver: baseline versus guarded slot access.

    python3 scripts/e1-slot-access.py PREFIX [--sizes 0,1,32,1024,16384] [--pairs 5]

PREFIX must be a dev-feature prefix (built with
BIRNAM_BUILD_CARGO_FEATURES=birnam-id/dev-guarded-slots,birnam-tcc/dev-guarded-slots,birnam-codegen/dev-guarded-slots).
Baseline and candidate run the same prefix and the same fixture source; only
BIRNAM_DEV_GUARDED_SLOTS=1 in the candidate process selects the guarded
emission for the fixture's own compiled unit. Each pair is two fresh
processes in alternating order; medians are reported per size along with the
candidate's guard counters from one separate instrumented run. This is a
development diagnostic, not release evidence (Design 74 §7.3, §7.5).
"""
import argparse
import os
import statistics
import subprocess
import sys
import tempfile
from pathlib import Path


def run(binary, root, iterations, guarded, counters, workdir):
    env = {k: v for k, v in os.environ.items() if not k.startswith("BIRNAM_DEV_")}
    env["E1_ITERATIONS"] = str(iterations)
    unit = Path(workdir) / f"e1-{'guarded' if guarded else 'baseline'}-{iterations}-{os.getpid()}.dylib"
    env["E1_UNIT_PATH"] = str(unit)
    if guarded:
        env["BIRNAM_DEV_GUARDED_SLOTS"] = "1"
    if counters:
        env["BIRNAM_DEV_GUARDED_SLOTS_COUNT"] = "1"
    if unit.exists():
        unit.unlink()
    result = subprocess.run(
        [str(binary), "bench/e1-slot-access.birnam"],
        cwd=root,
        env=env,
        stdin=subprocess.DEVNULL,
        capture_output=True,
        text=True,
        timeout=600,
    )
    if unit.exists():
        unit.unlink()
    lines = result.stdout.splitlines()
    if result.returncode != 0 or "e1-ready" not in lines:
        raise RuntimeError(f"fixture failed (exit {result.returncode}): {result.stdout!r} {result.stderr!r}")
    values = dict(line.split(" ", 1) for line in lines if " " in line)
    counter_line = next((line for line in result.stderr.splitlines() if line.startswith("birnam-dev-guarded-slots")), None)
    return {
        "empty_us": int(values["empty-us"]),
        "step_us": int(values["step-us"]),
        "empty_answer": int(values["empty-answer"]),
        "checksum": int(values["checksum"]),
        "counters": counter_line,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("prefix")
    parser.add_argument("--sizes", default="0,1,32,1024,16384")
    parser.add_argument("--pairs", type=int, default=5)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    binary = Path(args.prefix) / "bin" / "birnam"
    sizes = [int(value) for value in args.sizes.split(",")]
    print(f"prefix {args.prefix}")
    print(f"{'n':>6} {'baseline empty':>15} {'guarded empty':>14} {'delta':>7} {'baseline step':>14} {'guarded step':>13} {'delta':>7}  checksum  counters")
    with tempfile.TemporaryDirectory() as workdir:
        for size in sizes:
            expected = 7 * size + size * (size + 1) // 2
            samples = {False: [], True: []}
            for pair in range(args.pairs):
                order = [False, True] if pair % 2 == 0 else [True, False]
                for guarded in order:
                    sample = run(binary, root, size, guarded, False, workdir)
                    if sample["empty_answer"] != 7 or sample["checksum"] != expected:
                        raise RuntimeError(f"n={size} guarded={guarded}: answer {sample['empty_answer']} checksum {sample['checksum']} expected {expected}")
                    samples[guarded].append(sample)
            counted = run(binary, root, size, True, True, workdir)
            base_empty = statistics.median(s["empty_us"] for s in samples[False])
            cand_empty = statistics.median(s["empty_us"] for s in samples[True])
            base_step = statistics.median(s["step_us"] for s in samples[False])
            cand_step = statistics.median(s["step_us"] for s in samples[True])
            def delta(a, b):
                return "n/a" if a == 0 else f"{(b - a) / a * 100:+.1f}%"
            print(f"{size:>6} {base_empty:>15.0f} {cand_empty:>14.0f} {delta(base_empty, cand_empty):>7} {base_step:>14.0f} {cand_step:>13.0f} {delta(base_step, cand_step):>7}  {expected:>8}  {counted['counters']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
