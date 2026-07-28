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
RECEIPT_V2 = REPO / "abi/c2-rung-receipts-v2.json"
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
EXPECTED_V2 = {
    "lang": {
        "source": "src/id/lang.zig",
        "rust_identity": "rust-lang-n5.1-grammar",
        "zig_identity": "zig-lang-oracle-v1",
        "canonical": "libbirnam_id_lang.dylib",
        "rust_fixture": "libbirnam_id_lang_zig.dylib",
        "rust_exports": {"module_init", "birnam_lang_rust_identity"},
        "runtime_prerequisites": ("libbirnam_id.dylib", "libbirnam_id_peg.dylib"),
    },
    "tcc": {
        "source": "src/id/tcc.zig",
        "rust_identity": "rust-tcc-n4.2-state",
        "zig_identity": "zig-tcc-oracle-v1",
        "canonical": "libbirnam_id_tcc.dylib",
        "rust_fixture": "libbirnam_id_tcc_zig.dylib",
        "rust_exports": {
            "birnam_alloc", "birnam_blockVt", "birnam_cstr", "birnam_ensurePrivateVtable", "birnam_false",
            "birnam_floatValue", "birnam_free_cstr", "birnam_intern", "birnam_layoutAllOops", "birnam_method",
            "birnam_mkFloat", "birnam_mkSmallInt", "birnam_mkString", "birnam_mkWord", "birnam_nil",
            "birnam_objectVt", "birnam_proto", "birnam_send", "birnam_smallIntValue", "birnam_tcc_rust_identity",
            "birnam_true", "birnam_uintValue", "birnam_wordValue", "module_init",
        },
        "runtime_prerequisites": ("libbirnam_id.dylib",),
    },
    "codegen": {
        "source": "src/id/codegen.zig",
        "rust_identity": "rust-codegen-n6.3-emitter",
        "zig_identity": "zig-codegen-oracle-v1",
        "canonical": "libbirnam_id_codegen.dylib",
        "rust_fixture": "libbirnam_id_codegen_zig.dylib",
        "rust_exports": {
            "birnam_candidate_prepare_mode", "birnam_candidate_root_get", "birnam_candidate_root_set", "birnam_candidate_validate_binding",
            "birnam_codegen_begin_quiescence", "birnam_codegen_end_quiescence", "birnam_codegen_enter_running", "birnam_codegen_rust_identity",
            "birnam_global", "birnam_global_b", "birnam_global_set", "birnam_install_method", "birnam_is_error",
            "birnam_record_global_src", "birnam_record_module_source", "birnam_remove_method",
            "birnam_rust_codegen_candidate_prepare_mode", "birnam_rust_codegen_candidate_root_get", "birnam_rust_codegen_candidate_root_set",
            "birnam_rust_codegen_candidate_validate_binding", "birnam_rust_codegen_global", "birnam_rust_codegen_global_b",
            "birnam_rust_codegen_global_set", "birnam_rust_codegen_install_method", "birnam_rust_codegen_is_error",
            "birnam_rust_codegen_record_global_src", "birnam_rust_codegen_record_module_source", "birnam_rust_codegen_remove_method", "module_init",
        },
        "runtime_prerequisites": ("libbirnam_id.dylib", "libbirnam_store.dylib", "libbirnam_id_lang.dylib", "libbirnam_id_tcc.dylib"),
    },
    "prelude": {
        "source": "src/id/prelude.zig",
        "rust_identity": "rust-prelude-n7.1-core",
        "zig_identity": "zig-prelude-oracle-v1",
        "canonical": "libbirnam_id_prelude.dylib",
        "rust_fixture": "libbirnam_id_prelude_zig.dylib",
        "rust_exports": {"module_init", "birnam_prelude_rust_identity"},
        "runtime_prerequisites": ("libbirnam_id.dylib", "libbirnam_id_codegen.dylib", "libbirnam_id_error.dylib"),
    },
    "collection": {
        "source": "src/id/collection.zig",
        "rust_identity": "rust-collection-r1-vector",
        "zig_identity": "zig-collection-oracle-v1",
        "canonical": "libbirnam_id_collection.dylib",
        "rust_fixture": "libbirnam_id_collection_zig.dylib",
        "rust_exports": {"module_init", "birnam_collection_rust_identity"},
        "runtime_prerequisites": ("libbirnam_id.dylib",),
    },
    "error": {
        "source": "src/id/error.zig",
        "rust_identity": "rust-error-r2-core",
        "zig_identity": "zig-error-oracle-v1",
        "canonical": "libbirnam_id_error.dylib",
        "rust_fixture": "libbirnam_id_error_zig.dylib",
        "rust_exports": {"module_init", "birnam_error_rust_identity"},
        "runtime_prerequisites": ("libbirnam_id.dylib",),
    },
    "isolate": {
        "source": "src/id/isolate.zig",
        "rust_identity": "rust-isolate-r3-codec",
        "zig_identity": "zig-isolate-oracle-v1",
        "canonical": "libbirnam_id_isolate.dylib",
        "rust_fixture": "libbirnam_id_isolate_zig.dylib",
        "rust_exports": {
            "birnam_isolate_drain", "birnam_isolate_rust_decode_header", "birnam_isolate_rust_decode_oop",
            "birnam_isolate_rust_encode_header", "birnam_isolate_rust_encode_oop", "birnam_isolate_rust_identity",
            "birnam_isolate_shutdown", "birnam_isolate_worker_boot_error", "birnam_isolate_worker_driver", "module_init",
        },
        "runtime_prerequisites": ("libbirnam_id.dylib", "libbirnam_id_collection.dylib", "libbirnam_id_error.dylib"),
    },
    "relational": {
        "source": "src/id/relational.zig",
        "rust_identity": "rust-relational-r4.5-project",
        "zig_identity": "zig-relational-oracle-v1",
        "canonical": "libbirnam_id_relational.dylib",
        "rust_fixture": "libbirnam_id_relational_zig.dylib",
        "rust_exports": {
            "birnam_relational_rust_aggregate_live", "birnam_relational_rust_classify", "birnam_relational_rust_collect_live",
            "birnam_relational_rust_collect_window", "birnam_relational_rust_group_aggregate_live", "birnam_relational_rust_identity",
            "birnam_relational_rust_project_disjunction_live", "birnam_relational_rust_project_join_live", "birnam_relational_rust_project_live",
            "birnam_relational_rust_project_negated_live", "birnam_relational_rust_project_predicate_live", "birnam_relational_rust_project_recursive_live",
            "birnam_relational_rust_project_rule_live", "birnam_relational_rust_project_rule_store_live", "birnam_relational_rust_project_rule_tree_live",
            "birnam_relational_rust_project_window_live", "birnam_relational_rust_pull_live", "module_init",
        },
        "runtime_prerequisites": ("libbirnam_id.dylib", "libbirnam_store.dylib", "libbirnam_id_collection.dylib"),
    },
    "alien": {
        "source": "src/id/alien.zig",
        "rust_identity": "rust-alien-r5-loader",
        "zig_identity": "zig-alien-oracle-v1",
        "canonical": "libbirnam_id_alien.dylib",
        "rust_fixture": "libbirnam_id_alien_zig.dylib",
        "rust_exports": {"module_init", "birnam_alien_rust_identity"},
        "runtime_prerequisites": ("libbirnam_id.dylib", "libbirnam_id_tcc.dylib", "libbirnam_store.dylib", "libbirnam_id_collection.dylib", "libbirnam_id_error.dylib"),
    },
    "image": {
        "source": "src/id/image.zig",
        "rust_identity": "rust-image-r6-shell",
        "zig_identity": "zig-image-oracle-v1",
        "canonical": "libbirnam_id_image.dylib",
        "rust_fixture": "libbirnam_id_image_zig.dylib",
        "rust_exports": {"module_init", "birnam_image_rust_identity"},
        "runtime_prerequisites": ("libbirnam_id.dylib", "libbirnam_store.dylib", "libbirnam_id_codegen.dylib", "libbirnam_id_prelude.dylib", "libbirnam_id_error.dylib"),
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


def validate_rows(
    receipt: dict,
    expected_rows: dict,
    source_paths: set[object],
    rollback_entries: dict[object, dict],
    label: str,
) -> None:
    required_row = {"id", "source", "readiness", "rollback_status", "canonical_rust", "zig_fixture", "selection", "evidence"}
    required_rust = {"identity", "install_name", "profiles", "exports", "dynamic_dependencies", "runtime_prerequisites"}
    required_profile = {"path", "sha256"}
    required_zig = {"owner", "identity", "install_name", "profile", "path", "sha256"}
    required_selection = {"rust", "zig_fixture", "rollback_vector", "same_floor_processes", "cross_floor_processes"}
    expected_ids = list(expected_rows)
    rows = receipt.get("rungs")
    if not isinstance(rows, list) or [row.get("id") for row in rows if isinstance(row, dict)] != expected_ids:
        fail(f"{label} rungs must be exactly {', '.join(expected_ids)} in order")

    for row in rows:
        if not isinstance(row, dict) or set(row) != required_row:
            fail(f"{label} each rung row has the wrong schema")
        rung = row["id"]
        expected = expected_rows.get(rung)
        if expected is None:
            fail(f"{label} unknown rung: {rung}")
        if row["source"] != expected["source"] or row["source"] not in source_paths:
            fail(f"{label} {rung} source is not in the residue manifest")
        if row["readiness"] != "blocked" or row["rollback_status"] != "pinned":
            fail(f"{label} {rung} may not advance readiness or rollback status")

        rust = row["canonical_rust"]
        if not isinstance(rust, dict) or set(rust) != required_rust:
            fail(f"{label} {rung} Rust authority has the wrong schema")
        if rust["identity"] != expected["rust_identity"] or rust["install_name"] != expected["canonical"]:
            fail(f"{label} {rung} Rust identity/install name is wrong")
        if set(rust["exports"]) != expected["rust_exports"]:
            fail(f"{label} {rung} Rust export set is wrong")
        if rust["runtime_prerequisites"] != list(expected["runtime_prerequisites"]):
            fail(f"{label} {rung} runtime prerequisites are wrong")
        deps = rust["dynamic_dependencies"]
        if not isinstance(deps, list) or deps != ["libSystem.B.dylib"]:
            fail(f"{label} {rung} dynamic dependency set is not the direct system library only")
        profiles = rust["profiles"]
        if not isinstance(profiles, dict) or set(profiles) != {"debug", "release"}:
            fail(f"{label} {rung} must record Debug and Release profiles")
        for profile, value in profiles.items():
            if not isinstance(value, dict) or set(value) != required_profile or not DIGEST.fullmatch(value["sha256"]):
                fail(f"{label} {rung} {profile} Rust digest is malformed")
            safe_relative(value["path"], f"{label} {rung} {profile} Rust path")

        zig = row["zig_fixture"]
        if not isinstance(zig, dict) or set(zig) != required_zig:
            fail(f"{label} {rung} Zig fixture has the wrong schema")
        if zig["owner"] != "zig-rung" or zig["identity"] != expected["zig_identity"]:
            fail(f"{label} {rung} Zig fixture owner/identity is wrong")
        rollback_entry = rollback_entries.get(expected["canonical"])
        if not rollback_entry or rollback_entry.get("fixture") != expected["rust_fixture"] or rollback_entry.get("identity") != expected["zig_identity"]:
            fail(f"{label} {rung} Zig fixture does not match the complete rollback vector")
        if zig["install_name"] != expected["rust_fixture"] or zig["profile"] not in {"debug", "release"}:
            fail(f"{label} {rung} Zig fixture install/profile is wrong")
        if not DIGEST.fullmatch(zig["sha256"]):
            fail(f"{label} {rung} Zig fixture digest is malformed")
        safe_relative(zig["path"], f"{label} {rung} Zig fixture path")

        selection = row["selection"]
        if not isinstance(selection, dict) or set(selection) != required_selection:
            fail(f"{label} {rung} selection has the wrong schema")
        for key in ("rust", "zig_fixture", "rollback_vector"):
            if not isinstance(selection[key], str) or not selection[key].strip():
                fail(f"{label} {rung} selection command is missing: {key}")
        for key in ("same_floor_processes", "cross_floor_processes"):
            values = selection[key]
            if not isinstance(values, list) or len(values) != 2 or any(not isinstance(value, str) or not value for value in values):
                fail(f"{label} {rung} must record exactly two {key}")
        evidence = row["evidence"]
        if not isinstance(evidence, list) or not evidence or any(not isinstance(value, str) or not value.strip() for value in evidence):
            fail(f"{label} {rung} evidence is empty or malformed")


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

    receipt_v2 = read(RECEIPT_V2, "C2.2 receipt")
    if receipt_v2.get("schema_version") != 1:
        fail("C2.2 receipt must use schema version 1")
    evidence_commit_v2 = receipt_v2.get("evidence_commit")
    if not isinstance(evidence_commit_v2, str) or not COMMIT.fullmatch(evidence_commit_v2):
        fail("C2.2 evidence_commit must be an immutable Git commit")
    git_commit(evidence_commit_v2)
    if receipt_v2.get("prior_receipt_manifest") != "abi/c2-rung-receipts-v1.json":
        fail("C2.2 receipt must chain to the C2.1 receipt")
    if receipt_v2.get("source_manifest") != "abi/remaining-zig-v1.json" or receipt_v2.get("rollback_manifest") != "abi/rollback-prefix-v1.json":
        fail("C2.2 receipt manifests are not the reviewed residue/rollback manifests")
    if receipt_v2.get("strict_r6_receipt") != STRICT_R6:
        fail("C2.2 strict R6 receipt is stale")
    validate_rows(receipt_v2, EXPECTED_V2, source_paths, rollback_entries, "C2.2")

    print("c2-rung-receipts: ok rungs=floor,store,peg readiness=blocked rollback_status=pinned "
          f"evidence_commit={evidence_commit}")
    print("c2-rung-receipts: ok C2.2 rungs=lang,tcc,codegen,prelude,collection,error,isolate,relational,alien,image "
          f"readiness=blocked rollback_status=pinned evidence_commit={evidence_commit_v2}")


if __name__ == "__main__":
    main()
