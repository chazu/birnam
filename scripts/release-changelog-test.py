#!/usr/bin/env python3
import pathlib
import subprocess
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parent.parent
PROMOTER = ROOT / "scripts" / "release-changelog.py"


class ReleaseChangelogTest(unittest.TestCase):
    def promote(self, body: str, version: str = "0.14.0") -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as directory:
            changelog = pathlib.Path(directory) / "CHANGELOG.md"
            changelog.write_text(body, encoding="utf-8")
            result = subprocess.run(
                ["python3", str(PROMOTER), str(changelog), version, "2026-08-27"],
                check=False,
                text=True,
                capture_output=True,
            )
            result.changelog = changelog.read_text(encoding="utf-8")
            return result

    def test_promotes_draft_and_leaves_next_draft_header(self) -> None:
        result = self.promote(
            "# Changelog\n\nIntro.\n\n## [Unreleased]\n\n### Added\n\n- Stackless execution.\n\n## [0.13.0] — 2026-08-04\n"
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.changelog.count("## [Unreleased]"), 1)
        self.assertIn(
            "## [Unreleased]\n\n## [0.14.0] — 2026-08-27\n\n### Added\n\n- Stackless execution.",
            result.changelog,
        )

    def test_existing_version_gets_a_fresh_draft_header(self) -> None:
        result = self.promote(
            "# Changelog\n\nIntro.\n\n## [0.14.0] — 2026-08-27\n\n- Notes.\n"
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(
            "Intro.\n\n## [Unreleased]\n\n## [0.14.0] — 2026-08-27",
            result.changelog,
        )

    def test_rejects_a_changelog_without_release_notes(self) -> None:
        original = "# Changelog\n\n## [0.13.0] — 2026-08-04\n"
        result = self.promote(original)

        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(result.changelog, original)
        self.assertIn("no '## [0.14.0]' section and no '## [Unreleased]'", result.stderr)


if __name__ == "__main__":
    unittest.main()
