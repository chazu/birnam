#!/usr/bin/env python3
"""Fail-closed Design 66 Z0.1 residue and runtime-boundary checker."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path


REPO = Path(__file__).resolve().parent.parent
MANIFEST = REPO / "abi/remaining-zig-v1.json"
EDGE_MANIFEST = REPO / "abi/remaining-zig-edges-v1.json"
KINDS = {
    "canonical-production",
    "zig-oracle",
    "build-only",
    "shared-contract",
    "test-evidence",
    "native-policy-bridge",
}
DYNAMIC_NAME = re.compile(r"^libbirnam_[a-z0-9_]+\.dylib$")
STEP_NAME = re.compile(r'\.step\(\s*"([^"]+)"')
EDGE_KINDS = {"runtime-dlopen", "build-input", "oracle", "evidence", "native-policy"}
EDGE_PHASES = {"P0", "P1", "P2", "P3", "P4"}
EDGE_CONTEXTS = {"production", "rollback", "oracle", "build", "evidence"}
DERIVED_MANIFEST_KINDS = {
    "canonical-production",
    "zig-oracle",
    "build-only",
    "shared-contract",
    "test-evidence",
    "native-policy-bridge",
}
DERIVED_GRAPH_KEYS = {"birnam_sources", "rust_crates", "test_scripts", "build_targets"}


def fail(message: str) -> None:
    print(f"remaining-zig-inventory: {message}", file=sys.stderr)
    raise SystemExit(1)


def listed(pattern: str, *extra: str) -> list[str]:
    command = ["rg", "--files", *extra, "-g", pattern]
    result = subprocess.run(command, cwd=REPO, check=True, text=True, capture_output=True)
    return sorted(line for line in result.stdout.splitlines() if line)


def main() -> None:
    try:
        manifest = json.loads(MANIFEST.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"cannot read manifest: {exc}")
    if manifest.get("schema_version") != 1:
        fail("unsupported schema version")

    files = manifest.get("files")
    if not isinstance(files, list):
        fail("files must be an array")
    paths = [item.get("path") for item in files if isinstance(item, dict)]
    if len(paths) != len(files) or any(not isinstance(path, str) for path in paths):
        fail("every file entry must be an object with a string path")
    if len(paths) != len(set(paths)):
        fail("a Zig file is classified more than once")
    for item in files:
        if item.get("kind") not in KINDS:
            fail(f"unsupported or missing kind for {item.get('path')!r}")
        for field in ("owner", "disposition"):
            if not isinstance(item.get(field), str) or not item[field].strip():
                fail(f"{item.get('path')!r} has no {field}")
        path = item["path"]
        if Path(path).is_absolute() or ".." in Path(path).parts:
            fail(f"unsafe manifest path: {path}")
        if not (REPO / path).is_file():
            fail(f"classified Zig file does not exist: {path}")

    actual_zig = listed("*.zig")
    if set(paths) != set(actual_zig):
        fail(
            "repo-wide Zig graph differs; "
            f"missing={sorted(set(actual_zig) - set(paths))}, "
            f"stale={sorted(set(paths) - set(actual_zig))}"
        )

    graph = manifest.get("graph")
    if not isinstance(graph, dict):
        fail("graph is missing")
    for key, pattern, args in (
        ("birnam_sources", "*.birnam", ()),
        ("rust_crates", "Cargo.toml", ()),
        ("test_scripts", "*.sh", ("scripts",)),
    ):
        expected = listed(pattern, *args)
        if key == "test_scripts":
            expected = sorted(set(expected) | set(listed("*.py", "scripts")))
        recorded = graph.get(key)
        if recorded != expected:
            fail(f"{key} graph drifted; expected={expected}, recorded={recorded}")
    if not graph.get("build_targets") or not isinstance(graph.get("build_targets"), list):
        fail("build_targets must be a non-empty array")
    build_text = (REPO / "build.zig").read_text()
    actual_targets = sorted(set(STEP_NAME.findall(build_text)))
    if graph["build_targets"] != actual_targets:
        fail(
            "build target graph drifted; "
            f"missing={sorted(set(actual_targets) - set(graph['build_targets']))}, "
            f"stale={sorted(set(graph['build_targets']) - set(actual_targets))}"
        )
    destinations = graph.get("install_destinations")
    if not isinstance(destinations, list) or "bin/birnam" not in destinations:
        fail("install_destinations must name the canonical executable")
    if not any(isinstance(item, str) and item.startswith("lib/libbirnam_") for item in destinations):
        fail("install_destinations must name the dynamic library family")

    dependencies = manifest.get("dynamic_dependencies")
    if not isinstance(dependencies, list) or not dependencies:
        fail("dynamic_dependencies must be a non-empty array")
    names = []
    for dependency in dependencies:
        if not isinstance(dependency, dict):
            fail("dynamic dependency entries must be objects")
        name = dependency.get("name")
        if not isinstance(name, str) or not DYNAMIC_NAME.fullmatch(name):
            fail(f"not a declared dynamic library name: {name!r}")
        if name in names:
            fail(f"dynamic dependency appears more than once: {name}")
        if not dependency.get("owner") or not dependency.get("role"):
            fail(f"dynamic dependency has no owner/role: {name}")
        names.append(name)

    main_text = (REPO / "src/id/main.zig").read_text()
    match = re.search(r"const script_modules = \[_\]\[\]const u8\{(.*?)\n\};", main_text, re.DOTALL)
    if not match:
        fail("could not locate canonical script_modules load order")
    expected_dynamic = {"libbirnam_id.dylib"}
    expected_dynamic.update(f"{module}.dylib" for module in re.findall(r'"(libbirnam_[^"]+)"', match.group(1)))
    if set(names) != expected_dynamic:
        fail(f"declared dynamic names differ from runtime load order; expected={sorted(expected_dynamic)}, declared={sorted(names)}")

    try:
        edge_manifest = json.loads(EDGE_MANIFEST.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"cannot read residue edge manifest: {exc}")
    if edge_manifest.get("schema_version") != 1:
        fail("unsupported residue edge schema version")
    if set(edge_manifest.get("allowed_edge_kinds", [])) != EDGE_KINDS:
        fail("residue edge kinds are not frozen")
    if set(edge_manifest.get("allowed_phases", [])) != EDGE_PHASES:
        fail("residue edge phases are not frozen")
    if set(edge_manifest.get("allowed_selection_contexts", [])) != EDGE_CONTEXTS:
        fail("residue edge selection contexts are not frozen")
    edges = edge_manifest.get("edges")
    if not isinstance(edges, list) or not edges:
        fail("residue edge table must be a non-empty array")
    derived_families = edge_manifest.get("derived_edge_families")
    if not isinstance(derived_families, list) or not derived_families:
        fail("derived residue edge families must be a non-empty array")
    seen_edges = set()
    edge_kinds = set()
    runtime_contexts: dict[str, set[str]] = {}

    def validate_edge(edge: dict, *, derived: bool = False) -> tuple[str, ...]:
        if not isinstance(edge, dict):
            fail("residue edge entries must be objects")
        required = ("source", "consumer", "provider", "edge_kind", "phase", "selection_context")
        if any(not isinstance(edge.get(field), str) or not edge[field].strip() for field in required):
            fail("residue edge entries require non-empty source/consumer/provider/kind/phase/context")
        edge_kind = edge["edge_kind"]
        if edge_kind not in EDGE_KINDS:
            fail(f"unsupported residue edge kind: {edge_kind}")
        if edge["phase"] not in EDGE_PHASES:
            fail(f"unsupported residue edge phase: {edge['phase']}")
        if edge["selection_context"] not in EDGE_CONTEXTS:
            fail(f"unsupported residue edge selection context: {edge['selection_context']}")
        source = Path(edge["source"])
        if source.is_absolute() or ".." in source.parts or not (REPO / source).is_file():
            fail(f"residue edge source does not exist safely: {edge['source']}")
        provider = edge["provider"]
        if edge_kind == "runtime-dlopen":
            if not DYNAMIC_NAME.fullmatch(provider) or provider not in names:
                fail(f"runtime residue edge names undeclared dynamic provider: {provider}")
            runtime_contexts.setdefault(provider, set()).add(edge["selection_context"])
        else:
            provider_path = Path(provider)
            if provider_path.is_absolute() or ".." in provider_path.parts or not (REPO / provider_path).is_file():
                fail(f"residue edge provider does not exist safely: {provider}")
        key = tuple(edge[field] for field in required)
        if key in seen_edges:
            fail(f"duplicate residue edge: {edge}")
        seen_edges.add(key)
        edge_kinds.add(edge_kind)
        return key

    for edge in edges:
        validate_edge(edge)

    paths_by_kind = {
        kind: sorted(item["path"] for item in files if item["kind"] == kind)
        for kind in DERIVED_MANIFEST_KINDS
    }
    graph_by_key = {
        key: sorted(value)
        for key, value in graph.items()
        if key in DERIVED_GRAPH_KEYS and isinstance(value, list)
    }
    derived_count = 0
    for family in derived_families:
        if not isinstance(family, dict):
            fail("derived residue edge families must be objects")
        required_family = ("source", "consumer", "edge_kind", "phase", "selection_context")
        if any(not isinstance(family.get(field), str) or not family[field].strip() for field in required_family):
            fail("derived edge families require non-empty source/consumer/kind/phase/context")
        selectors = [key for key in ("provider_manifest_kind", "provider_graph_key") if key in family]
        if len(selectors) != 1:
            fail("derived edge families require exactly one provider selector")
        selector = selectors[0]
        if selector == "provider_manifest_kind":
            provider_key = family[selector]
            if provider_key not in DERIVED_MANIFEST_KINDS:
                fail(f"unsupported derived manifest kind: {provider_key}")
            providers = paths_by_kind[provider_key]
        else:
            provider_key = family[selector]
            if provider_key not in DERIVED_GRAPH_KEYS or provider_key not in graph_by_key:
                fail(f"unsupported derived graph key: {provider_key}")
            providers = graph_by_key[provider_key]
        for provider in providers:
            edge = {
                "source": family["source"],
                "consumer": family["consumer"],
                "provider": provider,
                "edge_kind": family["edge_kind"],
                "phase": family["phase"],
                "selection_context": family["selection_context"],
            }
            validate_edge(edge, derived=True)
            derived_count += 1
    if edge_kinds != EDGE_KINDS:
        fail(f"residue edge table lacks representative kinds: {sorted(EDGE_KINDS - edge_kinds)}")
    if set(runtime_contexts) != set(names):
        fail(f"residue edge table does not cover every dynamic provider: missing={sorted(set(names) - set(runtime_contexts))}")
    for provider, contexts in runtime_contexts.items():
        if not {"production", "rollback"}.issubset(contexts):
            fail(f"dynamic provider lacks production/rollback edge contexts: {provider}")

    coverage_requirements = {
        "build-only": "build-input",
        "shared-contract": "build-input",
        "zig-oracle": "oracle",
        "test-evidence": "evidence",
    }
    for kind, edge_kind in coverage_requirements.items():
        expected = set(paths_by_kind[kind])
        observed = {
            edge["provider"]
            for edge in edges
            if edge["edge_kind"] == edge_kind
        }
        observed.update(
            edge["provider"]
            for family in derived_families
            if family["edge_kind"] == edge_kind
            for edge in (
                {"provider": provider}
                for provider in (
                    paths_by_kind[family["provider_manifest_kind"]]
                    if "provider_manifest_kind" in family
                    else graph_by_key[family["provider_graph_key"]]
                )
            )
        )
        missing = sorted(expected - observed)
        if missing:
            fail(f"{kind} edge coverage is incomplete: missing={missing}")

    native_policy_paths = set(graph.get("birnam_sources", []))
    observed_native_policy = {
        edge["provider"]
        for edge in edges
        if edge["edge_kind"] == "native-policy"
    }
    observed_native_policy.update(
        provider
        for family in derived_families
        if family["edge_kind"] == "native-policy"
        for provider in (
            paths_by_kind[family["provider_manifest_kind"]]
            if "provider_manifest_kind" in family
            else graph_by_key[family["provider_graph_key"]]
        )
    )
    missing_native_policy = sorted(native_policy_paths - observed_native_policy)
    if missing_native_policy:
        fail(f"native policy edge coverage is incomplete: missing={missing_native_policy}")

    print(
        "remaining-zig-inventory: ok "
        f"zig_files={len(paths)} birnam_sources={len(graph['birnam_sources'])} "
        f"rust_crates={len(graph['rust_crates'])} test_scripts={len(graph['test_scripts'])} "
        f"build_targets={len(graph['build_targets'])} dynamic_dependencies={len(names)} "
        f"edge_rows={len(edges) + derived_count} derived_edge_rows={derived_count}"
    )


if __name__ == "__main__":
    main()
