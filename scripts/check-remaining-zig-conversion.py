#!/usr/bin/env python3
"""Fail-closed checker for the Design 68 remaining-Zig conversion ledger."""

from __future__ import annotations

import argparse
import hashlib
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
BUILD_ACTIONS = REPO / "abi/birnam-build-actions-v1.json"
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
PRODUCTION_FORBIDDEN = ("project.zig", "artifact_preflight.zig", "_zig.dylib", "birnam-zig-oracle")
BUILD_DAG_KINDS = {"build-action", "provider-install", "stage1-compile", "test", "package", "bootstrap", "rollback"}
BUILD_INSTALL_DESTINATIONS = {"none", "prefix"}
BUILD_DIGEST_POLICIES = {"normalized-receipt-v1", "installed-artifacts-sha256", "exact-bytes-v1"}
BUILD_REF_KINDS = {
    "repo-file", "repo-tree", "generated", "external-tool", "target-output", "install-path"
}
BUILD_COMMAND_KINDS = {"build-action", "provider-install", "stage1-compile", "package", "test", "bootstrap"}
STAGE1_MODULES = [
    ("src/id/modules.birnam", "modules"),
    ("src/id/stdlib.birnam", "stdlib"),
    ("src/id/compiled-unit.birnam", "compiledUnit"),
    ("src/id/toml.birnam", "toml"),
    ("src/id/config.birnam", "config"),
    ("src/id/app-config.birnam", "appConfig"),
    ("src/id/project-policy.birnam", "projectPolicy"),
    ("src/id/isolate.birnam", "isolate"),
    ("modules/html.birnam", "html"),
    ("modules/file.birnam", "file"),
    ("modules/json.birnam", "json"),
    ("modules/socket.birnam", "socket"),
    ("modules/time.birnam", "time"),
    ("modules/doc.birnam", "doc"),
    ("modules/zigdoc.birnam", "zigdoc"),
    ("modules/lsp.birnam", "lsp"),
    ("modules/repl.birnam", "repl"),
    ("modules/cli.birnam", "cli"),
    ("modules/http.birnam", "http"),
    ("modules/log.birnam", "log"),
    ("modules/markdown.birnam", "markdown"),
    ("modules/os.birnam", "os"),
    ("modules/process.birnam", "process"),
    ("modules/random.birnam", "random"),
    ("modules/test.birnam", "test"),
    ("modules/compiled-unit-docs.birnam", "compiledUnitDocs"),
]
PROVIDER_SOURCES = {
    "libbirnam_id.dylib": ("src/id/id.zig", "rust/birnam-id/Cargo.toml", "-Dfloor"),
    "libbirnam_id_artifact.dylib": ("src/id/artifact_inspector.zig", "rust/birnam-artifact/Cargo.toml", "-Dartifact-inspector-impl"),
    "libbirnam_store.dylib": ("src/id/store.zig", "rust/birnam-store/Cargo.toml", "-Dstore-impl"),
    "libbirnam_id_peg.dylib": ("src/id/peg.zig", "rust/birnam-peg/Cargo.toml", "-Dpeg"),
    "libbirnam_id_lang.dylib": ("src/id/lang.zig", "rust/birnam-lang/Cargo.toml", "-Dlang"),
    "libbirnam_id_tcc.dylib": ("src/id/tcc.zig", "rust/birnam-tcc/Cargo.toml", "-Dtcc"),
    "libbirnam_id_codegen.dylib": ("src/id/codegen.zig", "rust/birnam-codegen/Cargo.toml", "-Dcodegen-impl"),
    "libbirnam_id_prelude.dylib": ("src/id/prelude.zig", "rust/birnam-prelude/Cargo.toml", "-Dprelude-impl"),
    "libbirnam_id_collection.dylib": ("src/id/collection.zig", "rust/birnam-collection/Cargo.toml", "-Dcollection-impl"),
    "libbirnam_id_error.dylib": ("src/id/error.zig", "rust/birnam-error/Cargo.toml", "-Derror-impl"),
    "libbirnam_id_isolate.dylib": ("src/id/isolate.zig", "rust/birnam-isolate/Cargo.toml", "-Disolate-impl"),
    "libbirnam_id_relational.dylib": ("src/id/relational.zig", "rust/birnam-relational/Cargo.toml", "-Drelational"),
    "libbirnam_id_alien.dylib": ("src/id/alien.zig", "rust/birnam-alien/Cargo.toml", "-Dalien-impl"),
    "libbirnam_id_image.dylib": ("src/id/image.zig", "rust/birnam-image/Cargo.toml", "-Dimage-impl"),
}


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


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate_build_ref(raw: object, label: str, build_targets: set[str], action_keys: set[str]) -> None:
    if not isinstance(raw, dict) or set(raw) != {"kind", "path"} and set(raw) != {"kind", "name"} and set(raw) != {"kind", "target"}:
        fail(f"{label} must be a typed build reference")
    kind = raw.get("kind")
    if kind not in BUILD_REF_KINDS:
        fail(f"{label} has an unknown reference kind: {kind!r}")
    if kind in {"repo-file", "repo-tree"}:
        if set(raw) != {"kind", "path"}:
            fail(f"{label} repository reference has the wrong schema")
        path = safe_repo_path(raw["path"], label) if kind == "repo-file" else REPO / raw["path"]
        if kind == "repo-tree":
            if Path(raw["path"]).is_absolute() or ".." in Path(raw["path"]).parts or not path.is_dir():
                fail(f"unsafe or missing {label}: {raw['path']!r}")
    elif kind in {"generated", "external-tool", "install-path"}:
        if set(raw) != {"kind", "name" if kind in {"generated", "external-tool"} else "path"}:
            fail(f"{label} has the wrong typed-reference schema")
        value = raw.get("name", raw.get("path"))
        if not isinstance(value, str) or not value.strip() or Path(value).is_absolute() or ".." in Path(value).parts:
            fail(f"unsafe {label}: {value!r}")
    elif kind == "target-output":
        if set(raw) != {"kind", "target"} or raw["target"] not in build_targets | action_keys:
            fail(f"{label} references an unknown target/action output")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate-commit", help="clean candidate commit to receipt")
    args = parser.parse_args()

    ledger = read_json(LEDGER, "conversion ledger")
    source = read_json(SOURCE, "residue manifest")
    rollback = read_json(ROLLBACK, "rollback manifest")
    actions_doc = read_json(ACTIONS, "native action manifest")
    build_actions_doc = read_json(BUILD_ACTIONS, "build action manifest")
    if ledger.get("schema_version") != 1:
        fail("unsupported ledger schema version")
    if source.get("schema_version") != 1 or rollback.get("schema_version") != 1:
        fail("source and rollback manifests must use schema version 1")
    if actions_doc.get("schema_version") != 1:
        fail("native action manifest must use schema version 1")
    if actions_doc.get("ledger_ref") != "abi/remaining-zig-conversion-v1.json":
        fail("native action manifest must foreign-key the canonical conversion ledger")
    if build_actions_doc.get("schema_version") != 1:
        fail("build action manifest must use schema version 1")
    if build_actions_doc.get("source_graph_ref") != "abi/remaining-zig-v1.json#graph.build_targets":
        fail("build action manifest must foreign-key the checked build target graph")
    if build_actions_doc.get("canonical_entrypoint") != "build.zig" or build_actions_doc.get("build_authority") != "canonical":
        fail("build action manifest must keep build.zig canonical before M5")
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
    head = git("rev-parse", "HEAD")
    if resolved_candidate != head:
        fail("candidate must be the current HEAD; checked files are read from the worktree")
    dirty = git("status", "--porcelain")
    if dirty:
        fail("candidate worktree is dirty; commit the manifest/checker before issuing a receipt")
    checked_tree = git("rev-parse", "HEAD^{tree}")

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

    capture = build_actions_doc.get("capture")
    if not isinstance(capture, dict) or set(capture) != {"level", "policy", "toolchain"}:
        fail("build action capture has the wrong schema")
    if capture["level"] != "target" or any(
        not isinstance(capture[field], str) or not capture[field].strip()
        for field in ("policy", "toolchain")
    ):
        fail("build action capture is incomplete")
    build_targets = source.get("graph", {}).get("build_targets")
    if not isinstance(build_targets, list) or not build_targets or any(not isinstance(target, str) for target in build_targets):
        fail("residue manifest build target graph is missing")
    if build_actions_doc.get("target_count") != len(build_targets):
        fail("build action target_count differs from the checked graph")
    build_rows = build_actions_doc.get("actions")
    if not isinstance(build_rows, list) or [row.get("target") for row in build_rows if isinstance(row, dict)] != build_targets:
        fail("build action rows differ in order or coverage from the checked build target graph")
    build_required = {
        "action_key", "target", "owner_key", "action_kind", "selection_context", "delegation",
        "tool_ref", "inputs", "outputs", "flags", "environment", "dependencies",
        "install_destination", "digest_policy", "build_authority", "abi_surface",
        "removal_authority", "last_receipt",
    }
    build_seen = set()
    for row in build_rows:
        if not isinstance(row, dict) or set(row) != build_required:
            fail("each build action must have the exact frozen target-level schema")
        target = row["target"]
        action_key = row["action_key"]
        if action_key != "build:" + target or target in build_seen:
            fail(f"build action key is missing or duplicated: {target!r}")
        build_seen.add(target)
        if row["owner_key"] != "zig:path:build.zig" or row["owner_key"] not in seen_keys:
            fail(f"build action lost canonical build.zig ownership: {target}")
        if row["action_kind"] not in BUILD_DAG_KINDS or row["selection_context"] not in SELECTION_CONTEXTS:
            fail(f"build action enum is outside the frozen set: {target}")
        if row["delegation"] != "none" or row["build_authority"] != "canonical":
            fail(f"build action is delegated or non-canonical before M5: {target}")
        if row["install_destination"] not in BUILD_INSTALL_DESTINATIONS:
            fail(f"build action install destination is outside the frozen set: {target}")
        if row["digest_policy"] not in BUILD_DIGEST_POLICIES:
            fail(f"build action digest policy is outside the frozen set: {target}")
        safe_ref(row["tool_ref"], f"build tool reference: {target}")
        if not row["tool_ref"].startswith("build.zig#"):
            fail(f"build action tool is not build.zig: {target}")
        for field in ("inputs", "outputs", "flags", "environment", "dependencies"):
            values = row[field]
            if not isinstance(values, list) or any(not isinstance(value, str) or not value.strip() for value in values):
                fail(f"build action {field} is not a non-empty-string array: {target}")
        for value in row["inputs"]:
            safe_repo_path(value, f"build input for {target}")
        for value in row["dependencies"]:
            if value.startswith("target:"):
                dependency_target = value.removeprefix("target:")
                if dependency_target not in build_targets or dependency_target == target:
                    fail(f"build action dependency is outside the target graph: {target}")
            elif not value.startswith("external:"):
                fail(f"build action dependency is not a target or external tool: {target}")
        if not isinstance(row["abi_surface"], str) or not row["abi_surface"].strip():
            fail(f"build action ABI surface is missing: {target}")
        if row["removal_authority"] not in REMOVAL_AUTHORITIES:
            fail(f"build action removal authority is outside the frozen set: {target}")
        if not isinstance(row["last_receipt"], str) or not row["last_receipt"].strip():
            fail(f"build action receipt is missing: {target}")

    source_digests = build_actions_doc.get("source_digests")
    if not isinstance(source_digests, dict) or set(source_digests) != {"build.zig", "Cargo.toml", "Cargo.lock"}:
        fail("build action source_digests must pin build.zig and the locked Cargo workspace")
    for path, digest in source_digests.items():
        safe_repo_path(path, "build source digest")
        if not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest):
            fail(f"build source digest is malformed: {path}")
        if sha256_file(REPO / path) != digest:
            fail(f"build source digest is stale: {path}")

    command_rows = build_actions_doc.get("command_actions")
    if not isinstance(command_rows, list) or not command_rows:
        fail("command-level build actions are missing")
    command_required = {
        "action_key", "owner_key", "action_kind", "selection_context", "delegation",
        "tool_ref", "inputs", "outputs", "flags", "environment", "dependencies",
        "install_destination", "digest_policy", "build_authority", "abi_surface",
        "removal_authority", "last_receipt",
    }
    command_keys = set()
    for action in command_rows:
        if not isinstance(action, dict) or set(action) != command_required:
            fail("each command build action must have the exact typed schema")
        key = action["action_key"]
        if not isinstance(key, str) or not re.fullmatch(r"action:[a-z0-9:_-]+", key) or key in command_keys:
            fail(f"command action key is missing or duplicated: {key!r}")
        command_keys.add(key)
        if action["owner_key"] != "zig:path:build.zig" or action["delegation"] != "none":
            fail(f"command action lost canonical build.zig ownership: {key}")
        if action["action_kind"] not in BUILD_COMMAND_KINDS or action["selection_context"] not in SELECTION_CONTEXTS:
            fail(f"command action enum is outside the frozen set: {key}")
        if action["build_authority"] != "canonical" or action["removal_authority"] not in REMOVAL_AUTHORITIES:
            fail(f"command action authority is outside the frozen set: {key}")
        if action["install_destination"] not in BUILD_INSTALL_DESTINATIONS or action["digest_policy"] not in BUILD_DIGEST_POLICIES:
            fail(f"command action install/digest policy is outside the frozen set: {key}")
        validate_build_ref(action["tool_ref"], f"command tool for {key}", set(build_targets), command_keys | build_seen)
        for field in ("inputs", "outputs"):
            values = action[field]
            if not isinstance(values, list) or not values:
                fail(f"command action {field} is empty: {key}")
            for index, value in enumerate(values):
                validate_build_ref(value, f"command {field}[{index}] for {key}", set(build_targets), command_keys | build_seen)
        if not isinstance(action["flags"], list) or any(not isinstance(value, str) or not value.strip() for value in action["flags"]):
            fail(f"command action flags are malformed: {key}")
        if not isinstance(action["environment"], list) or any(
            not isinstance(value, dict) or set(value) != {"name", "value"} or
            not isinstance(value["name"], str) or not value["name"].strip() or
            not isinstance(value["value"], str) or not value["value"].strip()
            for value in action["environment"]
        ):
            fail(f"command action environment is malformed: {key}")
        if not isinstance(action["dependencies"], list):
            fail(f"command action dependencies are malformed: {key}")
        for dependency in action["dependencies"]:
            if not isinstance(dependency, str) or not (dependency.startswith("action:") or dependency.startswith("target:")):
                fail(f"command action dependency is not typed: {key}")
            if dependency.startswith("action:") and dependency not in command_keys:
                # Forward references are legal, but the final closed-set check below must see them.
                continue
            if dependency.startswith("target:") and dependency.removeprefix("target:") not in build_targets:
                fail(f"command action dependency targets an unknown build target: {key}")
        for field in ("abi_surface", "last_receipt"):
            if not isinstance(action[field], str) or not action[field].strip():
                fail(f"command action field is missing: {field} for {key}")
        if action["selection_context"] == "production":
            refs = json.dumps(action["inputs"]) + json.dumps(action["outputs"]) + json.dumps(action["tool_ref"])
            if any(forbidden in refs for forbidden in PRODUCTION_FORBIDDEN):
                fail(f"production command action reaches a Zig-only path: {key}")
    all_action_keys = build_seen | command_keys
    for action in command_rows:
        for dependency in action["dependencies"]:
            if dependency.startswith("action:") and dependency not in all_action_keys:
                fail(f"command action dependency references an unknown action: {action['action_key']}")
    provider_production = {a["action_key"] for a in command_rows if a["action_kind"] == "provider-install" and a["selection_context"] == "production"}
    provider_rollback = {a["action_key"] for a in command_rows if a["action_kind"] == "provider-install" and a["selection_context"] == "rollback"}
    if len(provider_production) != len(dynamic_names) or len(provider_rollback) != len(dynamic_names):
        fail("command actions must freeze every canonical provider and every rollback provider fixture")

    if build_actions_doc.get("action_count") != len(build_rows) + len(command_rows):
        fail("build action action_count does not equal target plus command actions")
    roots = build_actions_doc.get("target_action_roots")
    if not isinstance(roots, list) or len(roots) != len(build_targets):
        fail("target_action_roots must cover every checked build target")
    root_targets = set()
    for root in roots:
        if not isinstance(root, dict) or set(root) != {"target", "action_keys"} or root["target"] not in build_targets:
            fail("target action root has the wrong schema or target")
        if root["target"] in root_targets or not isinstance(root["action_keys"], list) or not root["action_keys"]:
            fail(f"target action root is missing or duplicated: {root.get('target')!r}")
        root_targets.add(root["target"])
        if any(key not in all_action_keys for key in root["action_keys"]):
            fail(f"target action root references an unknown action: {root['target']}")
    if root_targets != set(build_targets):
        fail("target action roots differ from the checked target graph")

    surfaces = build_actions_doc.get("surfaces")
    if not isinstance(surfaces, dict) or set(surfaces) != {
        "build_options", "rust_workspace", "libtcc", "providers", "stage1_modules", "resources", "target_dependencies"
    }:
        fail("build action surfaces have the wrong schema")
    options = surfaces["build_options"]
    expected_options = [
        "target", "optimize", "distribution", "floor", "store-impl", "peg", "codegen-impl", "tcc", "lang",
        "host-impl", "artifact-inspector-impl", "prelude-impl", "collection-impl", "error-impl", "isolate-impl",
        "relational", "image-impl", "alien-impl", "id-conformance-floor", "id-conformance-identity", "id-conformance-sha256",
    ]
    if not isinstance(options, dict) or set(options) != {"source_ref", "names"} or options["names"] != expected_options:
        fail("build option surface is stale or incomplete")
    safe_ref(options["source_ref"], "build option source")
    workspace = surfaces["rust_workspace"]
    if not isinstance(workspace, dict) or set(workspace) != {"manifest", "lockfile", "members", "command", "target_dir"}:
        fail("Rust workspace build surface has the wrong schema")
    safe_repo_path(workspace["manifest"], "Rust workspace manifest")
    safe_repo_path(workspace["lockfile"], "Rust workspace lockfile")
    if workspace["members"] != source.get("graph", {}).get("rust_crates") or workspace["command"] != ["cargo", "build", "--workspace", "--locked", "--offline"] or workspace["target_dir"] != ".cargo-target":
        fail("Rust workspace build surface is stale")
    tcc = surfaces["libtcc"]
    if not isinstance(tcc, dict) or set(tcc) != {"source_tree", "source_file", "include_path", "flags", "linkage"} or tcc["flags"] != ["-std=gnu11", "-w"] or tcc["linkage"] != "static":
        fail("libtcc build surface is stale")
    if not (REPO / tcc["source_tree"]).is_dir() or safe_repo_path(tcc["source_file"], "libtcc source") is None or not (REPO / tcc["include_path"]).is_dir():
        fail("libtcc source/include surface is missing")
    providers = surfaces["providers"]
    if not isinstance(providers, list) or len(providers) != len(dynamic_names):
        fail("provider build surface does not cover every dynamic provider")
    seen_providers = set()
    for provider in providers:
        if not isinstance(provider, dict) or set(provider) != {"canonical", "zig_source", "rust_crate", "selection_flag", "rust_fixture", "zig_fixture"}:
            fail("provider build surface row has the wrong schema")
        name = provider["canonical"]
        if name in seen_providers or name not in PROVIDER_SOURCES:
            fail(f"provider build surface has an unknown/duplicate provider: {name}")
        seen_providers.add(name)
        expected_source, expected_crate, expected_flag = PROVIDER_SOURCES[name]
        if (provider["zig_source"], provider["rust_crate"], provider["selection_flag"]) != (expected_source, expected_crate, expected_flag):
            fail(f"provider build surface is stale: {name}")
        safe_repo_path(provider["zig_source"], "provider Zig source", suffix=".zig")
        safe_repo_path(provider["rust_crate"], "provider Cargo manifest", suffix="Cargo.toml")
        for field in ("rust_fixture", "zig_fixture"):
            if not isinstance(provider[field], str) or not provider[field].startswith("lib/") or not provider[field].endswith(".dylib"):
                fail(f"provider fixture path is malformed: {name}")
    if seen_providers != set(dynamic_names):
        fail("provider build surface differs from dynamic dependency names")
    stage1 = surfaces["stage1_modules"]
    expected_stage1 = [{"order": i, "source": source_path, "name": name, "output": f"lib/libbirnam_{name}.dylib"} for i, (source_path, name) in enumerate(STAGE1_MODULES)]
    if stage1 != expected_stage1:
        fail("stage1 module order/source/output surface is stale")
    for module in stage1:
        safe_repo_path(module["source"], "stage1 source")
    resources = surfaces["resources"]
    expected_resources = [
        {"source": {"kind": "repo-tree", "path": "src/id"}, "destination": {"kind": "install-path", "path": "share/birnam/src/id"}},
        {"source": {"kind": "repo-tree", "path": "rust"}, "destination": {"kind": "install-path", "path": "share/birnam/src/rust"}},
        {"source": {"kind": "repo-file", "path": "Cargo.toml"}, "destination": {"kind": "install-path", "path": "share/birnam/src/Cargo.toml"}},
        {"source": {"kind": "repo-file", "path": "Cargo.lock"}, "destination": {"kind": "install-path", "path": "share/birnam/src/Cargo.lock"}},
        {"source": {"kind": "repo-tree", "path": "docs/guide"}, "destination": {"kind": "install-path", "path": "share/birnam/docs/guide"}},
    ]
    if resources != expected_resources:
        fail("installed resource surface is stale")
    for resource in resources:
        validate_build_ref(resource["source"], "resource source", set(build_targets), all_action_keys)
        validate_build_ref(resource["destination"], "resource destination", set(build_targets), all_action_keys)
    dependency_capture = surfaces["target_dependencies"]
    if not isinstance(dependency_capture, dict) or set(dependency_capture) != {"source_ref", "edge_count", "edge_sha256", "capture"}:
        fail("target dependency capture has the wrong schema")
    safe_ref(dependency_capture["source_ref"], "target dependency source")
    dependency_lines = [" ".join(line.strip().split()) for line in (REPO / "build.zig").read_text(encoding="utf-8").splitlines() if ".dependOn" in line]
    dependency_digest = hashlib.sha256(("\n".join(dependency_lines) + "\n").encode()).hexdigest()
    if dependency_capture["edge_count"] != len(dependency_lines) or dependency_capture["edge_sha256"] != dependency_digest:
        fail("target dependency edges drifted from build.zig")

    build_transaction = build_actions_doc.get("transaction")
    if not isinstance(build_transaction, dict) or set(build_transaction) != {
        "candidate_commit", "receipt_commit", "post_receipt_gate"
    }:
        fail("build action transaction has the wrong schema")
    for field in ("candidate_commit", "receipt_commit"):
        value = build_transaction[field]
        if value is not None:
            if not isinstance(value, str) or not COMMIT.fullmatch(value):
                fail(f"build action transaction {field} must be a commit or null")
            git("rev-parse", f"{value}^{{commit}}")
    if build_transaction["receipt_commit"] is not None and build_transaction["candidate_commit"] is None:
        fail("build action receipt commit cannot omit its candidate commit")
    if not isinstance(build_transaction["post_receipt_gate"], str) or not build_transaction["post_receipt_gate"].strip():
        fail("build action transaction post_receipt_gate is missing")

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
        f"candidate={resolved_candidate} tree={checked_tree} "
        f"build_targets={len(build_rows)} build_actions={len(command_rows)}"
    )


if __name__ == "__main__":
    main()
