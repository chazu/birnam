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
FUNCTION_RE = re.compile(
    r'^\s*(?:(?:pub(?:\([^)]*\))?|unsafe|async|const|extern(?:\s+"[^"]+")?)\s+)*'
    r'fn\s+([A-Za-z0-9_]+)'
)


def fail(message: str) -> None:
    print(f"rust-host-message-owner-inventory: {message}", file=sys.stderr)
    raise SystemExit(1)


def function_region(lines: list[str], line: int) -> tuple[str, int, int]:
    function = "?"
    start = 0
    for index in range(line - 1, -1, -1):
        match = FUNCTION_RE.search(lines[index])
        if match:
            function = match.group(1)
            start = index
            break
    end = len(lines)
    depth = 0
    body_started = False
    for index in range(start, len(lines)):
        code = re.sub(r'"(?:\\.|[^"\\])*"', "", lines[index])
        code = re.sub(r"'(?:\\.|[^'\\])'", "", code)
        code = code.split("//", 1)[0]
        depth += code.count("{") - code.count("}")
        if "{" in code:
            body_started = True
        if body_started and depth == 0:
            end = index + 1
            break
    return function, start, end


def send_extent(lines: list[str], line: int) -> tuple[str, int, int, int, int]:
    source = "\n".join(lines) + "\n"
    offsets: list[int] = []
    offset = 0
    for source_line in lines:
        offsets.append(offset)
        offset += len(source_line) + 1

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
    closing_line = source.count("\n", 0, closing) + 1
    return source, position, opening, closing_line, closing


def send_parts(lines: list[str], line: int) -> list[str]:
    source, _, opening, _, closing = send_extent(lines, line)

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
    return parts


def resolved_selector(lines: list[str], line: int, expression: str) -> str:
    direct = re.search(
        r'\((?:self\.stack\.api\(\)|self\.api|api)\.intern\)\(c"([^"]*)"\.as_ptr\(\)',
        expression,
    )
    if direct:
        return direct.group(1)
    _, start, _ = function_region(lines, line)
    for index in range(line - 2, start - 1, -1):
        match = re.search(
            r"\b(?:let|const)\s+([A-Za-z0-9_]+).*?"
            r"\((?:self\.stack\.api\(\)|self\.api|api)\.intern\)"
            r'\(c"([^"]*)"\.as_ptr\(\)',
            lines[index],
        )
        if match and match.group(1) == expression:
            return match.group(2)
    return f"dynamic:{expression}"


def send_callsite(lines: list[str], line: int) -> dict[str, object]:
    parts = send_parts(lines, line)
    function, _, _ = function_region(lines, line)
    normalize = lambda value: re.sub(r"\s+", " ", value)
    selector_binding = normalize(parts[1])
    selector_literal = resolved_selector(lines, line, selector_binding)
    arity = int(parts[-1].rstrip("L")) if re.fullmatch(r"\d+L?", parts[-1]) else "dynamic"
    behavior = observed_behavior(lines, line)
    return {
        "line": line,
        "function": function,
        "receiver_expression": normalize(parts[0]),
        "selector_binding": selector_binding,
        "selector_literal": selector_literal,
        "arity": arity,
        **behavior,
    }


