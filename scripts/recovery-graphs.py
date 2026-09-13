#!/usr/bin/env python3
"""74T recovery and numeric preparation evidence; no timing claim."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
from compatibility_authority import unchanged_authority

ROOT = Path(__file__).resolve().parent.parent
BASE = "ff71ec23"
AUTHORITIES = (
    "rust/birnam-abi/src/lib.rs", "abi/idapi-v1.json", "abi/fixtures/idapi-v1/idapi_v1.h",
    "rust/birnam-artifact/src/frame_descriptor.rs", "rust/birnam-id/src/registration.rs",
    "rust/birnam-codegen/src/resumable_c.rs", "rust/birnam-image/src/lib.rs", "rust/birnam-image/src/v8.rs",
)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def sources():
    paths = [ROOT / "Cargo.toml", ROOT / "Cargo.lock", Path(__file__).resolve()]
    paths += [ROOT / "scripts" / name for name in (
        "compatibility_authority.py", "rust-cutover-verify.sh",
        "rust-driver-native-smoke.sh", "rust-driver-cold-archive-smoke.sh",
        "closure-elision-smoke.sh",
        "closure-control-smoke.sh", "closure-sequence-smoke.py",
        "unboxed-numeric-smoke.py", "unboxed-numeric-control-smoke.py",
        "closure-sequence-mutation-smoke.py",
        "closure-budget-smoke.py",
        "closure-journal-smoke.py",
        "closure-measurement-smoke.py",
        "closure-timing.py", "closure_timing_common.py",
    )]
    paths += [ROOT / path for path in (
        "abi/native-numeric-semantics-v1.md", "scripts/fixtures/numeric-semantics.c",
        "scripts/fixtures/numeric-specialization.birnam",
        "scripts/fixtures/native-numeric-api-context.c",
        "abi/native-field-projection-v1.md", "scripts/fixtures/field-projection.c",
        "abi/native-block-construction-v1.md", "scripts/fixtures/native-block-construction.c",
        "abi/fixtures/portable-code-v1/program-explicit-captures.bin",
        "scripts/fixtures/closure-elision.birnam",
        "scripts/fixtures/closure-observation.birnam",
        "scripts/fixtures/closure-control.birnam",
        "scripts/fixtures/closure-live-api.birnam",
        "scripts/fixtures/closure-timing.birnam",
        "scripts/fixtures/closure-api-context.c",
    )]
    for folder in ("rust", "src/id", "modules"):
        paths.extend(p for p in (ROOT / folder).rglob("*") if p.is_file())
    return {str(p.relative_to(ROOT)): sha(p) for p in sorted(paths)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify", action="store_true")
    parser.add_argument("--unboxed", action="store_true", help="experimental unboxed source and V2 recovery verification")
    parser.add_argument("--numeric", action="store_true")
    parser.add_argument("--chains", action="store_true", help="production raw-chain verification")
    parser.add_argument("--tails", action="store_true", help="registered tail-loop entry verification")
    parser.add_argument("--allocations", action="store_true", help="discarded aggregate allocation and GC verification")
    parser.add_argument("--fields", action="store_true", help="live aggregate projection and physical recovery verification")
    parser.add_argument("--closures", action="store_true", help="experimental source closure elimination and recovery verification")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.unboxed:
        args.closures = True
    if args.closures:
        args.fields = True
    if args.fields:
        args.allocations = True
    if args.allocations:
        args.tails = True
    if args.tails:
        args.chains = True
    if args.chains:
        args.numeric = True
    if args.output is None:
        args.output = ROOT / "docs/measurements" / (
            "74v-unboxed-numeric.json" if args.unboxed else
            "74t-closure-plans.json" if args.closures else
            "74t-virtual-fields.json" if args.fields else
            "74t-discarded-clones.json" if args.allocations else
            "74t-tail-entry.json" if args.tails else
            "74t-numeric-chains.json" if args.chains else
            "74t-numeric-preparation.json" if args.numeric else "74t-recovery-graphs.json")
    work = ROOT / ".cargo-target/74t-recovery"
    work.mkdir(parents=True, exist_ok=True)
    env = {k: v for k, v in os.environ.items() if not k.startswith(("BIRNAM_", "DYLD_"))}
    env.update(CARGO_TARGET_DIR=str(ROOT / ".cargo-target"), MISE_TRUSTED_CONFIG_PATHS=str(ROOT))
    receipt = {
        "design": "74T", "status": "running", "sources": sources(), "commands": [],
        "scope": ("Experimental source numeric box elimination and V2 identity recovery; default-off and broader acceptance pending"
                  if args.unboxed else "Experimental source closure allocation elimination, guarded capture substitution and identity recovery; disabled by default and broader acceptance pending"
                  if args.closures else "Bounded production live aggregate field projection and identity recovery; broader acceptance remains open"
                  if args.fields else "Bounded repeated discarded-clone consumer and dead-allocation accounting; live aggregate replacement and remaining family obligations in progress"
                  if args.allocations else "Registered tail-loop entry plans; ordinary lookup, allocation, Frame identity and stackless control retained"
                  if args.tails else "Production raw chains with per-call operand and implementation revalidation; profitability and remaining families in progress"
                  if args.chains else "Numeric declarations, guarded boxed calls and raw kernel preparation"
                  if args.numeric else "Recovery graph infrastructure; four optimizing families remain in progress"),
        "timing_qualification": "not measured", "authorities": {}, "binaries": {},
    }

    def save():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")

    def run(argv, label):
        print("74T: " + label, flush=True)
        argv = [str(a) for a in argv]
        completed = subprocess.run(argv, cwd=ROOT, env=env, capture_output=True, text=True)
        output = completed.stdout + completed.stderr
        path = work / (label + ".log")
        path.write_text(output)
        receipt["commands"].append({"label": label, "argv": argv,
                                    "exit_code": completed.returncode, "output": output})
        save()
        if completed.returncode:
            raise RuntimeError(f"{label} failed; see {path}")
        return completed.stdout

    save()
    try:
        for name in AUTHORITIES:
            data = (ROOT / name).read_bytes()
            baseline = subprocess.check_output(["git", "show", f"{BASE}:{name}"], cwd=ROOT)
            if not unchanged_authority(name, data, baseline):
                raise RuntimeError("changed compatibility authority: " + name)
            receipt["authorities"][name] = sha(ROOT / name)
        for profile in ("debug", "release"):
            if args.closures:
                run(["mise", "exec", "--", "cargo", "build", "--locked", "--offline",
                     "-p", "birnam-id", "-p", "birnam-prelude", "-p", "birnam-collection",
                     "-p", "birnam-error", "-p", "birnam-portable-code",
                     "-p", "birnam-codegen-runtime", "-p", "birnam-tcc"]
                    + (["--release"] if profile == "release" else []), profile + "-closure-native-build")
                fixture = work / (profile + "-block-construction")
                run(["cc", "-Wall", "-Wextra", "-Werror", "-Iabi/fixtures/idapi-v1",
                     "scripts/fixtures/native-block-construction.c", "-o", fixture], profile + "-closure-native-fixture")
                libraries = ROOT / ".cargo-target" / profile
                run([fixture] + [libraries / ("libbirnam_" + name + ".dylib") for name in
                    ("id", "prelude", "collection", "error", "portable_code")]
                    + ["abi/fixtures/portable-code-v1/program-explicit-captures.bin"], profile + "-closure-native-admission")
                run([fixture] + [libraries / ("libbirnam_" + name + ".dylib") for name in
                    ("id", "prelude", "collection", "error", "portable_code")]
                    + ["abi/fixtures/portable-code-v1/program-explicit-captures.bin", "without-prelude"], profile + "-closure-native-absent-family")
                api_fixture = work / (profile + "-closure-api-context")
                run(["cc", "-Wall", "-Wextra", "-Werror", "-Iabi/fixtures/idapi-v1",
                     "scripts/fixtures/closure-api-context.c", "-o", api_fixture], profile + "-closure-api-fixture")
                run([api_fixture] + [libraries / ("libbirnam_" + name + ".dylib") for name in
                    ("id", "tcc", "codegen_runtime")], profile + "-closure-api-context")
            output = run(["mise", "exec", "--", "cargo", "test", "--locked", "--offline",
                          "-p", "birnam-id", "-p", "birnam-codegen", "-p", "birnam-prelude", "-p", "birnam-relational", "--no-run", "--message-format=json"]
                         + (["--release"] if profile == "release" else []), profile + "-build")
            binaries = {}
            for line in output.splitlines():
                row = json.loads(line)
                if row.get("reason") == "compiler-artifact" and row.get("executable") and row["profile"]["test"]:
                    binaries[row["target"]["name"]] = Path(row["executable"])
            receipt["binaries"][profile] = {name: {"path": str(path), "sha256": sha(path)}
                                              for name, path in binaries.items()}
            checks = [
                ("birnam_codegen", "shape", "recovery_graph::tests"),
                ("birnam_id", "materialization", "runtime::recovery::graph::tests"),
                ("birnam_id", "id-producer", "runtime::native_tier::leaf_tests::graph_tests"),
                ("birnam_id", "dense-recovery", "production_recovery_admits_id_recipe"),
            ]
            if args.closures:
                checks.append(("birnam_codegen", "closure-plan", "closure_elision::"))
                checks.append(("birnam_id", "closure-context", "closure_context_requires_active_nil"))
                checks.append(("birnam_id", "closure-staging", "disabled_closure_mode_skips_preparation"))
            if args.numeric:
                checks += [
                    ("birnam_codegen", "numeric-plan", "numeric_chain::"),
                    ("birnam_id", "numeric-kernels", "runtime::numeric::tests"),
                    ("birnam_id", "numeric-driver", "runtime::native_tier::leaf_tests::numeric_tests"),
                    ("birnam_prelude", "numeric-declarations", "numeric_declarations_name_exact_implementations"),
                ]
            if args.unboxed:
                checks.append(("birnam_id", "unboxed-numeric", "runtime::native_tier::leaf_tests::unboxed_numeric_tests"))
            if args.chains:
                checks.append(("birnam_id", "numeric-chain-storage", "runtime::numeric::chain::tests"))
                checks.append(("birnam_id", "numeric-fusion", "runtime::native_tier::leaf_tests::fused_numeric_tests"))
            if args.tails:
                checks += [
                    ("birnam_codegen", "tail-entry-plan", "tail_entry::tests"),
                    ("birnam_id", "tail-entry-arena", "execution::tail_entry::tests"),
                    ("birnam_id", "tail-entry-loop", "runtime::native_tier::leaf_tests::tail_entry_tests"),
                ]
            if args.allocations:
                checks += [
                    ("birnam_codegen", "discarded-clone-plan", "discarded_clone::tests"),
                    ("birnam_id", "dead-allocation-journal", "heap::dead_allocations::tests"),
                    ("birnam_id", "discarded-clone-driver", "runtime::native_tier::leaf_tests::discarded_clone_tests"),
                ]
            if args.fields:
                checks += [
                    ("birnam_codegen", "virtual-field-plan", "virtual_aggregate::tests"),
                    ("birnam_id", "virtual-field-driver", "runtime::native_tier::leaf_tests::virtual_aggregate_tests"),
                    ("birnam_id", "virtual-field-layout", "all_oop_layout_checks_current_prefix_tail_and_exact_vtable"),
                    ("birnam_relational", "native-field-declaration", "native_field_projection_declares_only_the_exact_pure_getter"),
                ]
            for package, label, test_filter in checks:
                output = run([binaries[package], test_filter, "--nocapture"], profile + "-" + label)
                match = re.search(r"test result: ok\. (\d+) passed; 0 failed", output)
                if not match or int(match[1]) == 0:
                    raise RuntimeError("no passing tests for " + label)
        if args.verify:
            if args.closures:
                with tempfile.TemporaryDirectory(prefix="closure-source-", dir=work) as evidence:
                    env["BIRNAM_TEST_CLOSURE_RECEIPTS"] = evidence
                    if args.unboxed:
                        unboxed_evidence = Path(evidence) / "unboxed"
                        unboxed_evidence.mkdir()
                        env["BIRNAM_TEST_UNBOXED_RECEIPTS"] = str(unboxed_evidence)
                    try:
                        output = run(["mise", "run", "verify"], "production-verification")
                    finally:
                        env.pop("BIRNAM_TEST_CLOSURE_RECEIPTS", None)
                        env.pop("BIRNAM_TEST_UNBOXED_RECEIPTS", None)
                    expected = {f"{family}-{profile}.json" for family in ("sequence", "mutation", "budget", "journal", "measure")
                                for profile in ("debug", "release")}
                    paths = list(Path(evidence).glob("*.json"))
                    if {p.name for p in paths} != expected:
                        raise RuntimeError("missing source closure matrix receipts")
                    matrices = {p.name: json.loads(p.read_text()) for p in sorted(paths)}
                    if any(m["status"] != "passed" for m in matrices.values()):
                        raise RuntimeError("source closure matrix did not pass")
                    for profile in ("debug", "release"):
                        hashes = {m["floor_sha256"] for name, m in matrices.items() if name.endswith(f"-{profile}.json")}
                        if len(hashes) != 1 or next(iter(hashes)) not in output:
                            raise RuntimeError("source matrix floor differs from control verification")
                    receipt["closure_source_matrices"] = matrices
                    if args.unboxed:
                        expected_unboxed = {f"{family}-{p}.json" for family in ("unboxed", "unboxed-captured", "unboxed-control") for p in ("debug", "release")}
                        paths = list(unboxed_evidence.glob("*.json"))
                        if {p.name for p in paths} != expected_unboxed:
                            raise RuntimeError("missing source unboxed matrix receipts")
                        unboxed = {p.name: json.loads(p.read_text()) for p in sorted(paths)}
                        for profile in ("debug", "release"):
                            matrix = unboxed[f"unboxed-{profile}.json"]
                            expected_floor = matrices[f"sequence-{profile}.json"]["floor_sha256"]
                            if matrix["status"] != "passed" or matrix["floor_sha256"] != expected_floor:
                                raise RuntimeError("source unboxed matrix failed or floor differs")
                            if len(matrix["rows"]) != 146 or not matrix["production_probe_absent"]:
                                raise RuntimeError("incomplete source unboxed matrix")
                            captured = unboxed[f"unboxed-captured-{profile}.json"]
                            if captured["status"] != "passed" or captured["floor_sha256"] != expected_floor:
                                raise RuntimeError("source captured numeric matrix failed or floor differs")
                            if len(captured["rows"]) != 146 or not captured["production_probe_absent"] or not captured["captured_inputs"]:
                                raise RuntimeError("incomplete source captured numeric matrix")
                            control = unboxed[f"unboxed-control-{profile}.json"]
                            if control["status"] != "passed" or control["floor_sha256"] != expected_floor:
                                raise RuntimeError("source unboxed control matrix failed or floor differs")
                            if len(control["rows"]) != 448:
                                raise RuntimeError("incomplete source unboxed control matrix")
                        receipt["unboxed_source_matrices"] = unboxed
            else:
                run(["mise", "run", "verify"], "production-verification")
        if receipt["sources"] != sources():
            raise RuntimeError("sources changed during verification")
        receipt["status"] = "passed"
        receipt["production_verification"] = "passed" if args.verify else "not run"
    except BaseException as error:
        receipt["status"] = "failed"
        receipt["error"] = str(error)
        raise
    finally:
        save()


if __name__ == "__main__":
    main()
