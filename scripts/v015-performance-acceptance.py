#!/usr/bin/env python3
"""One-release performance acceptance for v0.15.0 (Design 60HC).

The ordinary Design 46 comparator runs unchanged and stays authoritative for
every case. Exactly one case, `vector.includes-miss`, is judged by its raw
per-operation median instead of its empty-loop-subtracted net: v0.15 made the
subtracted loop baseline about twice as cheap, so the case's net rose while its
raw time fell. Every other flag the comparator raises still fails closed.
"""

import hashlib
import json
import os
import re
import statistics
import subprocess
import sys

# Tracked prior-production snapshot: bench/results/v0.14.0-rust-3b10b93-*.json.
BASELINE_SHA256 = "2e7f326fa9a52b955c5315785c70b618f4801729661413283aaa01371f5f4f0c"
BASELINE_COMMIT = "3b10b93"
BASELINE_MACHINE = "Mac15,12"
CANDIDATE_VERSION = "0.15.0"

# The single raw-bounded case and the ordinary micro allowance (Design 46).
RAW_BOUNDED_CASE = "vector.includes-miss"
RAW_ALLOWANCE = 1.10

REGRESSION_LINE = re.compile(r"^REG\s+(\S+?):")
SUMMARY_LINE = re.compile(r"^regressions:\s*(\d+),\s*missing:\s*(\d+),\s*unusable:\s*(\d+)")


class Rejected(ValueError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise Rejected(message)


def digest(path: str) -> str:
    value = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def raw_median(document: dict, name: str) -> float:
    """Median of a micro case's raw ns/op across the document's process runs."""
    raws = []
    for suite in document.get("suites", []):
        for case in suite.get("data", {}).get("cases", []):
            if case.get("name") == name:
                raw = case.get("ns_per_op_raw", 0)
                require(isinstance(raw, (int, float)) and raw > 0, f"{name}: incomplete raw measurement")
                raws.append(float(raw))
    require(len(raws) > 0, f"{name}: absent from benchmark document")
    return statistics.median(raws)


def judge(comparator_output: str, comparator_exit: int, baseline: dict, candidate: dict) -> str:
    """Apply the 60HC rule to the ordinary comparator's verdict.

    Returns the acceptance summary line; raises Rejected otherwise.
    """
    regressions = [match.group(1) for line in comparator_output.splitlines() if (match := REGRESSION_LINE.match(line))]
    summary = next((SUMMARY_LINE.match(line) for line in comparator_output.splitlines() if SUMMARY_LINE.match(line)), None)
    require(summary is not None, "comparator produced no summary line")
    missing = int(summary.group(2))
    require(missing == 0, f"comparator reports {missing} missing case(s)")
    require(int(summary.group(1)) == len(regressions), "comparator summary disagrees with its regression lines")

    others = [name for name in regressions if name != RAW_BOUNDED_CASE]
    require(not others, f"ordinary comparator regressions remain: {', '.join(others)}")

    if RAW_BOUNDED_CASE in regressions:
        reference = raw_median(baseline, RAW_BOUNDED_CASE)
        observed = raw_median(candidate, RAW_BOUNDED_CASE)
        ceiling = reference * RAW_ALLOWANCE
        require(
            observed <= ceiling,
            f"{RAW_BOUNDED_CASE}: raw {observed:.0f} ns/op exceeds accepted raw ceiling {ceiling:.0f} (baseline raw {reference:.0f})",
        )
        return (
            f"v0.15 performance acceptance: ordinary comparator clean except {RAW_BOUNDED_CASE}, "
            f"whose raw median {observed:.0f} ns/op is within {RAW_ALLOWANCE:.2f}x of baseline raw {reference:.0f}"
        )
    require(comparator_exit == 0, f"comparator exited {comparator_exit} without a listed regression")
    return "v0.15 performance acceptance: ordinary comparator clean; no raw-bounded override needed"


def main() -> None:
    if len(sys.argv) != 3:
        print("usage: v015-performance-acceptance.py BASELINE_JSON CANDIDATE_JSON", file=sys.stderr)
        raise SystemExit(2)
    baseline_path, candidate_path = sys.argv[1], sys.argv[2]
    try:
        require(digest(baseline_path) == BASELINE_SHA256, "prior-production baseline bytes differ from Design 60HC")
        with open(baseline_path, encoding="utf-8") as handle:
            baseline = json.load(handle)
        with open(candidate_path, encoding="utf-8") as handle:
            candidate = json.load(handle)
        baseline_stamp = baseline.get("stamp", {})
        require(baseline_stamp.get("sha") == BASELINE_COMMIT, "unexpected prior-production baseline commit")
        require(baseline_stamp.get("machine") == BASELINE_MACHINE, "unexpected prior-production baseline machine")
        stamp = candidate.get("stamp", {})
        require(stamp.get("version") == CANDIDATE_VERSION, f"candidate is not v{CANDIDATE_VERSION}")
        require(stamp.get("machine") == BASELINE_MACHINE, "candidate machine differs from baseline")
        require(stamp.get("mode") == "release" and stamp.get("quick") is False, "candidate is not a full release run")
        require(stamp.get("dirty") is False, "candidate benchmark is dirty")

        repo = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        comparator = subprocess.run(
            [os.path.join(repo, "scripts", "bench-compare.sh"), baseline_path, candidate_path],
            capture_output=True,
            text=True,
            check=False,
        )
        output = comparator.stdout + comparator.stderr
        sys.stdout.write(output)
        require(comparator.returncode in (0, 1), f"comparator failed to run (exit {comparator.returncode})")
        print(judge(output, comparator.returncode, baseline, candidate))
    except (OSError, TypeError, ValueError, json.JSONDecodeError) as error:
        print(f"v0.15 performance acceptance rejected: {error}", file=sys.stderr)
        raise SystemExit(1)


if __name__ == "__main__":
    main()
