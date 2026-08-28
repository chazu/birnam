#!/usr/bin/env python3
import importlib.util
import pathlib
import unittest


PATH = pathlib.Path(__file__).with_name("v014-performance-acceptance.py")
SPEC = importlib.util.spec_from_file_location("v014_performance_acceptance", PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


def candidate() -> dict:
    suites = {}
    macros = []
    for name, (label, value) in MODULE.REFERENCE.items():
        if label == "macro":
            macros.append({"name": name, "wall_us_median": value})
            continue
        suites.setdefault(label, []).append(
            {
                "name": name,
                "net_us": value / 1000,
                "ops": 1,
                "ns_per_op_raw": MODULE.RAW_REFERENCE.get(name, max(abs(value), 1)),
                "median_us": max(abs(value) / 1000, 1),
            }
        )
    return {
        "stamp": {
            "version": "0.14.0",
            "machine": "Mac15,12",
            "floor": "rust",
            "mode": "release",
            "quick": False,
            "dirty": False,
            "floor_sha256": MODULE.FLOOR_SHA256,
        },
        "suites": [
            {"label": label, "run": 1, "data": {"cases": cases}}
            for label, cases in suites.items()
        ],
        "macro": macros,
    }


class V014PerformanceAcceptanceTest(unittest.TestCase):
    def test_accepts_the_recorded_candidate_boundary(self) -> None:
        MODULE.validate_candidate(candidate(), "Mac15,12")

    def test_rejects_a_case_above_its_noise_bounded_ceiling(self) -> None:
        document = candidate()
        case = next(
            case
            for suite in document["suites"]
            for case in suite["data"]["cases"]
            if case["name"] == "sends.unary"
        )
        case["net_us"] *= 1.101
        with self.assertRaisesRegex(MODULE.Rejected, "sends.unary.*exceeds accepted ceiling"):
            MODULE.validate_candidate(document, "Mac15,12")

    def test_rejects_a_missing_case(self) -> None:
        document = candidate()
        document["macro"].pop()
        with self.assertRaisesRegex(MODULE.Rejected, "case set differs"):
            MODULE.validate_candidate(document, "Mac15,12")

    def test_rejects_a_raw_bounded_case_above_its_ceiling(self) -> None:
        document = candidate()
        case = next(
            case
            for suite in document["suites"]
            for case in suite["data"]["cases"]
            if case["name"] == "error.propagate-5-deep"
        )
        case["ns_per_op_raw"] *= 1.101
        with self.assertRaisesRegex(MODULE.Rejected, "error.propagate-5-deep.*exceeds accepted raw ceiling"):
            MODULE.validate_candidate(document, "Mac15,12")

    def test_rejects_a_run_bounded_case_above_its_ceiling(self) -> None:
        document = candidate()
        case = next(
            case
            for suite in document["suites"]
            for case in suite["data"]["cases"]
            if case["name"] == "image.save"
        )
        case["net_us"] = MODULE.MAX_RUN_REFERENCE["image.save"] * 1.051 / 1000
        with self.assertRaisesRegex(MODULE.Rejected, "image.save.*exceeds accepted run ceiling"):
            MODULE.validate_candidate(document, "Mac15,12")

    def test_quarantine_requires_real_raw_measurements(self) -> None:
        document = candidate()
        case = next(
            case
            for suite in document["suites"]
            for case in suite["data"]["cases"]
            if case["name"] == MODULE.QUARANTINED_NET_CASE
        )
        case["ns_per_op_raw"] = 0
        with self.assertRaisesRegex(MODULE.Rejected, "complete positive raw measurements"):
            MODULE.validate_candidate(document, "Mac15,12")


if __name__ == "__main__":
    unittest.main()
