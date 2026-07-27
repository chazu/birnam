#!/usr/bin/env python3
"""Fail closed when Rust-host message/global callsites drift from Z2.0."""

from __future__ import annotations

import json
import hashlib
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


def send_surface(lines: list[str], callsites: list[int]) -> str:
    source = "\n".join(lines) + "\n"
    offsets: list[int] = []
    offset = 0
    for line in lines:
        offsets.append(offset)
        offset += len(line) + 1

    details: list[str] = []
    for line in callsites:
        position = source.find("send", offsets[line - 1])
        opening = source.find("(", position)
        if position < 0 or opening < 0:
            fail(f"cannot parse send callsite at line {line}")
        depth = 0
        closing = None
        for index in range(opening, len(source)):
            if source[index] in "([{":
                depth += 1
            elif source[index] in ")]}":
                depth -= 1
                if depth == 0:
                    closing = index
                    break
        if closing is None:
            fail(f"unterminated send callsite at line {line}")

        parts: list[str] = []
        current = ""
        nested = 0
        for character in source[opening + 1 : closing]:
            if character in "([{":
                nested += 1
            elif character in ")]}":
                nested -= 1
            if character == "," and nested == 0:
                if current.strip():
                    parts.append(current.strip())
                current = ""
            else:
                current += character
        if current.strip():
            parts.append(current.strip())
        if len(parts) < 3:
            fail(f"send callsite at line {line} has fewer than three arguments")

        function = "?"
        for source_line in range(line - 1, -1, -1):
            match = re.search(r"\bfn\s+([A-Za-z0-9_]+)", lines[source_line])
            if match:
                function = match.group(1)
                break
        normalize = lambda value: re.sub(r"\s+", " ", value)
        arity = parts[-1] if re.fullmatch(r"\d+L?", parts[-1]) else "unknown"
        details.append(
            f"{line}|{function}|{normalize(parts[0])}|{normalize(parts[1])}|{arity}"
        )
    payload = ("\n".join(details) + "\n").encode()
    return hashlib.sha256(payload).hexdigest()


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
    actual_surface_digest = send_surface(lines, actual_send)
    if actual_surface_digest != manifest.get("send_callsite_surface_sha256"):
        fail(
            "send receiver/selector/arity surface drifted; "
            f"expected={manifest.get('send_callsite_surface_sha256')}, "
            f"actual={actual_surface_digest}"
        )

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

    groups = manifest.get("send_function_groups")
    if not isinstance(groups, list) or not groups:
        fail("send_function_groups must be a non-empty array")
    group_functions: dict[str, str] = {}
    for group in groups:
        if not isinstance(group, dict) or not isinstance(group.get("id"), str):
            fail("every send function group needs an id")
        functions = group.get("functions")
        if not isinstance(functions, list) or not functions:
            fail(f"send function group {group.get('id')!r} has no functions")
        for field in ("actual_owner", "effect_class", "test", "disposition"):
            if not isinstance(group.get(field), str) or not group[field].strip():
                fail(f"send function group {group.get('id')!r} has no {field}")
        for function in functions:
            if not isinstance(function, str) or not function:
                fail(f"send function group {group['id']!r} has an invalid function")
            if function in group_functions:
                fail(
                    f"send function {function} appears in both "
                    f"{group_functions[function]} and {group['id']}"
                )
            group_functions[function] = group["id"]

    actual_functions: set[str] = set()
    for line in actual_send:
        for source_line in range(line - 1, -1, -1):
            match = re.search(r"\bfn\s+([A-Za-z0-9_]+)", lines[source_line])
            if match:
                actual_functions.add(match.group(1))
                break
    if set(group_functions) != actual_functions:
        fail(
            "send function review coverage drifted; "
            f"missing={sorted(actual_functions - set(group_functions))}, "
            f"stale={sorted(set(group_functions) - actual_functions)}"
        )

    print(
        "rust-host-message-owner-inventory: ok "
        f"send_calls={len(actual_send)} named_global_lookups={len(actual_globals)} "
        f"owner_classes={len(owners)} reviewed_functions={len(actual_functions)} "
        "disposition=read-only"
    )


if __name__ == "__main__":
    main()
