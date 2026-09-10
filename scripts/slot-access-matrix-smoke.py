#!/usr/bin/env python3
"""Guarded slot-access smoke (Design 74E §3.7): the Design 74 E2 semantic matrix
on one installed prefix, checked against the outputs recorded in 74C §1.

    python3 scripts/slot-access-matrix-smoke.py PREFIX

Each case compiles its own resumable unit in a fresh process. The two cases
that override `oopAt:` on Machine abort in every runtime (birnam-kh3) and are
not run here.
"""
import os
import subprocess
import sys
import tempfile
from pathlib import Path

EXPECTED = {
    "empty": "zero 0 calls 0",
    "ordinary-error": "result error:#e2Stop calls 2",
    "replace-step": "checksum 20150 calls 20",
    "replace-during-argument": "checksum 20150 calls 20",
    "activate-rollback": "before 350 successor-compiled false successor-mapped false successor-activated false during 20350 rolled-back false after 350",
    "gc-nested": "checksum 21500 calls 200",
    "churn": "checksum 2015000 calls 2000",
    "invalid-body": "result error:#doesNotUnderstand",
}


def run(binary, root, case, workdir):
    env = dict(os.environ)
    env["E2_CASE"] = case
    env["E2_UNIT_PATH"] = str(Path(workdir) / f"slot-access-{case}.dylib")
    result = subprocess.run([str(binary), "bench/slot-access-matrix.birnam"], cwd=root, env=env,
                            stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=600)
    lines = [line for line in result.stdout.splitlines() if not line.startswith(("churn-us", "e2-"))]
    return result.returncode, " ".join(lines), result.stderr


def main():
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    root = Path(__file__).resolve().parent.parent
    binary = Path(sys.argv[1]) / "bin" / "birnam"
    failures = 0
    with tempfile.TemporaryDirectory() as workdir:
        for case, expected in EXPECTED.items():
            code, body, stderr = run(binary, root, case, workdir)
            ok = code == 0 and body == expected
            failures += 0 if ok else 1
            print(f"{'ok  ' if ok else 'FAIL'} {case:24} exit {code:>3}: {body}")
            if not ok:
                print("  expected:", expected)
                print("  stderr:  ", stderr.strip()[-400:])
    print("slot-access-matrix-smoke:", "all cases match" if failures == 0 else f"{failures} case(s) differ")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