def observed_behavior(lines: list[str], line: int) -> dict[str, object]:
    source, position, _, closing_line, closing = send_extent(lines, line)
    function, start, end = function_region(lines, line)
    prefix = source[max(0, position - 240) : position]
    binding = None
    binding_matches = list(
        re.finditer(
            r"(?:let|const)\s+(?:mut\s+)?([A-Za-z0-9_]+)\s*=\s*unsafe\s*\{",
            prefix,
            flags=re.MULTILINE,
        )
    )
    # A few recovery paths deliberately reassign an existing binding rather
    # than introducing a new `let`. Capture those assignments as retained
    # send results too, while keeping selector/helper assignments out of the
    # result classification by requiring the unsafe block to start here.
    binding_matches.extend(
        re.finditer(
            r"(?<![A-Za-z0-9_])([A-Za-z_][A-Za-z0-9_]*)\s*=\s*unsafe\s*\{",
            prefix,
            flags=re.MULTILINE,
        )
    )
    for match in reversed(binding_matches):
        tail = prefix[match.end() :]
        if "}" not in tail:
            binding = match.group(1)
            break
    condition = False
    for match in re.finditer(r"\b(?:if|while)\s+unsafe\s*\{", prefix, re.MULTILINE):
        tail = prefix[match.end() :]
        if "}" not in tail:
            condition = True
            break
    after = source[closing + 1 : closing + 160]
    comparison = bool(re.match(r"\s*==", after))
    returned = bool(
        re.search(r"(?:Ok|Some|return)\s*\(\s*unsafe\s*\{[^}]*$", prefix)
        or re.search(r"\|[^\n]*\|\s*unsafe\s*\{[^}]*$", prefix)
    )
    if binding is not None:
        handling = f"bound:{binding}"
    elif condition or comparison:
        handling = "conditional"
    elif returned:
        handling = "returned"
    else:
        handling = "ignored"

    post_lines: list[int] = []
    post_snippets: list[str] = []
    if binding is not None:
        pattern = re.compile(rf"\b{re.escape(binding)}\b")
        assignment = re.compile(
            rf"(?:\b(?:let|const)\s+(?:mut\s+)?{re.escape(binding)}\b\s*=(?!=)|"
            rf"(?<![A-Za-z0-9_]){re.escape(binding)}\s*=(?!=))"
        )
        for index in range(closing_line, end + 1):
            if index == line:
                continue
            line_text = lines[index - 1]
            stripped = line_text.lstrip()
            if stripped.startswith("//") or stripped.startswith("/*") or stripped.startswith("*"):
                continue
            if assignment.search(line_text):
                # A later send-result assignment starts a new observation
                # window. Do not attribute its branches to this callsite.
                if any(marker in line_text for marker in SEND_MARKERS):
                    break
                post_lines.append(index)
                post_snippets.append(re.sub(r"\s+", " ", line_text).strip())
                break
            # A later retry/reassignment is another send call, not evidence
            # that the first result was validated. Keep the callsite's own
            # post-send branches and conversions only.
            if any(marker in line_text for marker in SEND_MARKERS):
                continue
            if pattern.search(line_text):
                post_lines.append(index)
                post_snippets.append(re.sub(r"\s+", " ", line_text).strip())
    pre_lines = [
        re.sub(r"\s+", " ", lines[index - 1]).strip()
        for index in range(max(start + 1, line - 3), line)
        if lines[index - 1].strip()
    ]
    pre_evidence = " | ".join(pre_lines[-2:]) or "function entry"
    if handling == "ignored":
        post_evidence = "return ignored at this callsite"
    elif handling == "conditional":
        post_evidence = "send result is the enclosing condition"
    elif handling == "returned":
        post_evidence = "send result returned to the enclosing caller or closure"
    elif post_snippets:
        post_evidence = " | ".join(
            f"line {number}: {snippet}" for number, snippet in zip(post_lines[:4], post_snippets[:4])
        )
    else:
        post_evidence = f"bound as {binding}; no later branch in {function}"
    return {
        "send_result": {
            "handling": handling,
            "binding": binding,
            "post_use_lines": post_lines[:4],
            "post_use_snippets": post_snippets[:4],
        },
        "pre_post_send_branches": f"pre: {pre_evidence}; post: {post_evidence}",
        "retained_validation": (
            f"observed {handling}; {post_evidence}"
            if handling != "ignored"
            else "observed ignored return; no validation at this callsite"
        ),
    }


def send_surface(lines: list[str], callsites: list[int]) -> str:
    details = []
    for line in callsites:
        call = send_callsite(lines, line)
        details.append(
            f"{line}|{call['function']}|{call['receiver_expression']}|"
            f"{call['selector_literal']}|{call['arity']}"
        )
    payload = ("\n".join(details) + "\n").encode()
    return hashlib.sha256(payload).hexdigest()


