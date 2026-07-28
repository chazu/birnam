#!/usr/bin/env python3
"""Fail-closed checker for the Design 68 C2.1 rung receipts."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path


REPO = Path(__file__).resolve().parent.parent
RECEIPT = REPO / "abi/c2-rung-receipts-v1.json"
SOURCE = REPO / "abi/remaining-zig-v1.json"
ROLLBACK = REPO / "abi/rollback-prefix-v1.json"
COMMIT = re.compile(r"^[0-9a-f]{7,64}$")
DIGEST = re.compile(r"^[0-9a-f]{64}$")
EXPECTED = {
    "floor": {
        "source": "src/id/id.zig",
        "rust_identity": "rust-id-v1",
        "zig_identity": "zig-id-v1",
        "canonical": "libbirnam_id.dylib",
        "rust_fixture": "libbirnam_id_zig.dylib",
        "rust_exports": {"id_init", "birnam_id_floor_identity", "birnam_id_rust_crate_identity"},
        "runtime_prerequisites": (),
        "zig_owner": "zig-floor",
    },
    "store": {
        "source": "src/id/store.zig",
        "rust_identity": "rust-store-n2.2-core",
        "zig_identity": "zig-store-oracle-v1",
        "canonical": "libbirnam_store.dylib",
        "rust_fixture": "libbirnam_store_zig.dylib",
        "rust_exports": {"module_init", "birnam_store_rust_identity"},
        "runtime_prerequisites": ("libbirnam_id.dylib",),
        "zig_owner": "zig-rung",
    },
    "peg": {
        "source": "src/id/peg.zig",
        "rust_identity": "rust-peg-n3.3-matcher",
        "zig_identity": "zig-peg-oracle-v1",
        "canonical": "libbirnam_id_peg.dylib",
        "rust_fixture": "libbirnam_id_peg_zig.dylib",
        "rust_exports": {"module_init", "birnam_peg_rust_identity"},
        "runtime_prerequisites": ("libbirnam_id.dylib",),
        "zig_owner": "zig-rung",
    },
}
STRICT_R6 = {
    "focused": "218/218 steps; 99/99 tests",
    "verify": "307/307 steps; 516/522 tests; 6 skipped",
    "live_alien": "66/66 steps; 9/9 tests",
    "z3": [
        "distribution-selection: production omits Zig; rollback is a complete Zig vector",
        "z3-archive: production_a=clean production_b=clean rollback=full-zig traces=ok",
    ],
    "benchmark": ["query.aggregate +5.8%", "compile.stage1-stdlib +11.9%"],
}


def fail(message: str) -> None:
    print(f"c2-rung-receipts: {message}", file=sys.stderr)
    raise SystemExit(1)


def read(path: Path, label: str) -> dict:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"cannot read {label}: {exc}")
    if not isinstance(value, dict):
        fail(f"{label} must be an object")
    return value


def safe_relative(raw: object, label: str) -> None:
    if not isinstance(raw, str) or not raw or Path(raw).is_absolute() or ".." in Path(raw).parts:
        fail(f"unsafe {label}: {raw!r}")


def git_commit(commit: str) -> str:
    result = subprocess.run(
        ["git", "rev-parse", f"{commit}^{{commit}}"],
        cwd=REPO,
        check=False,
        text=True,
        capture_output=True,
    )
    if result.returncode:
        fail(f"evidence_commit does not resolve: {commit}")
    return result.stdout.strip()


def main() -> None:
    receipt = read(RECEIPT, "C2 receipt")
    source = read(SOURCE, "residue manifest")
    rollback = read(ROLLBACK, "rollback manifest")
    if receipt.get("schema_version") != 1 or source.get("schema_version") != 1 or rollback.get("schema_version") != 1:
        fail("all manifests must use schema version 1")
    evidence_commit = receipt.get("evidence_commit")
    if not isinstance(evidence_commit, str) or not COMMIT.fullmatch(evidence_commit):
        fail("evidence_commit must be an immutable Git commit")
    git_commit(evidence_commit)
    if receipt.get("source_manifest") != "abi/remaining-zig-v1.json":
        fail("source_manifest is not the reviewed residue manifest")
    if receipt.get("rollback_manifest") != "abi/rollback-prefix-v1.json":
        fail("rollback_manifest is not the reviewed rollback vector")
    if receipt.get("strict_r6_receipt") != STRICT_R6:
        fail("strict R6 receipt is stale")

    source_paths = {entry.get("path") for entry in source.get("files", []) if isinstance(entry, dict)}
    rollback_entries = {
        entry.get("canonical"): entry
        for entry in rollback.get("vector", [])
        if isinstance(entry, dict)
    }
    rows = receipt.get("rungs")
    if not isinstance(rows, list) or [row.get("id") for row in rows if isinstance(row, dict)] != ["floor", "store", "peg"]:
        fail("rungs must be exactly floor, store, peg in order")

    required_row = {"id", "source", "readiness", "rollback_status", "canonical_rust", "zig_fixture", "selection", "evidence"}
    required_rust = {"identity", "install_name", "profiles", "exports", "dynamic_dependencies", "runtime_prerequisites"}
    required_profile = {"path", "sha256"}
    required_zig = {"owner", "identity", "install_name", "profile", "path", "sha256"}
    required_selection = {"rust", "zig_fixture", "rollback_vector", "same_floor_processes", "cross_floor_processes"}

    for row in rows:
        if not isinstance(row, dict) or set(row) != required_row:
            fail("each rung row has the wrong schema")
        rung = row["id"]
        expected = EXPECTED.get(rung)
        if expected is None:
            fail(f"unknown rung: {rung}")
        if row["source"] != expected["source"] or row["source"] not in source_paths:
            fail(f"{rung} source is not in the residue manifest")
        if row["readiness"] != "blocked" or row["rollback_status"] != "pinned":
            fail(f"{rung} may not advance readiness or rollback status in C2.1")

        rust = row["canonical_rust"]
        if not isinstance(rust, dict) or set(rust) != required_rust:
            fail(f"{rung} Rust authority has the wrong schema")
        if rust["identity"] != expected["rust_identity"] or rust["install_name"] != expected["canonical"]:
            fail(f"{rung} Rust identity/install name is wrong")
        if set(rust["exports"]) != expected["rust_exports"]:
            fail(f"{rung} Rust export set is wrong")
        if rust["runtime_prerequisites"] != list(expected["runtime_prerequisites"]):
            fail(f"{rung} runtime prerequisites are wrong")
        deps = rust["dynamic_dependencies"]
        if not isinstance(deps, list) or not deps or any(not isinstance(dep, str) for dep in deps):
            fail(f"{rung} dynamic dependency list is empty or malformed")
        profiles = rust["profiles"]
        if not isinstance(profiles, dict) or set(profiles) != {"debug", "release"}:
            fail(f"{rung} must record Debug and Release profiles")
        for profile, value in profiles.items():
            if not isinstance(value, dict) or set(value) != required_profile or not DIGEST.fullmatch(value["sha256"]):
                fail(f"{rung} {profile} Rust digest is malformed")
            safe_relative(value["path"], f"{rung} {profile} Rust path")

        zig = row["zig_fixture"]
        if not isinstance(zig, dict) or set(zig) != required_zig:
            fail(f"{rung} Zig fixture has the wrong schema")
        if zig["owner"] != expected["zig_owner"] or zig["identity"] != expected["zig_identity"]:
            fail(f"{rung} Zig fixture owner/identity is wrong")
        rollback_entry = rollback_entries.get(expected["canonical"])
        if not rollback_entry or rollback_entry.get("fixture") != expected["rust_fixture"] or rollback_entry.get("identity") != expected["zig_identity"]:
            fail(f"{rung} Zig fixture does not match the complete rollback vector")
        if zig["install_name"] != expected["rust_fixture"] or zig["profile"] not in {"debug", "release"}:
            fail(f"{rung} Zig fixture install/profile is wrong")
        if not DIGEST.fullmatch(zig["sha256"]):
            fail(f"{rung} Zig fixture digest is malformed")
        safe_relative(zig["path"], f"{rung} Zig fixture path")

        selection = row["selection"]
        if not isinstance(selection, dict) or set(selection) != required_selection:
            fail(f"{rung} selection has the wrong schema")
        for key in ("rust", "zig_fixture", "rollback_vector"):
            if not isinstance(selection[key], str) or not selection[key].strip():
                fail(f"{rung} selection command is missing: {key}")
        for key in ("same_floor_processes", "cross_floor_processes"):
            values = selection[key]
            if not isinstance(values, list) or len(values) != 2 or any(not isinstance(value, str) or not value for value in values):
                fail(f"{rung} must record exactly two {key}")
        evidence = row["evidence"]
        if not isinstance(evidence, list) or not evidence or any(not isinstance(value, str) or not value.strip() for value in evidence):
            fail(f"{rung} evidence is empty or malformed")

    print("c2-rung-receipts: ok rungs=floor,store,peg readiness=blocked rollback_status=pinned "
          f"evidence_commit={evidence_commit}")


if __name__ == "__main__":
    main()
