#!/usr/bin/env python3
"""Regression checks for diagnostic accounting (no runtime timing assertions)."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "diagnostic", Path(__file__).with_name("run-allocation-diagnostic.py"))
diagnostic = importlib.util.module_from_spec(spec)
spec.loader.exec_module(diagnostic)


class CounterTests(unittest.TestCase):
    def summarize(self, contents):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "events"
            path.write_text(contents)
            return diagnostic.summarize(path)

    def test_boundaries_exclude_startup_and_reporting(self):
        result = self.summarize("""D66 integer 100
D66 gc 1
D66 arena 128
D66 operand 1
D66 integer 1
D66 integer 1
D66 capture_words 0
D66 capture_words 2
D66 initial_pairs 3
D66 gc 1
D66 arena 256
D66 integer 999
""")
        self.assertEqual(result["integer"], 2)
        self.assertEqual(result["non_operand_integers"], 1)
        self.assertEqual(result["capture_words_allocations"], 1)
        self.assertEqual(result["initial_pairs_allocations"], 1)
        self.assertEqual(result["arena_bytes"], 128)
        self.assertEqual(result["after_arena_bytes"], 256)

    def test_uninstrumented_and_inconsistent_logs_fail(self):
        with self.assertRaises(RuntimeError):
            self.summarize("D66 gc 1\n")
        with self.assertRaises(RuntimeError):
            self.summarize("D66 gc 1\nD66 operand 2\nD66 integer 1\nD66 gc 1\n")

    def test_automatic_collection_does_not_reset_workload_counts(self):
        result = self.summarize("D66 gc 1\nD66 arena 0\nD66 integer 5\n"
                                "D66 gc 1\nD66 arena 128\nD66 integer 7\n"
                                "D66 gc 1\nD66 arena 0\nD66 integer 999\n")
        self.assertEqual(result["integer"], 12)
        self.assertEqual(result["collections_within_region"], 1)
        self.assertEqual(result["observed_max_arena_bytes"], 128)
        self.assertEqual(result["after_arena_bytes"], 0)


if __name__ == "__main__":
    unittest.main()
