#!/usr/bin/env python3
"""One-release performance acceptance for v0.15.0 (Design 60HC).

The ordinary Design 46 comparator runs unchanged and stays authoritative for
every layer except micro. A micro-layer regression it reports is accepted only
when the case's raw per-operation median (no empty-loop subtraction) is within
Design 46's 10% micro allowance of the baseline's raw median, or when the raw
median moved by less than the larger of the two documents' run-to-run raw
spreads (max - min over the release run's replicated micro processes). Any
regression in another layer, any missing case, and any incomplete raw
measurement still fail closed.
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

# Only the micro layer is judged by raw medians (Design 46's micro allowance).
MICRO_LAYER = "micro"
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


def layers_of(document: dict, name: str) -> set:
    """Every layer label under which the document carries a case of this name."""
    layers = set()
    for suite in document.get("suites", []):
        for case in suite.get("data", {}).get("cases", []):
            if case.get("name") == name:
                layers.add(suite.get("label"))
    for case in document.get("macro", []) or []:
        if isinstance(case, dict) and case.get("name") == name:
            layers.add("macro")
    return layers


def raw_samples(document: dict, name: str) -> list:
    """A micro case's raw ns/op from each of the document's micro processes."""
    raws = []
    for suite in document.get("suites", []):
        if suite.get("label") != MICRO_LAYER:
            continue
        for case in suite.get("data", {}).get("cases", []):
            if case.get("name") == name:
                raw = case.get("ns_per_op_raw", 0)
                require(isinstance(raw, (int, float)) and raw > 0, f"{name}: incomplete raw measurement")
                raws.append(float(raw))
    require(len(raws) > 0, f"{name}: absent from benchmark document")
    return raws


def spread(samples: list) -> float:
    return max(samples) - min(samples) if len(samples) > 1 else 0.0


def judge_micro(name: str, baseline: dict, candidate: dict) -> str:
    """Accept one micro flag by raw allowance or by run-to-run spread; raise otherwise."""
    reference_samples = raw_samples(baseline, name)
    observed_samples = raw_samples(candidate, name)
    reference = statistics.median(reference_samples)
    observed = statistics.median(observed_samples)
    ceiling = reference * RAW_ALLOWANCE
    if observed <= ceiling:
        return f"{name}: raw median {observed:.0f} ns/op within {RAW_ALLOWANCE:.2f}x of baseline raw {reference:.0f}"
    tolerance = max(spread(reference_samples), spread(observed_samples))
    delta = observed - reference
    require(
        delta <= tolerance,
        f"{name}: raw {observed:.0f} ns/op exceeds accepted raw ceiling {ceiling:.0f} "
        f"(baseline raw {reference:.0f}) and its raw delta {delta:.0f} exceeds the run-to-run spread {tolerance:.0f}",
    )
    return f"{name}: raw delta {delta:.0f} ns/op inside the run-to-run raw spread {tolerance:.0f} (baseline raw {reference:.0f})"


def judge(comparator_output: str, comparator_exit: int, baseline: dict, candidate: dict) -> str:
    """Apply the 60HC rule to the ordinary comparator's verdict.

    Returns the acceptance summary; raises Rejected otherwise.
    """
    regressions = [match.group(1) for line in comparator_output.splitlines() if (match := REGRESSION_LINE.match(line))]
    summary = next((SUMMARY_LINE.match(line) for line in comparator_output.splitlines() if SUMMARY_LINE.match(line)), None)
    require(summary is not None, "comparator produced no summary line")
    missing = int(summary.group(2))
    require(missing == 0, f"comparator reports {missing} missing case(s)")
    require(int(summary.group(1)) == len(regressions), "comparator summary disagrees with its regression lines")

    others = [name for name in regressions if layers_of(candidate, name) != {MICRO_LAYER}]
    require(not others, f"ordinary comparator regressions remain outside the micro layer: {', '.join(others)}")

    if regressions:
        accepted = [judge_micro(name, baseline, candidate) for name in regressions]
        return "v0.15 performance acceptance: ordinary comparator clean except micro-layer flags judged by raw medians:\n  " + "\n  ".join(
            accepted
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
