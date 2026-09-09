#!/usr/bin/env python3
import importlib.util
import pathlib
import unittest


PATH = pathlib.Path(__file__).with_name("v015-performance-acceptance.py")
SPEC = importlib.util.spec_from_file_location("v015_performance_acceptance", PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


def document(raws: list[float]) -> dict:
    return {
        "suites": [
            {
                "label": "micro",
                "run": index + 1,
                "data": {"cases": [{"name": MODULE.RAW_BOUNDED_CASE, "ns_per_op_raw": raw, "net_us": 1, "ops": 1}]},
            }
            for index, raw in enumerate(raws)
        ],
        "macro": [],
    }


CLEAN = "ok   sends.unary: 1 -> 1 (+0.0%)\nregressions: 0, missing: 0, unusable: 0\n"
FLAGGED = (
    "REG  vector.includes-miss: 155769 -> 173546 (+11.4%)\n"
    "ILL  map.at-hit: unusable value(s)\n"
    "regressions: 1, missing: 0, unusable: 1\n"
)
TWO_FLAGS = "REG  vector.includes-miss: 1 -> 2 (+100.0%)\nREG  sends.unary: 1 -> 2 (+100.0%)\nregressions: 2, missing: 0, unusable: 0\n"
MISSING = "MISSING    boot.image (not in new results, not retired)\nregressions: 0, missing: 1, unusable: 0\n"


class V015PerformanceAcceptanceTest(unittest.TestCase):
    def test_accepts_the_single_flag_when_raw_is_within_allowance(self) -> None:
        baseline = document([222291, 235195, 217273])
        candidate = document([195571, 208295, 209580])
        summary = MODULE.judge(FLAGGED, 1, baseline, candidate)
        self.assertIn("raw median 208295", summary)

    def test_rejects_the_single_flag_when_raw_exceeds_allowance(self) -> None:
        baseline = document([222291, 235195, 217273])
        candidate = document([250000, 251000, 252000])
        with self.assertRaises(MODULE.Rejected):
            MODULE.judge(FLAGGED, 1, baseline, candidate)

    def test_rejects_any_other_regression(self) -> None:
        baseline = document([1000])
        candidate = document([1000])
        with self.assertRaises(MODULE.Rejected) as raised:
            MODULE.judge(TWO_FLAGS, 1, baseline, candidate)
        self.assertIn("sends.unary", str(raised.exception))

    def test_rejects_missing_cases(self) -> None:
        with self.assertRaises(MODULE.Rejected):
            MODULE.judge(MISSING, 1, document([1]), document([1]))

    def test_clean_comparator_passes_without_override(self) -> None:
        self.assertIn("no raw-bounded override", MODULE.judge(CLEAN, 0, document([1]), document([1])))

    def test_incomplete_raw_measurements_fail_closed(self) -> None:
        with self.assertRaises(MODULE.Rejected):
            MODULE.judge(FLAGGED, 1, document([222291]), document([0]))


if __name__ == "__main__":
    unittest.main()
