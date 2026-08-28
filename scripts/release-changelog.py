#!/usr/bin/env python3
"""Promote release notes while keeping the next Unreleased section ready."""

import pathlib
import re
import sys


def fail(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(1)


def main() -> None:
    if len(sys.argv) != 4:
        fail("usage: release-changelog.py CHANGELOG VERSION YYYY-MM-DD")

    path = pathlib.Path(sys.argv[1])
    version = sys.argv[2]
    date = sys.argv[3]
    body = path.read_text(encoding="utf-8")
    version_heading = re.compile(
        rf"^## \[{re.escape(version)}\](?:[ \t]+.*)?$", re.MULTILINE
    )
    unreleased_heading = re.compile(
        r"^## \[Unreleased\](?:[ \t]+.*)?$", re.MULTILINE
    )
    version_matches = list(version_heading.finditer(body))
    unreleased_matches = list(unreleased_heading.finditer(body))

    if len(version_matches) > 1 or len(unreleased_matches) > 1:
        fail("CHANGELOG.md contains duplicate release section headings")

    if unreleased_matches:
        if version_matches:
            fail(f"CHANGELOG.md contains both '## [Unreleased]' and '## [{version}]'")
        match = unreleased_matches[0]
        replacement = f"## [Unreleased]\n\n## [{version}] — {date}"
        body = body[: match.start()] + replacement + body[match.end() :]
        action = f"promoted [Unreleased] -> [{version}] and opened the next [Unreleased] section"
    elif version_matches:
        match = version_matches[0]
        body = body[: match.start()] + "## [Unreleased]\n\n" + body[match.start() :]
        action = f"opened [Unreleased] above existing [{version}]"
    else:
        fail(
            f"CHANGELOG.md has no '## [{version}]' section and no '## [Unreleased]' to promote"
        )

    path.write_text(body, encoding="utf-8")
    print(action)


if __name__ == "__main__":
    main()
