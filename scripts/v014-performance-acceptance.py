#!/usr/bin/env python3
"""Bounded, one-release acceptance for Design 60HB's v0.14 tradeoffs."""

import hashlib
import json
import statistics
import sys


BASELINE_SHA256 = "98e90eb7e5049b0e17fa73838c2b220888ab0285e1427a9fd2b43bafc5cc65ca"
FLOOR_SHA256 = "9232fd4c3eca912afd81bbcd97316f0145b2ec0551086b724450f96cc3b8be12"
QUARANTINED_NET_CASE = "map.at-miss"

# These cases subtract a large harness cost from the measured process time.
# Bound their stable raw measurements rather than amplifying subtraction noise.
RAW_REFERENCE = {
    "error.propagate-5-deep": 75641,
    "ffi.wrapped-call": 8380,
    "map.at-hit": 8054,
    "sends.compiled-unary": 8391,
}

# Image save is bimodal across its three clean runs. Its worst observed run is
# stable across candidates, so retain all three samples and bound their maximum.
MAX_RUN_REFERENCE = {
    "image.save": 515506000,
}

# Medians from clean candidate 637419b on Mac15,12. A rerun may exceed these
# only by the ordinary Design 46 noise allowance: 10% for micro, 5% otherwise.
REFERENCE = {
    "blocks.value": ("micro", 105),
    "cells.read": ("micro", 615),
    "cells.write": ("micro", 2579),
    "codegen.define-method": ("micro", 406593),
    "error.propagate-5-deep": ("micro", 13943),
    "error.raise-handle": ("micro", 59874),
    "ffi.peek-poke": ("micro", 471),
    "ffi.wrapped-call": ("micro", 468),
    "gc.collect-known-heap": ("gc", 317029000),
    "image.save": ("image", 442066000),
    "loops.timesRepeat": ("micro", 49844),
    "loops.to-do": ("micro", 49574),
    "loops.whileTrue": ("micro", 45190),
    "map.at-hit": ("micro", 136),
    "map.at-miss": ("micro", -466),
    "map.put-10k": ("micro", 284032000),
    "parse.json-20kb": ("parse", 574634000),
    "pull.nested": ("relational", 559288000),
    "query.aggregate": ("relational", 699770000),
    "query.asOf": ("relational", 477114000),
    "query.join-3way": ("relational", 2415907000),
    "query.negation": ("relational", 781671000),
    "query.rule-recursive": ("relational", 696191000),
    "query.single-clause": ("relational", 701726000),
    "range.asVector": ("micro", 284393000),
    "sends.compiled-unary": ("micro", 551),
    "sends.dnu-mint": ("micro", 1441),
    "sends.keyword3": ("micro", 2848),
    "sends.unary": ("micro", 1718),
    "store.assert-10k": ("relational", 90603),
    "string.indexOf-native": ("micro", 274552),
    "string.scan-derived": ("micro", 3084201000),
    "string.split-native": ("micro", 2769593),
    "stringbuilder.build-10k": ("micro", 64053000),
    "vector.at-random": ("micro", 1394),
    "vector.copy-1k": ("micro", 194955),
    "vector.fold-sum": ("micro", 284885000),
    "vector.includes-miss": ("micro", 161027),
    "vector.push-10k": ("micro", 282262000),
    "vector.reversed": ("micro", 2001234),
    "boot.bare": ("macro", 10468),
    "boot.image": ("macro", 1834602),
    "boot.stdlib-dylib": ("macro", 135416),
    "boot.stdlib-source": ("macro", 331242),
    "compile.stage1-stdlib": ("macro", 117474),
    "docgen.full": ("macro", 54357008),
    "script.primes": ("macro", 153087),
    "script.word-frequency": ("macro", 145883),
}