def verify_definition(owner: str, path: Path, selector: str, line: int) -> None:
    text = path.read_text(encoding="utf-8", errors="ignore")
    if owner.startswith("native-birnam:"):
        pattern = rf"Reflect\s+defineMethod:\s*#" + re.escape(selector) + r"(?=\s|$)"
        if not re.search(pattern, text):
            fail(
                f"send callsite row {line} native Birnam definition does not register "
                f"#{selector} with Reflect: {path}"
            )
        return
    if not (owner.startswith("rust-dynamic:") or owner.startswith("id-runtime:")):
        fail(f"send callsite row {line} has unknown owner kind {owner!r}")
    escaped = re.escape(selector)
    direct = re.search(
        rf"\([^)]*\.method\).*?intern\([^)]*b\"{escaped}\\0\"\)",
        text,
        flags=re.DOTALL,
    )
    if direct:
        return
    constants = re.findall(
        rf"const\s+([A-Z0-9_]+):[^=]+ = b\"{escaped}\\0\"", text
    )
    if any(
        re.search(rf"\([^)]*\.method\).*?intern\([^)]*\b{re.escape(name)}\b", text, re.DOTALL)
        for name in constants
    ):
        return
    if owner.startswith("id-runtime:") and re.search(
        rf"b\"{escaped}\\0\"|b\"{escaped}\"", text
    ) and re.search(rf"selectors\.[A-Za-z0-9_]+.*?{escaped.replace('\\', '')}", text, re.DOTALL):
        return
    if re.search(rf"\(b\"{escaped}\\0\",\s*[A-Za-z_][A-Za-z0-9_]*\)", text) and re.search(
        r"\([^)]*\.method\)", text
    ):
        return
    fail(
        f"send callsite row {line} Rust definition does not register "
        f"{selector!r} through api.method: {path}"
    )


def required_direct_target(call: dict[str, object]) -> str | None:
    function = str(call["function"])
    effect = str(call.get("effect_class", ""))
    if effect.startswith("image-recovery:"):
        return "test-rust-image-bootstrap"
    if effect.startswith("mop-mirror:"):
        return "test-rust-store-mop"
    if effect.startswith("repl-orchestration:"):
        return "test-repl"
    if (
        effect.startswith("module-lifecycle:")
        or effect.startswith("project-policy:")
        or effect.startswith("toml-policy:")
        or function.startswith(("activate_", "build_project", "compile_project", "plan_project"))
        or function.startswith(("record_project", "validate_project", "run_project", "install_project"))
    ):
        return "test-project-check"
    return None


