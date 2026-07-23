#!/usr/bin/env python3
"""Fail-closed verifier for the Design 62 N0.5 frozen baseline."""

from __future__ import annotations

import hashlib
import json
from collections import Counter
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "abi/native-rung-baseline-v1.json"


def fail(message: str) -> None:
    raise SystemExit(f"native-rung-baseline: {message}")


def load_json(path: Path) -> dict:
    try:
        with path.open() as source:
            return json.load(source)
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read {path.relative_to(ROOT)}: {error}")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(64 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify_semantics(manifest: dict) -> None:
    for evidence in manifest["semantic_evidence"]:
        path = ROOT / evidence["path"]
        try:
            text = path.read_text()
        except OSError as error:
            fail(f"missing semantic evidence {evidence['path']}: {error}")
        for needle in evidence["contains"]:
            if needle not in text:
                fail(
                    f"{evidence['contract']} drifted: {needle!r} "
                    f"is absent from {evidence['path']}"
                )


def suite_cases(result: dict) -> set[str]:
    return {
        case["name"]
        for suite in result["suites"]
        for case in suite["data"]["cases"]
    }


def verify_design46(manifest: dict) -> None:
    performance = manifest["performance"]
    methodology = performance["methodology"]
    results: list[dict] = []
    for floor in ("zig", "rust"):
        frozen = performance["accepted_pair"][floor]
        path = ROOT / frozen["path"]
        if not path.is_file():
            fail(f"missing accepted {floor} result {frozen['path']}")
        actual = sha256(path)
        if actual != frozen["sha256"]:
            fail(
                f"{floor} result digest mismatch: expected {frozen['sha256']}, "
                f"got {actual}"
            )
        result = load_json(path)
        stamp = result["stamp"]
        if (
            stamp["optimize"] != methodology["optimize"]
            or stamp["mode"] != "release"
            or stamp["quick"] is not methodology["quick"]
            or stamp["floor"] != floor
        ):
            fail(f"{floor} result is not the frozen guarded ReleaseSafe run")

        expected_processes = {
            "micro": methodology["micro_processes"],
            "parse": methodology["parse_processes"],
            "relational": methodology["relational_processes"],
            "gc": methodology["gc_processes"],
            "image": methodology["image_processes"],
        }
        actual_processes = Counter(suite["label"] for suite in result["suites"])
        if actual_processes != expected_processes:
            fail(
                f"{floor} suite replication mismatch: "
                f"expected {expected_processes}, got {dict(actual_processes)}"
            )

        cases = suite_cases(result)
        macro = {case["name"]: case for case in result["macro"]}
        for category, required in performance["categories"].items():
            for case in required:
                if case not in cases and case not in macro:
                    fail(f"{floor} result lacks {category} case {case}")
        for name, case in macro.items():
            expected_reps = (
                methodology["heavy_macro_repetitions"]
                if name in {"compile.stage1-stdlib", "docgen.full"}
                else methodology["macro_repetitions"]
            )
            if case["reps"] != expected_reps or case["peak_rss_bytes"] <= 0:
                fail(f"{floor} macro evidence is unusable for {name}")
        results.append(result)

    expected_crossings = {
        "query.aggregate": (1475835, 1562867),
        "compile.stage1-stdlib": (39818, 44564),
    }
    for name, (zig_expected, rust_expected) in expected_crossings.items():
        values: list[int] = []
        for result in results:
            macro = {case["name"]: case for case in result["macro"]}
            if name in macro:
                values.append(macro[name]["wall_us_median"])
                continue
            samples = sorted(
                case["ns_per_op"]
                for suite in result["suites"]
                for case in suite["data"]["cases"]
                if case["name"] == name
            )
            values.append(samples[len(samples) // 2])
        if values != [zig_expected, rust_expected]:
            fail(f"accepted R6 crossing {name} drifted: got {values}")


def verify_artifact_baseline(manifest: dict) -> None:
    contract = manifest["performance"]["artifact_inspector"]
    baseline = load_json(ROOT / contract["path"])
    if baseline.get("mode") != "ReleaseSafe":
        fail("artifact-inspector baseline is not ReleaseSafe")
    rung = baseline.get("rung", {})
    if (
        rung.get("name") != "artifact-inspector"
        or rung.get("implementation") != "zig"
        or len(rung.get("sha256", "")) != 64
    ):
        fail("artifact-inspector baseline has no exact Zig rung identity")
    acceptance = baseline.get("acceptance", {})
    if (
        acceptance.get("time_variance_limit_percent")
        != contract["time_variance_limit_percent"]
        or acceptance.get("rss_variance_limit_percent")
        != contract["rss_variance_limit_percent"]
    ):
        fail("artifact-inspector acceptance limits drifted")
    runs = baseline.get("runs", [])
    if len(runs) != contract["runs"]:
        fail("artifact-inspector baseline does not contain exactly two runs")
    for run in runs:
        if (
            run.get("case") != contract["case"]
            or run.get("iterations") != contract["iterations_per_run"]
            or run.get("elapsed_ns", 0) <= 0
            or run.get("ns_per_inspection", 0) <= 0
            or run.get("peak_rss_bytes", 0) <= 0
        ):
            fail("artifact-inspector run is missing usable time or RSS evidence")
    observed = baseline.get("observed", {})
    if (
        observed.get("time_variance_percent", float("inf"))
        > contract["time_variance_limit_percent"]
        or observed.get("rss_variance_percent", float("inf"))
        > contract["rss_variance_limit_percent"]
    ):
        fail("artifact-inspector clean-process runs exceed frozen variance limits")


def main() -> None:
    manifest = load_json(MANIFEST)
    if manifest.get("schema_version") != 1:
        fail("unknown manifest schema")
    verify_semantics(manifest)
    verify_design46(manifest)
    verify_artifact_baseline(manifest)
    print("native-rung-baseline: semantic fixtures and optimized evidence passed")


if __name__ == "__main__":
    main()
