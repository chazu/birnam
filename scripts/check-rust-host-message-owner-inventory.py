#!/usr/bin/env python3
"""Fail closed when Rust-host message/global callsites drift from Z2.0."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


REPO = Path(__file__).resolve().parent.parent
MANIFEST = REPO / "abi/rust-host-message-owner-v1.json"
SOURCE = REPO / "rust/birnam-host/src/lib.rs"
SEND_MARKERS = ("api.send", "stack.api().send")


def fail(message: str) -> None:
    print(f"rust-host-message-owner-inventory: {message}", file=sys.stderr)
    raise SystemExit(1)


def main() -> None:
    try:
        manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
        lines = SOURCE.read_text(encoding="utf-8").splitlines()
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"cannot read inventory or source: {exc}")

    if manifest.get("schema_version") != 1:
        fail("unsupported schema version")
    if manifest.get("source") != str(SOURCE.relative_to(REPO)):
        fail("manifest source does not name rust/birnam-host/src/lib.rs")

    recorded_send = manifest.get("send_lines")
    if not isinstance(recorded_send, list) or any(
        not isinstance(line, int) or line < 1 for line in recorded_send
    ):
        fail("send_lines must be positive integers")
    if len(recorded_send) != len(set(recorded_send)):
        fail("send line appears more than once")

    actual_send = sorted(
        index
        for index, text in enumerate(lines, start=1)
        if any(marker in text for marker in SEND_MARKERS)
    )
    if recorded_send != actual_send:
        fail(f"send callsite coverage drifted; expected={actual_send}, recorded={recorded_send}")

    recorded_globals = manifest.get("global_lookups")
    if not isinstance(recorded_globals, dict):
        fail("global_lookups must be an object keyed by source line")
    try:
        global_lines = sorted(int(line) for line in recorded_globals)
    except ValueError:
        fail("global_lookups keys must be integer source lines")
    if len(global_lines) != len(set(global_lines)):
        fail("global lookup line appears more than once")

    actual_globals: dict[int, str] = {}
    for index, text in enumerate(lines, start=1):
        if "global_named(" not in text:
            continue
        match = re.search(r'global_named\([^,]+,\s*"([^"]+)"\)', text)
        if match:
            actual_globals[index] = match.group(1)
    if sorted(actual_globals) != global_lines:
        fail(
            "named global coverage drifted; "
            f"expected={sorted(actual_globals)}, recorded={global_lines}"
        )
    normalized_recorded = {int(line): name for line, name in recorded_globals.items()}
    if normalized_recorded != actual_globals:
        fail(
            "named global names drifted; "
            f"expected={actual_globals}, recorded={normalized_recorded}"
        )

    owners = manifest.get("global_owner_classes")
    if not isinstance(owners, dict):
        fail("global_owner_classes must be an object")
    missing_owners = sorted(set(actual_globals.values()) - set(owners))
    if missing_owners:
        fail(f"named globals have no owner class: {missing_owners}")

    print(
        "rust-host-message-owner-inventory: ok "
        f"send_calls={len(actual_send)} named_global_lookups={len(actual_globals)} "
        f"owner_classes={len(owners)} disposition=read-only"
    )


if __name__ == "__main__":
    main()
