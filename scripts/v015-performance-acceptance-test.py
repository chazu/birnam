#!/usr/bin/env python3
import importlib.util
import pathlib
import unittest


PATH = pathlib.Path(__file__).with_name("v015-performance-acceptance.py")
SPEC = importlib.util.spec_from_file_location("v015_performance_acceptance", PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


def document(cases: dict, layer: str = "micro") -> dict:
    """One suite process per sample index; `cases` maps name -> raw samples."""
    runs = max(len(samples) for samples in cases.values())
    return {
        "suites": [
            {
                "label": layer,
                "run": index + 1,
                "data": {
                    "cases": [
                        {"name": name, "ns_per_op_raw": samples[index], "net_us": 1, "ops": 1}
                        for name, samples in cases.items()
                        if index < len(samples)
                    ]
                },
            }
            for index in range(runs)
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
ATTEMPT_THREE = (
    "REG  string.split-native: 2789578 -> 3070921 (+10.0%)\n"
    "REG  map.at-hit: 168 -> 301 (+78.9%)\n"
    "REG  map.at-miss: 254 -> 352 (+38.4%)\n"
    "ILL  ffi.wrapped-call: unusable value(s)\n"
    "REG  vector.reversed: 1937546 -> 2182625 (+12.6%)\n"
    "regressions: 4, missing: 0, unusable: 1\n"
)


class V015PerformanceAcceptanceTest(unittest.TestCase):
    def test_accepts_a_micro_flag_when_raw_is_within_allowance(self) -> None:
        baseline = document({"vector.includes-miss": [222291, 235195, 217273]})
        candidate = document({"vector.includes-miss": [195571, 208295, 209580]})
        summary = MODULE.judge(FLAGGED, 1, baseline, candidate)
        self.assertIn("raw median 208295", summary)

    def test_accepts_a_micro_flag_whose_raw_delta_is_inside_the_run_spread(self) -> None:
        baseline = document({"vector.includes-miss": [2002250, 1981625, 2078156]})
        candidate = document({"vector.includes-miss": [2230406, 2490171, 1857593]})
        summary = MODULE.judge(FLAGGED, 1, baseline, candidate)
        self.assertIn("inside the run-to-run raw spread 632578", summary)

    def test_rejects_a_micro_flag_when_raw_exceeds_allowance_and_spread(self) -> None:
        baseline = document({"vector.includes-miss": [222291, 235195, 217273]})
        candidate = document({"vector.includes-miss": [250000, 251000, 252000]})
        with self.assertRaises(MODULE.Rejected) as raised:
            MODULE.judge(FLAGGED, 1, baseline, candidate)
        self.assertIn("exceeds the run-to-run spread", str(raised.exception))

    def test_judges_every_micro_flag_independently(self) -> None:
        baseline = document({"vector.includes-miss": [1000, 1000, 1000], "sends.unary": [1000, 1000, 1000]})
        candidate = document({"vector.includes-miss": [1050, 1050, 1050], "sends.unary": [2000, 2000, 2000]})
        with self.assertRaises(MODULE.Rejected) as raised:
            MODULE.judge(TWO_FLAGS, 1, baseline, candidate)
        self.assertIn("sends.unary", str(raised.exception))
        candidate = document({"vector.includes-miss": [1050, 1050, 1050], "sends.unary": [1090, 1090, 1090]})
        summary = MODULE.judge(TWO_FLAGS, 1, baseline, candidate)
        self.assertIn("vector.includes-miss: raw median 1050", summary)
        self.assertIn("sends.unary: raw median 1090", summary)

    def test_rejects_any_regression_outside_the_micro_layer(self) -> None:
        baseline = document({"vector.includes-miss": [1000, 1000, 1000], "sends.unary": [1000]}, layer="relational")
        candidate = document({"vector.includes-miss": [1000, 1000, 1000], "sends.unary": [1000]}, layer="relational")
        with self.assertRaises(MODULE.Rejected) as raised:
            MODULE.judge(TWO_FLAGS, 1, baseline, candidate)
        self.assertIn("outside the micro layer", str(raised.exception))

    def test_rejects_a_flag_the_candidate_carries_in_two_layers(self) -> None:
        candidate = document({"vector.includes-miss": [1000]})
        candidate["suites"].append(document({"vector.includes-miss": [1000]}, layer="gc")["suites"][0])
        with self.assertRaises(MODULE.Rejected):
            MODULE.judge(FLAGGED, 1, document({"vector.includes-miss": [1000]}), candidate)

    def test_accepts_the_retained_attempt_three_flags(self) -> None:
        baseline = document(
            {
                "string.split-native": [2839234, 2828140, 2898875],
                "map.at-hit": [8649, 7769, 7831],
                "map.at-miss": [8373, 7839, 8082],
                "vector.reversed": [2002250, 1981625, 2078156],
            }
        )
        candidate = document(
            {
                "string.split-native": [3198640, 3108562, 2817484],
                "map.at-hit": [5442, 5601, 5675],
                "map.at-miss": [5447, 5109, 5702],
                "vector.reversed": [2230406, 2490171, 1857593],
            }
        )
        summary = MODULE.judge(ATTEMPT_THREE, 1, baseline, candidate)
        self.assertEqual(summary.count("\n  "), 4)
        self.assertIn("vector.reversed: raw delta 228156", summary)

    def test_rejects_missing_cases(self) -> None:
        with self.assertRaises(MODULE.Rejected):
            MODULE.judge(MISSING, 1, document({"x": [1]}), document({"x": [1]}))

    def test_clean_comparator_passes_without_override(self) -> None:
        self.assertIn("no raw-bounded override", MODULE.judge(CLEAN, 0, document({"x": [1]}), document({"x": [1]})))

    def test_incomplete_raw_measurements_fail_closed(self) -> None:
        with self.assertRaises(MODULE.Rejected):
            MODULE.judge(FLAGGED, 1, document({"vector.includes-miss": [222291]}), document({"vector.includes-miss": [0]}))


if __name__ == "__main__":
    unittest.main()
