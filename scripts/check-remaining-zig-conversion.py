#!/usr/bin/env python3
"""Fail-closed checker for the Design 68 remaining-Zig conversion ledger."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path


REPO = Path(__file__).resolve().parent.parent
LEDGER = REPO / "abi/remaining-zig-conversion-v1.json"
SOURCE = REPO / "abi/remaining-zig-v1.json"
ROLLBACK = REPO / "abi/rollback-prefix-v1.json"
ACTIONS = REPO / "abi/remaining-zig-native-actions-v1.json"
DYNAMIC_NAME = re.compile(r"^libbirnam_[a-z0-9_]+\.dylib$")
COMMIT = re.compile(r"^[0-9a-f]{7,64}$")
KINDS = {
    "zig-source",
    "dynamic-provider",
    "build-oracle",
    "shared-contract",
    "policy-source",
    "evidence-fixture",
}
CURRENT_OWNERS = {
    "rust-dylib",
    "zig-floor",
    "zig-rung",
    "zig-host-oracle",
    "zig-artifact-oracle",
    "zig-preflight-oracle",
    "zig-build",
    "shared-contract",
    "native-birnam-policy",
    "evidence",
}
TARGET_OWNERS = {
    "rust-dylib",
    "rust-host",
    "build-only",
    "native-birnam-policy",
    "shared-contract",
    "evidence-fixture",
}
PRODUCTION_STATUSES = {"canonical", "never-production", "retired"}
ROLLBACK_STATUSES = {"required", "pinned", "not-applicable"}
PHASES = {"C0", "C1", "C2", "C3", "C4", "none"}
REMOVAL_AUTHORITIES = {"none", "vector-review", "toolchain-review", "policy-review"}
POLICY_ROOTS = ("src/id/", "modules/")
ACTION_KINDS = {
    "host-observation",
    "artifact-preflight",
    "policy-review",
    "build-action",
    "provider-install",
    "stage1-compile",
    "test",
    "package",
    "rollback",
    "bootstrap",
    "oracle",
}
SELECTION_CONTEXTS = {"production", "rollback", "oracle", "build", "evidence"}
DELEGATIONS = {"none", "explicit-shadow"}
BUILD_ACTION_KINDS = {"build-action", "provider-install", "stage1-compile", "package"}
BUILD_AUTHORITIES = {"canonical", "shadow", "oracle", "retired"}
PRODUCTION_FORBIDDEN = ("project.zig", "artifact_preflight.zig", "_zig", "birnam-zig-oracle")


def fail(message: str) -> None:
    print(f"remaining-zig-conversion: {message}", file=sys.stderr)
    raise SystemExit(1)


def read_json(path: Path, label: str) -> dict:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"cannot read {label}: {exc}")
    if not isinstance(value, dict):
        fail(f"{label} must contain an object")
    return value


def git(*args: str) -> str:
    result = subprocess.run(
        ["git", *args], cwd=REPO, check=False, text=True, capture_output=True
    )
    if result.returncode != 0:
        fail(f"git {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout.strip()


def safe_repo_path(raw: object, label: str, *, suffix: str | None = None) -> Path:
    if not isinstance(raw, str) or not raw or Path(raw).is_absolute() or ".." in Path(raw).parts:
        fail(f"unsafe {label}: {raw!r}")
    path = REPO / raw
    if suffix is not None and not raw.endswith(suffix):
        fail(f"{label} is not a {suffix} path: {raw}")
    if not path.is_file():
        fail(f"{label} does not exist: {raw}")
    return path


def safe_ref(raw: object, label: str) -> None:
    """Validate a repository reference of the form path[#symbol-or-target]."""
    if not isinstance(raw, str) or not raw.strip():
        fail(f"missing {label}")
    path, separator, fragment = raw.partition("#")
    if separator and not fragment:
        fail(f"empty fragment in {label}: {raw}")
    safe_repo_path(path, label)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate-commit", help="clean candidate commit to receipt")
    parser.add_argument("--tree-hash", help="explicit reviewed Git tree object")
    args = parser.parse_args()
    if args.candidate_commit and args.tree_hash:
        fail("candidate commit and tree hash are mutually exclusive")

    ledger = read_json(LEDGER, "conversion ledger")
    source = read_json(SOURCE, "residue manifest")
    rollback = read_json(ROLLBACK, "rollback manifest")
    actions_doc = read_json(ACTIONS, "native action manifest")
    if ledger.get("schema_version") != 1:
        fail("unsupported ledger schema version")
    if source.get("schema_version") != 1 or rollback.get("schema_version") != 1:
        fail("source and rollback manifests must use schema version 1")
    if actions_doc.get("schema_version") != 1:
        fail("native action manifest must use schema version 1")
    if actions_doc.get("ledger_ref") != "abi/remaining-zig-conversion-v1.json":
        fail("native action manifest must foreign-key the canonical conversion ledger")
    if not COMMIT.fullmatch(str(ledger.get("baseline_commit", ""))):
        fail("baseline_commit must be an immutable Git commit")
    if not isinstance(ledger.get("candidate_tree_policy"), str) or not ledger["candidate_tree_policy"].strip():
        fail("candidate_tree_policy is missing")

    candidate = args.candidate_commit or git("rev-parse", "HEAD")
    if not COMMIT.fullmatch(candidate):
        fail("candidate commit is not a Git object name")
    resolved_candidate = git("rev-parse", f"{candidate}^{{commit}}")
    if args.candidate_commit and resolved_candidate != candidate and len(candidate) == 40:
        fail("candidate commit does not resolve exactly")
    if not args.tree_hash and resolved_candidate == git("rev-parse", "HEAD"):
        dirty = git("status", "--porcelain")
        if dirty:
            fail("candidate HEAD is dirty; use a clean candidate checkpoint or explicit tree hash")
    if args.tree_hash:
        tree = git("rev-parse", f"{args.tree_hash}^{{tree}}")
        if not re.fullmatch(r"[0-9a-f]{40}", tree):
            fail("tree hash does not resolve to a Git tree")

    transaction = actions_doc.get("transaction")
    if not isinstance(transaction, dict) or set(transaction) != {
        "candidate_policy", "candidate_commit", "receipt_commit", "post_receipt_gate"
    }:
        fail("native action transaction has the wrong schema")
    for field in ("candidate_policy", "post_receipt_gate"):
        if not isinstance(transaction[field], str) or not transaction[field].strip():
            fail(f"native action transaction field is missing: {field}")
    transaction_candidate = transaction["candidate_commit"]
    transaction_receipt = transaction["receipt_commit"]
    for field, value in (("candidate_commit", transaction_candidate), ("receipt_commit", transaction_receipt)):
        if value is not None:
            if not isinstance(value, str) or not COMMIT.fullmatch(value):
                fail(f"native action transaction {field} must be a commit or null")
            git("rev-parse", f"{value}^{{commit}}")
    if transaction_receipt is not None and transaction_candidate is None:
        fail("native action receipt commit cannot omit its candidate commit")

    files = source.get("files")
    if not isinstance(files, list):
        fail("residue manifest files are missing")
    source_by_path = {}
    for entry in files:
        if not isinstance(entry, dict):
            fail("residue manifest file entries must be objects")
        path = entry.get("path")
        safe_repo_path(path, "residue path", suffix=".zig")
        if path in source_by_path:
            fail(f"duplicate residue path: {path}")
        source_by_path[path] = entry

    dependencies = source.get("dynamic_dependencies")
    if not isinstance(dependencies, list) or not dependencies:
        fail("residue dynamic_dependencies are missing")
    dynamic_names = []
    for entry in dependencies:
        if not isinstance(entry, dict) or set(entry) != {"name", "role", "owner", "canonical"}:
            fail(f"dynamic dependency must be a dylib-only declaration: {entry!r}")
        if not DYNAMIC_NAME.fullmatch(str(entry.get("name", ""))):
            fail(f"invalid dynamic dependency: {entry!r}")
        if entry.get("role") != "canonical-runtime-provider" or entry.get("canonical") is not True:
            fail(f"dynamic dependency is not a canonical dylib provider: {entry!r}")
        name = entry["name"]
        if name in dynamic_names:
            fail(f"duplicate dynamic dependency: {name}")
        dynamic_names.append(name)
    rollback_vector = rollback.get("vector")
    if not isinstance(rollback_vector, list):
        fail("rollback vector is missing")
    rollback_names = [entry.get("canonical") for entry in rollback_vector if isinstance(entry, dict)]
    load_order = rollback.get("dynamic_load_order")
    if (
        len(rollback_names) != len(rollback_vector)
        or len(set(rollback_names)) != len(rollback_names)
        or not isinstance(load_order, list)
        or rollback_names != load_order
        or set(rollback_names) != set(dynamic_names)
    ):
        fail("rollback vector/load order and residue dynamic names differ")
    if rollback.get("canonical_host", {}).get("fixture") != "bin/birnam-zig-oracle":
        fail("rollback vector does not name the explicit Zig host oracle")

    rows = ledger.get("rows")
    if not isinstance(rows, list) or not rows:
        fail("ledger rows are missing")
    required = {
        "key", "kind", "current_owner", "target_owner", "production_status",
        "rollback_status", "identity_source", "digest_source", "required_fixtures",
        "next_phase", "removal_authority", "last_receipt",
    }
    seen_keys = set()
    path_rows = {}
    provider_rows = {}
    for row in rows:
        if not isinstance(row, dict) or set(row) != required | ({"path"} if "path" in row else {"name"} if "name" in row else set()):
            fail("each ledger row must have the exact schema fields plus path or name")
        key = row.get("key")
        if not isinstance(key, str) or key in seen_keys:
            fail(f"row key is missing or duplicated: {key!r}")
        seen_keys.add(key)
        kind = row.get("kind")
        if kind not in KINDS:
            fail(f"unsupported row kind: {kind!r}")
        if row.get("current_owner") not in CURRENT_OWNERS or row.get("target_owner") not in TARGET_OWNERS:
            fail(f"row owner is outside the frozen enum: {key}")
        if row.get("production_status") not in PRODUCTION_STATUSES or row.get("rollback_status") not in ROLLBACK_STATUSES:
            fail(f"row status is outside the frozen enum: {key}")
        if row.get("next_phase") not in PHASES or row.get("removal_authority") not in REMOVAL_AUTHORITIES:
            fail(f"row phase/removal authority is outside the frozen enum: {key}")
        if row.get("last_receipt") is not None and not isinstance(row["last_receipt"], str):
            fail(f"last_receipt must be string or null: {key}")
        fixtures = row.get("required_fixtures")
        if fixtures is not None and (not isinstance(fixtures, list) or any(not isinstance(x, str) for x in fixtures)):
            fail(f"required_fixtures must be string array or null: {key}")
        if row["rollback_status"] in {"required", "pinned"} and not fixtures:
            fail(f"pinned/required row has no required fixtures: {key}")
        if row["rollback_status"] == "not-applicable" and fixtures:
            fail(f"not-applicable row has fixtures: {key}")
        if key.startswith("zig:path:"):
            path = row.get("path")
            if key != "zig:path:" + str(path) or path not in source_by_path:
                fail(f"path row does not reference residue manifest: {key}")
            path_rows[path] = row
            if kind == "dynamic-provider" or row["production_status"] == "canonical":
                fail(f"Zig path cannot be canonical dynamic provider: {key}")
        elif key.startswith("dynamic:name:"):
            name = row.get("name")
            if key != "dynamic:name:" + str(name) or name not in dynamic_names or kind != "dynamic-provider":
                fail(f"dynamic row does not reference a declared provider: {key}")
            if row.get("current_owner") != "rust-dylib" or row.get("target_owner") != "rust-dylib" or row.get("production_status") != "canonical":
                fail(f"canonical dynamic row lost Rust ownership: {key}")
            if not isinstance(row.get("identity_source"), str) or not isinstance(row.get("digest_source"), str):
                fail(f"canonical dynamic row lacks identity/digest sources: {key}")
            provider_rows[name] = row
        else:
            fail(f"row key has no frozen prefix: {key}")
    if set(path_rows) != set(source_by_path):
        fail(f"ledger Zig-path coverage differs: missing={sorted(set(source_by_path) - set(path_rows))}, stale={sorted(set(path_rows) - set(source_by_path))}")
    if set(provider_rows) != set(dynamic_names):
        fail("ledger dynamic-provider coverage differs from residue manifest")

    for row in rows:
        for fixture in row.get("required_fixtures") or []:
            if fixture == "abi/rollback-prefix-v1.json":
                safe_repo_path(fixture, "fixture manifest")
            elif fixture.startswith("abi/rollback-prefix-v1.json#"):
                suffix = fixture.split("#", 1)[1]
                if suffix == "canonical_host":
                    continue
                if suffix.startswith("dynamic:name:"):
                    name = suffix.removeprefix("dynamic:name:")
                    if name not in dynamic_names:
                        fail(f"fixture references unknown provider: {fixture}")
                elif suffix.startswith("vector:"):
                    name = suffix.removeprefix("vector:")
                    if name not in dynamic_names:
                        fail(f"fixture references unknown vector entry: {fixture}")
                else:
                    fail(f"unsupported fixture reference: {fixture}")
            else:
                fail(f"fixture is outside the rollback manifest: {fixture}")

    policy = ledger.get("policy_owners")
    expected_policy = {p for p in source.get("graph", {}).get("birnam_sources", []) if p.startswith(POLICY_ROOTS)}
    if not isinstance(policy, list):
        fail("policy_owners section is missing")
    policy_paths = set()
    for owner in policy:
        if not isinstance(owner, dict) or set(owner) != {"key", "path", "owner", "status", "receipt"}:
            fail("policy owner rows have the wrong schema")
        path = owner["path"]
        if owner["key"] != "birnam:path:" + path or path not in expected_policy or not path.endswith(".birnam"):
            fail(f"policy owner is outside the checked scope: {path}")
        if owner["owner"] != "native-birnam-policy" or owner["status"] != "canonical":
            fail(f"policy owner is not canonical native Birnam: {path}")
        if owner["receipt"] is not None and not isinstance(owner["receipt"], str):
            fail(f"policy owner receipt must be string or null: {path}")
        policy_paths.add(path)
    if policy_paths != expected_policy:
        fail(f"policy owner coverage differs: missing={sorted(expected_policy - policy_paths)}, stale={sorted(policy_paths - expected_policy)}")

    owner_registry = actions_doc.get("owner_registry")
    if not isinstance(owner_registry, list) or not owner_registry:
        fail("native action owner_registry is missing")
    mechanism_owners = set()
    for owner in owner_registry:
        if not isinstance(owner, dict) or set(owner) != {
            "owner_key", "kind", "canonical_scope", "receipt_ref"
        }:
            fail("native mechanism-owner rows have the wrong schema")
        owner_key = owner["owner_key"]
        if (
            not isinstance(owner_key, str)
            or not owner_key.startswith("owner:")
            or owner_key in mechanism_owners
        ):
            fail(f"mechanism owner is missing or duplicated: {owner_key!r}")
        if owner["kind"] != "mechanism-owner":
            fail(f"unsupported mechanism owner kind: {owner_key}")
        scope = owner["canonical_scope"]
        if not isinstance(scope, list) or not scope or any(not isinstance(item, str) for item in scope):
            fail(f"mechanism owner has no canonical ledger scope: {owner_key}")
        if any(item not in seen_keys for item in scope):
            fail(f"mechanism owner scope is not in the canonical ledger: {owner_key}")
        safe_ref(owner["receipt_ref"], f"mechanism owner receipt: {owner_key}")
        mechanism_owners.add(owner_key)

    actions = actions_doc.get("actions")
    if not isinstance(actions, list) or not actions:
        fail("native actions are missing")
    action_required = {
        "action_key", "owner_key", "action_kind", "selection_context", "delegation",
        "function_ref", "tool_ref", "test_ref", "receipt_ref", "build_authority",
        "abi_surface", "removal_authority", "last_receipt",
    }
    valid_owner_keys = seen_keys | mechanism_owners | {"birnam:path:" + path for path in policy_paths}
    action_keys = set()
    for action in actions:
        if not isinstance(action, dict) or set(action) != action_required:
            fail("each native action must have the exact frozen schema")
        action_key = action["action_key"]
        if (
            not isinstance(action_key, str)
            or not re.fullmatch(r"[a-z0-9][a-z0-9:_-]+", action_key)
            or action_key in action_keys
        ):
            fail(f"native action key is missing or duplicated: {action_key!r}")
        action_keys.add(action_key)
        owner_key = action["owner_key"]
        if owner_key not in valid_owner_keys:
            fail(f"native action owner is not a ledger or mechanism owner: {action_key}")
        kind = action["action_kind"]
        context = action["selection_context"]
        delegation = action["delegation"]
        if kind not in ACTION_KINDS or context not in SELECTION_CONTEXTS or delegation not in DELEGATIONS:
            fail(f"native action enum is outside the frozen set: {action_key}")
        if action["build_authority"] not in BUILD_AUTHORITIES:
            fail(f"native action build authority is outside the frozen set: {action_key}")
        if action["removal_authority"] not in REMOVAL_AUTHORITIES:
            fail(f"native action removal authority is outside the frozen set: {action_key}")
        if kind == "policy-review":
            if not owner_key.startswith("birnam:path:") or owner_key not in {
                "birnam:path:" + path for path in policy_paths
            }:
                fail(f"policy action lacks a canonical native-policy owner: {action_key}")
        elif owner_key.startswith("birnam:path:"):
            fail(f"non-policy action claims native-policy ownership: {action_key}")
        if kind not in BUILD_ACTION_KINDS and delegation != "none":
            fail(f"non-build action delegates: {action_key}")
        if delegation == "explicit-shadow":
            if kind not in BUILD_ACTION_KINDS or action["build_authority"] != "shadow":
                fail(f"only shadow build actions may use explicit-shadow: {action_key}")
            if context == "production":
                fail(f"shadow delegation cannot be a production action: {action_key}")
        for field in ("function_ref", "tool_ref", "test_ref", "receipt_ref"):
            safe_ref(action[field], f"{field} for {action_key}")
        for field in ("abi_surface", "last_receipt"):
            if not isinstance(action[field], str) or not action[field].strip():
                fail(f"native action field is missing: {field} for {action_key}")
        if context == "production":
            refs = " ".join(str(action[field]) for field in ("function_ref", "tool_ref", "test_ref", "receipt_ref"))
            if any(forbidden in refs for forbidden in PRODUCTION_FORBIDDEN):
                fail(f"production action reaches a Zig-only path: {action_key}")

    counts = ledger.get("row_counts")
    expected_counts = {"zig_paths": len(source_by_path), "dynamic_names": len(dynamic_names), "policy_sources": len(expected_policy)}
    if counts != expected_counts:
        fail(f"row_counts mismatch: expected={expected_counts}, recorded={counts}")
    inventory = subprocess.run(
        [sys.executable, str(REPO / "scripts/check-remaining-zig-inventory.py")],
        cwd=REPO, check=False, text=True, capture_output=True,
    )
    expected_inventory = ledger.get("inventory_receipt")
    if inventory.returncode != 0 or inventory.stdout.strip() != expected_inventory:
        fail("inventory receipt is stale or inventory checker failed")

    print(
        "remaining-zig-conversion: ok "
        f"zig_paths={len(path_rows)} dynamic_names={len(provider_rows)} "
        f"policy_sources={len(policy_paths)} baseline={ledger['baseline_commit']} "
        f"candidate={resolved_candidate}"
    )


if __name__ == "__main__":
    main()