class Rejected(ValueError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise Rejected(message)


def gather(document: dict) -> tuple[dict[str, tuple[str, float]], dict[str, list[dict]]]:
    values: dict[str, list[float]] = {}
    labels: dict[str, str] = {}
    rows: dict[str, list[dict]] = {}
    for suite in document.get("suites", []):
        label = suite.get("label")
        for case in suite.get("data", {}).get("cases", []):
            name = case.get("name")
            require(isinstance(name, str) and isinstance(label, str), "malformed suite case")
            require(case.get("ops", 0) > 0, f"{name}: nonpositive operation count")
            value = case.get("net_us") * 1000 / case.get("ops")
            require(labels.setdefault(name, label) == label, f"{name}: inconsistent labels")
            values.setdefault(name, []).append(value)
            rows.setdefault(name, []).append(case)
    for case in document.get("macro", []):
        name = case.get("name")
        require(isinstance(name, str), "malformed macro case")
        require(name not in values, f"{name}: duplicate suite and macro case")
        labels[name] = "macro"
        values[name] = [case.get("wall_us_median")]
        rows[name] = [case]
    return ({name: (labels[name], statistics.median(seen)) for name, seen in values.items()}, rows)


def validate_candidate(document: dict, baseline_machine: str) -> None:
    stamp = document.get("stamp", {})
    require(stamp.get("version") == "0.14.0", "candidate is not v0.14.0")
    require(stamp.get("machine") == baseline_machine, "candidate machine differs from baseline")
    require(stamp.get("floor") == "rust", "candidate is not the Rust floor")
    require(stamp.get("mode") == "release" and stamp.get("quick") is False, "candidate is not a full release run")
    require(stamp.get("dirty") is False, "candidate benchmark is dirty")
    require(stamp.get("floor_sha256") == FLOOR_SHA256, "candidate runtime bytes differ from the accepted v0.14 floor")

    observed, rows = gather(document)
    require(set(observed) == set(REFERENCE), "candidate benchmark case set differs from the accepted v0.14 set")
    for name, (reference_label, reference_value) in REFERENCE.items():
        label, value = observed[name]
        require(label == reference_label, f"{name}: layer changed from {reference_label} to {label}")
        if name == QUARANTINED_NET_CASE:
            require(
                all(row.get("ns_per_op_raw", 0) > 0 and row.get("median_us", 0) > 0 for row in rows[name]),
                f"{name}: quarantined net still requires complete positive raw measurements",
            )
            continue
        if name in RAW_REFERENCE:
            raw_values = [row.get("ns_per_op_raw", 0) for row in rows[name]]
            require(all(raw > 0 for raw in raw_values), f"{name}: incomplete raw measurements")
            raw_value = statistics.median(raw_values)
            raw_ceiling = RAW_REFERENCE[name] * 1.10
            require(
                raw_value <= raw_ceiling,
                f"{name}: raw {raw_value:.3f} exceeds accepted raw ceiling {raw_ceiling:.3f}",
            )
            continue
        if name in MAX_RUN_REFERENCE:
            run_values = [row.get("net_us", 0) * 1000 / row.get("ops", 0) for row in rows[name]]
            require(all(run > 0 for run in run_values), f"{name}: incomplete run measurements")
            run_value = max(run_values)
            run_ceiling = MAX_RUN_REFERENCE[name] * 1.05
            require(
                run_value <= run_ceiling,
                f"{name}: run {run_value:.3f} exceeds accepted run ceiling {run_ceiling:.3f}",
            )
            continue
        require(value > 0, f"{name}: unusable nonpositive median")
        allowance = 1.10 if label == "micro" else 1.05
        require(value <= reference_value * allowance, f"{name}: {value:.3f} exceeds accepted ceiling {reference_value * allowance:.3f}")


def digest(path: str) -> str:
    value = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def main() -> None:
    if len(sys.argv) != 3:
        print("usage: v014-performance-acceptance.py BASELINE_JSON CANDIDATE_JSON", file=sys.stderr)
        raise SystemExit(2)
    try:
        require(digest(sys.argv[1]) == BASELINE_SHA256, "prior-production baseline bytes differ from Design 60HB")
        with open(sys.argv[1], encoding="utf-8") as handle:
            baseline = json.load(handle)
        with open(sys.argv[2], encoding="utf-8") as handle:
            candidate = json.load(handle)
        baseline_stamp = baseline.get("stamp", {})
        require(baseline_stamp.get("sha") == "2fe44d2", "unexpected prior-production baseline commit")
        require(baseline_stamp.get("machine") == "Mac15,12", "unexpected prior-production baseline machine")
        validate_candidate(candidate, baseline_stamp["machine"])
    except (OSError, TypeError, ValueError, json.JSONDecodeError) as error:
        print(f"v0.14 performance acceptance rejected: {error}", file=sys.stderr)
        raise SystemExit(1)
    print(
        f"v0.14 performance acceptance: "
        f"{len(REFERENCE) - len(RAW_REFERENCE) - len(MAX_RUN_REFERENCE) - 1} median-bounded cases, "
        f"{len(RAW_REFERENCE)} raw-bounded cases, {len(MAX_RUN_REFERENCE)} run-bounded case, "
        f"and {QUARANTINED_NET_CASE} quarantine passed"
    )


if __name__ == "__main__":
    main()
