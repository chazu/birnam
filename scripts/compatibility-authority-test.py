#!/usr/bin/env python3
"""The reviewed image-admission addition must not loosen any other authority."""
from pathlib import Path
import subprocess
import unittest

from compatibility_authority import unchanged_authority

ROOT = Path(__file__).resolve().parent.parent
IMAGE = "rust/birnam-image/src/lib.rs"


class CompatibilityAuthorityTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.baseline = subprocess.check_output(["git", "show", "ff71ec23:" + IMAGE], cwd=ROOT)
        cls.current = (ROOT / IMAGE).read_bytes()

    def test_original_and_reviewed_addition_are_admitted(self):
        self.assertTrue(unchanged_authority(IMAGE, self.baseline, self.baseline))
        self.assertTrue(unchanged_authority(IMAGE, self.current, self.baseline))

    def test_old_rejections_and_unrelated_image_code_remain_frozen(self):
        for mutated in (
            self.current.replace(b"            ABORT_ACTION_NAME,\n", b"", 1),
            self.current + b"\n// unrelated change\n",
            self.current.replace(b"ImageRejectionKind::StepAction", b"ImageRejectionKind::Machine", 1),
        ):
            with self.subTest(mutation=mutated[-50:]):
                self.assertFalse(unchanged_authority(IMAGE, mutated, self.baseline))

    def test_partial_or_altered_new_admission_is_rejected(self):
        for mutated in (
            self.current.replace(b"            EXIT_BOUNDARY_ACTION_NAME,\n", b"", 1),
            self.current.replace(b'"BoundaryCallAction\\0"', b'"OtherAction\\0"', 1),
        ):
            self.assertFalse(unchanged_authority(IMAGE, mutated, self.baseline))

    def test_exception_does_not_apply_to_other_contracts(self):
        self.assertFalse(unchanged_authority("rust/birnam-image/src/v8.rs", self.current, self.baseline))


if __name__ == "__main__":
    unittest.main()
