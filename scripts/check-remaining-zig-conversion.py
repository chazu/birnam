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
    if ledger.get("schema_version") != 1:
        fail("unsupported ledger schema version")
    if source.get("schema_version") != 1 or rollback.get("schema_version") != 1:
        fail("source and rollback manifests must use schema version 1")
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
        if not isinstance(entry, dict) or not DYNAMIC_NAME.fullmatch(str(entry.get("name", ""))):
            fail(f"invalid dynamic dependency: {entry!r}")
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
