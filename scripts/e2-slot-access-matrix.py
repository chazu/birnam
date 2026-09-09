#!/usr/bin/env python3
"""Design 74 E2 driver: run every matrix case in the baseline and guarded configurations.

    python3 scripts/e2-slot-access-matrix.py BASELINE_PREFIX DEV_PREFIX

Each case runs in its own process per configuration. The two configurations
must print byte-identical stdout (the churn elapsed line is compared only for
presence). Cases that abort in both configurations are reported as identical
aborts. The candidate's guard counters are recorded from stderr.
"""
import os
import subprocess
import sys
import tempfile
from pathlib import Path

CASES = ["empty", "ordinary-error", "replace-step", "replace-during-argument",
         "override-oopat-before", "override-oopat-during", "activate-rollback",
         "gc-nested", "churn", "invalid-body"]


def run(binary, root, case, guarded, workdir):
    env = {k: v for k, v in os.environ.items() if not k.startswith("BIRNAM_DEV_")}
    env["E2_CASE"] = case
    env["E2_UNIT_PATH"] = str(Path(workdir) / f"e2-{case}-{'guarded' if guarded else 'baseline'}.dylib")
    if guarded:
        env["BIRNAM_DEV_GUARDED_SLOTS"] = "1"
        env["BIRNAM_DEV_GUARDED_SLOTS_COUNT"] = "1"
    result = subprocess.run([str(binary), "bench/e2-slot-access-matrix.birnam"], cwd=root, env=env,
                            stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=600)
    stdout = [line for line in result.stdout.splitlines() if not line.startswith("churn-us")]
    churn = next((line for line in result.stdout.splitlines() if line.startswith("churn-us")), "")
    counters = next((line for line in result.stderr.splitlines() if line.startswith("birnam-dev-guarded-slots")), "")
    return result.returncode, stdout, churn, counters, result.stderr


def main():
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    root = Path(__file__).resolve().parent.parent
    baseline = Path(sys.argv[1]) / "bin" / "birnam"
    candidate = Path(sys.argv[2]) / "bin" / "birnam"
    failures = 0
    with tempfile.TemporaryDirectory() as workdir:
        for case in CASES:
            base = run(baseline, root, case, False, workdir)
            cand = run(candidate, root, case, True, workdir)
            same_out = base[1] == cand[1]
            same_exit = base[0] == cand[0]
            body = " ".join(line for line in cand[1] if not line.startswith("e2-"))
            status = "SAME" if same_out and same_exit else "DIFF"
            if status == "DIFF":
                failures += 1
            aborted = " (both aborted)" if base[0] != 0 and cand[0] != 0 and same_exit else ""
            print(f"{status:4} {case:24} exit {cand[0]:>3}{aborted}: {body} | {cand[3]} {base[2]} {cand[2]}".rstrip())
            if status == "DIFF":
                print("  baseline:", base[1], base[0])
                print("  guarded: ", cand[1], cand[0])
    print("e2-matrix:", "all cases identical" if failures == 0 else f"{failures} case(s) differ")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
