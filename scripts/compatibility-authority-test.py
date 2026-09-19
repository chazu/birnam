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

    def test_integer_migration_preserves_operands_checks_and_inherited_recognition(self):
        for before, after in (
            (b'(birnam_abi::integer::make)(basis as i64)', b'(birnam_abi::integer::make)(oldest as i64)'),
            (b'(birnam_abi::integer::value)(object)', b'(birnam_abi::integer::value)(vtable)'),
            (b'(birnam_abi::integer::is_integer)(size)', b'(api.isSmallInt)(size)'),
            (b'vtable_inherits_from(api, vtable, birnam_abi::integer::behavior())', b'vtable == birnam_abi::integer::behavior()'),
            (b'birnam_abi::integer_dependency!();', b''),
        ):
            self.assertIn(before, self.current)
            self.assertFalse(unchanged_authority(IMAGE, self.current.replace(before, after, 1), self.baseline))

    def test_float_migration_preserves_bits_and_inherited_recognition(self):
        for before, after in (
            (b'(birnam_abi::float::value)(object)', b'(birnam_abi::float::value)(vtable)'),
            (b'vtable_inherits_from(api, vtable, birnam_abi::float::behavior())',
             b'vtable == birnam_abi::float::behavior()'),
            (b'birnam_abi::float_dependency!();', b''),
            (b'(birnam_abi::float::make)', b'(api.mkFloat)'),
        ):
            self.assertIn(before, self.current)
            self.assertFalse(unchanged_authority(IMAGE, self.current.replace(before, after, 1), self.baseline))

    def test_word_migration_preserves_payload_and_inherited_recognition(self):
        for before, after in (
            (b'(birnam_abi::word::value)(object)', b'(birnam_abi::word::value)(vtable)'),
            (b'vtable_inherits_from(api, vtable, birnam_abi::word::behavior())',
             b'vtable == birnam_abi::word::behavior()'),
            (b'birnam_abi::word_dependency!();', b''),
            (b'(birnam_abi::word::make)(value)', b'(birnam_abi::word::make)(value >> 1)'),
        ):
            self.assertIn(before, self.current)
            self.assertFalse(unchanged_authority(IMAGE, self.current.replace(before, after, 1), self.baseline))

    def test_string_migration_preserves_bytes_and_inherited_recognition(self):
        for before, after in (
            (b'(birnam_abi::string::bytes)(object, &mut length)', b'(birnam_abi::string::bytes)(vtable, &mut length)'),
            (b'vtable_inherits_from(api, vtable, birnam_abi::string::behavior())', b'vtable == birnam_abi::string::behavior()'),
            (b'birnam_abi::string_dependency!();', b''),
            (b'(birnam_abi::string::make)(bytes.as_ptr(), bytes.len())', b'(birnam_abi::string::make)(bytes.as_ptr(), 0)'),
        ):
            self.assertIn(before, self.current)
            self.assertFalse(unchanged_authority(IMAGE, self.current.replace(before, after, 1), self.baseline))

    def test_namespace_names_preserve_operands_and_complete_migration(self):
        for before, after in (
            (b'(birnam_abi::namespace_names::name_of)(object)', b'(birnam_abi::namespace_names::name_of)(vtable)'),
            (b'(birnam_abi::namespace_names::name_of)(parent)', b'(api.exportedNameOf)(parent)'),
            (b'birnam_abi::namespace_names_dependency!();', b''),
        ):
            self.assertIn(before, self.current)
            self.assertFalse(unchanged_authority(IMAGE, self.current.replace(before, after, 1), self.baseline))

    def test_exception_does_not_apply_to_other_contracts(self):
        self.assertFalse(unchanged_authority("rust/birnam-image/src/v8.rs", self.current, self.baseline))

    def test_only_complete_owner_parent_call_migration_is_admitted(self):
        for before, after in (
            (b'birnam_abi::vtable_parent(api, vtable)', b'birnam_abi::vtable_parent(api, object)'),
            (b'birnam_abi::vtable_owner(api, vtable)', b'birnam_abi::vtable_parent(api, vtable)'),
            (b'birnam_abi::set_vtable_owner(api_ref, report_vt, object)',
             b'birnam_abi::set_vtable_owner(api_ref, object, report_vt)'),
            (b'birnam_abi::vtable_owner(api, vtable)', b'(api.vtableOwner)(vtable)'),
        ):
            self.assertIn(before, self.current)
            self.assertFalse(unchanged_authority(
                IMAGE, self.current.replace(before, after, 1), self.baseline))


if __name__ == "__main__":
    unittest.main()
