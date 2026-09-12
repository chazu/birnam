#!/usr/bin/env python3
"""74T recovery and numeric preparation evidence; no timing claim."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
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
    )]
    paths += [ROOT / path for path in (
        "abi/native-numeric-semantics-v1.md", "scripts/fixtures/numeric-semantics.c",
        "scripts/fixtures/numeric-specialization.birnam",
        "scripts/fixtures/native-numeric-api-context.c",
    )]
    for folder in ("rust", "src/id", "modules"):
        paths.extend(p for p in (ROOT / folder).rglob("*") if p.is_file())
    return {str(p.relative_to(ROOT)): sha(p) for p in sorted(paths)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verify", action="store_true")
    parser.add_argument("--numeric", action="store_true")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.output is None:
        args.output = ROOT / "docs/measurements" / (
            "74t-numeric-preparation.json" if args.numeric else "74t-recovery-graphs.json")
    work = ROOT / ".cargo-target/74t-recovery"
    work.mkdir(parents=True, exist_ok=True)
    env = {k: v for k, v in os.environ.items() if not k.startswith(("BIRNAM_", "DYLD_"))}
    env.update(CARGO_TARGET_DIR=str(ROOT / ".cargo-target"), MISE_TRUSTED_CONFIG_PATHS=str(ROOT))
    receipt = {
        "design": "74T", "status": "running", "sources": sources(), "commands": [],
        "scope": ("Numeric declarations, guarded boxed calls and raw kernel preparation; raw chain production integration remains pending"
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
            output = run(["mise", "exec", "--", "cargo", "test", "--locked", "--offline",
                          "-p", "birnam-id", "-p", "birnam-codegen", "-p", "birnam-prelude", "--no-run", "--message-format=json"]
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
            if args.numeric:
                checks += [
                    ("birnam_codegen", "numeric-plan", "numeric_chain::tests"),
                    ("birnam_id", "numeric-kernels", "runtime::numeric::tests"),
                    ("birnam_id", "numeric-driver", "runtime::native_tier::leaf_tests::numeric_tests"),
                    ("birnam_prelude", "numeric-declarations", "numeric_declarations_name_exact_implementations"),
                ]
            for package, label, test_filter in checks:
                output = run([binaries[package], test_filter, "--nocapture"], profile + "-" + label)
                match = re.search(r"test result: ok\. (\d+) passed; 0 failed", output)
                if not match or int(match[1]) == 0:
                    raise RuntimeError("no passing tests for " + label)
        if args.verify:
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