def build_target_exists(target: str) -> bool:
    build = (REPO / "build.zig").read_text(encoding="utf-8", errors="ignore")
    return bool(re.search(rf'\bb\.step\(\s*"{re.escape(target)}"(?:,|\))', build))


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

    rows = manifest.get("send_callsite_rows")
    if not isinstance(rows, list) or not rows:
        fail("send_callsite_rows must be a non-empty per-callsite array")
    actual_calls = {call["line"]: call for call in (send_callsite(lines, line) for line in actual_send)}
    row_lines: set[int] = set()
    required_fields = (
        "line",
        "function",
        "receiver_expression",
        "receiver_acquisition",
        "selector_binding",
        "selector_literal",
        "arity",
        "send_result",
        "actual_owner",
        "native_definition",
        "effect_class",
        "pre_post_send_branches",
        "retained_validation",
        "test",
        "disposition",
    )
    for row in rows:
        if not isinstance(row, dict):
            fail("every send_callsite_rows entry must be an object")
        for field in required_fields:
            if field not in row:
                fail(f"send callsite row is missing {field}")
        line = row["line"]
        if not isinstance(line, int) or line not in actual_calls:
            fail(f"send callsite row has stale/invalid line {line!r}")
        if line in row_lines:
            fail(f"send callsite row appears more than once at line {line}")
        row_lines.add(line)
        actual = actual_calls[line]
        for field in (
            "function",
            "receiver_expression",
            "selector_binding",
            "selector_literal",
            "arity",
            "send_result",
            "pre_post_send_branches",
            "retained_validation",
        ):
            if row[field] != actual[field]:
                fail(
                    f"send callsite row {line} disagrees for {field}: "
                    f"expected={actual[field]!r}, recorded={row[field]!r}"
                )
        for field in (
            "receiver_acquisition",
            "actual_owner",
            "effect_class",
            "pre_post_send_branches",
            "retained_validation",
            "disposition",
        ):
            if not isinstance(row[field], str) or not row[field].strip():
                fail(f"send callsite row {line} has no {field}")
        if not row["effect_class"].endswith(":" + actual["send_result"]["handling"]):
            fail(
                f"send callsite row {line} effect handling disagrees: "
                f"effect={row['effect_class']!r}, handling={actual['send_result']['handling']!r}"
            )
        test = row.get("test")
        if not isinstance(test, dict):
            fail(f"send callsite row {line} test evidence must be an object")
        kind = test.get("kind")
        target = test.get("target")
        evidence = test.get("evidence")
        direct = test.get("direct")
        if kind not in {"focused", "integration", "uncovered"}:
            fail(f"send callsite row {line} test kind is invalid")
        if not isinstance(target, str) or not target.strip() or not build_target_exists(target):
            fail(f"send callsite row {line} test target is not a named build target")
        if not isinstance(evidence, str) or not evidence.strip():
            fail(f"send callsite row {line} test evidence is empty")
        if not isinstance(direct, bool):
            fail(f"send callsite row {line} test direct flag is not boolean")
        if kind == "uncovered" and direct:
            fail(f"send callsite row {line} uncovered evidence cannot be direct")
        if kind != "uncovered" and not direct:
            fail(f"send callsite row {line} non-uncovered evidence must be direct")
        expected_target = required_direct_target({**actual, "effect_class": row["effect_class"]})
        if expected_target is not None and (not direct or target != expected_target):
            fail(
                f"send callsite row {line} requires direct {expected_target} evidence, "
                f"got kind={kind!r}, target={target!r}, direct={direct!r}"
            )
        definition = row["native_definition"]
        if not isinstance(definition, dict):
            fail(f"send callsite row {line} has no native_definition object")
        path = definition.get("path")
        selector = definition.get("selector")
        if not isinstance(path, str) or not path.strip():
            fail(f"send callsite row {line} has no native definition path")
        if not isinstance(selector, str) or selector != row["selector_literal"]:
            fail(f"send callsite row {line} native selector is not resolved")
        definition_path = REPO / path
        if not definition_path.is_file():
            fail(f"send callsite row {line} native definition path does not exist: {path}")
        if not selector.startswith("dynamic:"):
            verify_definition(
                row["actual_owner"], definition_path, selector, line
            )
        reference = row.get("reference_definition")
        if reference is not None:
            if not isinstance(reference, dict):
                fail(f"send callsite row {line} reference_definition is not an object")
            reference_path = reference.get("path")
            reference_selector = reference.get("selector")
            if not isinstance(reference_path, str) or not isinstance(reference_selector, str):
                fail(f"send callsite row {line} reference_definition is incomplete")
            reference_file = REPO / reference_path
            if not reference_file.is_file() or reference_selector not in reference_file.read_text(
                encoding="utf-8", errors="ignore"
            ):
                fail(f"send callsite row {line} Zig oracle reference is stale")
    if row_lines != set(actual_calls):
        fail(
            "send callsite review coverage drifted; "
            f"missing={sorted(set(actual_calls) - row_lines)}, "
            f"stale={sorted(row_lines - set(actual_calls))}"
        )

    seam = manifest.get("z2_1_seam")
    if not isinstance(seam, dict) or seam.get("status") not in {"candidate", "none"}:
        fail("z2_1_seam must explicitly nominate a candidate or conclude none exists")
    for field in ("status", "reviewed_scope", "rationale", "next_action"):
        if not isinstance(seam.get(field), str) or not seam[field].strip():
            fail(f"z2_1_seam has no {field}")
    if seam["status"] == "candidate":
        if not isinstance(seam.get("line"), int) or seam["line"] not in actual_calls:
            fail("z2_1_seam candidate must name an exact send callsite line")
        if not isinstance(seam.get("selector"), str) or not seam["selector"].strip():
            fail("z2_1_seam candidate must name a selector")
    else:
        if "line" in seam or "selector" in seam:
            fail("z2_1_seam status=none cannot carry a candidate line/selector")

    actual_functions = {call["function"] for call in actual_calls.values()}

    print(
        "rust-host-message-owner-inventory: ok "
        f"send_calls={len(actual_send)} named_global_lookups={len(actual_globals)} "
        f"owner_classes={len(owners)} reviewed_functions={len(actual_functions)} "
        "disposition=read-only"
    )


if __name__ == "__main__":
    main()
