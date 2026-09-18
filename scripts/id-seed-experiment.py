#!/usr/bin/env python3
"""Designs 81F/81G: isolated Id seed, Integer and object-model module proofs."""

import hashlib
import json
import os
from pathlib import Path
import platform
import resource
import shutil
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "experiments/id-seed"
CASES = [
    "bootstrap-and-payload",
    "symbol-identity",
    "binding-replace-remove-parent",
    "integer-construction-read",
    "integer-checked-arithmetic",
    "integer-overflow-no-construction",
    "integer-invalid-operands",
    "integer-replace-arithmetic",
    "integer-replace-construction-allocation",
    "integer-independent-seeds",
    "private-behavior-in-upper-dylib",
    "integer-private-behavior",
    "custom-parent-lookup-with-effects",
    "replace-during-reentrant-call",
    "replace-metacircular-lookup",
    "replace-allocation-method",
    "rejected-inputs-and-cycle-recovery",
    "independent-seed-universes",
    "ordered-shutdown",
]
INTEGER_ONLY_CASES = CASES[:CASES.index("private-behavior-in-upper-dylib")] + ["integer-only-shutdown"]
ARTIFACTS = [
    "birnam-seed-probe",
    "libbirnam_seed_experiment.dylib",
    "libbirnam_object_model_experiment.dylib",
    "libbirnam_integer_experiment.dylib",
]
EXPORTS = {
    ARTIFACTS[1]: {"_birnam_seed_create_v1", "_birnam_seed_destroy_v1"},
    ARTIFACTS[2]: {"_birnam_object_model_create_v1", "_birnam_object_model_destroy_v1"},
    ARTIFACTS[3]: {"_birnam_integer_create_v1", "_birnam_integer_destroy_v1"},
}
DEPENDENCIES = {
    "birnam-seed-abi": [],
    "birnam-seed-experiment": ["birnam-seed-abi"],
    "birnam-object-model-experiment": ["birnam-seed-abi"],
    "birnam-integer-experiment": ["birnam-seed-abi"],
    "birnam-seed-probe": ["birnam-seed-abi"],
}
# Exact seed/ABI sources from 81F's successful 20260918-100341-mgyf_i33 run.
# Adding Integer must not quietly grow or change either boundary.
SEED_REFERENCE_SHA256 = {
    "abi/src/lib.rs": "75a36b36c83462ba591a58fbacd2366547741665a1134752f28a5d1464f5705b",
    "seed/src/lib.rs": "6e7e7887373023b3784067b5555cf8ae334bb0ad0ca456db006c56c1ad2bc1b6",
}


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def system_image(path):
    return path.startswith(("/usr/lib/", "/System/Library/"))


def main():
    require(platform.system() == "Darwin", "81F/81G currently require macOS/dyld")
    # Broken variants abort on purpose; do not leave large core files behind.
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    base = ROOT / ".cargo-target/id-seed-experiment"
    base.mkdir(parents=True, exist_ok=True)
    output = Path(tempfile.mkdtemp(prefix=time.strftime("%Y%m%d-%H%M%S-"), dir=base))
    receipt_path = output / "receipt.json"
    receipt = {
        "schema": "birnam-id-seed-experiment-v2",
        "design": "81G",
        "extends": ["81F"],
        "status": "running",
        "host": {"system": platform.system(), "machine": platform.machine()},
        "scope": "Rust boundary witness; no production extraction or Birnam authoring proof",
        "limitations": [
            "single owner thread; bounded storage retained until seed destruction",
            "code and method data pinned through all invocations; no dynamic retirement",
            "no collector, ownership provider, mutation observers, images or Machine",
            "no performance or minimum-possible-ABI claim",
        ],
        "commands": [],
        "runs": [],
        "negative_controls": [],
    }
    env = dict(os.environ)
    sanitized = [key for key in env if key.startswith("DYLD_") or key in {
        "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER", "CARGO_TARGET_DIR", "CARGO_BUILD_TARGET",
    }]
    for key in sanitized:
        del env[key]
    receipt["removed_environment_keys"] = sorted(sanitized)

    def save():
        receipt_path.write_text(json.dumps(receipt, indent=2) + "\n")

    def run(command, *, cwd=ROOT, timeout=120, allow_failure=False):
        command = [str(part) for part in command]
        index = len(receipt["commands"])
        result = subprocess.run(command, cwd=cwd, env=env, text=True,
                                capture_output=True, timeout=timeout)
        log = output / f"command-{index:02d}.log"
        log.write_text(result.stdout + result.stderr)
        receipt["commands"].append({"argv": command, "cwd": str(cwd),
                                    "returncode": result.returncode, "log": str(log)})
        save()
        require(allow_failure or result.returncode == 0,
                f"command failed ({result.returncode}): {command}\n{result.stdout}{result.stderr}")
        return result

    try:
        receipt["git_head"] = run(["git", "rev-parse", "HEAD"]).stdout.strip()
        receipt["git_status"] = run(["git", "status", "--short"]).stdout
        receipt["rustc"] = run(["rustc", "-Vv"]).stdout.strip()
        receipt["cargo"] = run(["cargo", "--version"]).stdout.strip()
        require(all(digest(SOURCE / name) == value for name, value in SEED_REFERENCE_SHA256.items()),
                "81G requires the unchanged 81F seed and ABI sources")
        receipt["unchanged_seed_sha256"] = SEED_REFERENCE_SHA256
        # Only these source files are copied; no production implementation can
        # enter the build through an accidentally inherited workspace member.
        files = sorted(path for path in SOURCE.rglob("*") if path.is_file()
                       and (path.suffix == ".rs" or path.name in {"Cargo.toml", "Cargo.lock"})
                       and "target" not in path.relative_to(SOURCE).parts)
        snapshot = output / "source"
        for path in files:
            copied = snapshot / path.relative_to(SOURCE)
            copied.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, copied)
        inputs = [*files, Path(__file__).resolve()]
        receipt["source_sha256"] = {str(path.relative_to(ROOT)): digest(path) for path in inputs}
        manifest = snapshot / "Cargo.toml"
        target = output / "build"
        env["CARGO_TARGET_DIR"] = str(target)
        metadata = json.loads(run(["cargo", "metadata", "--manifest-path", manifest,
                                   "--format-version", "1", "--locked", "--offline"]).stdout)
        graph = {package["name"]: sorted(d["name"] for d in package["dependencies"])
                 for package in metadata["packages"]}
        require(graph == DEPENDENCIES, f"unexpected package graph: {graph}")
        for package in metadata["packages"]:
            require(package["source"] is None, "external package in isolated proof")
            for item in package["targets"]:
                require(Path(item["src_path"]).is_relative_to(snapshot), "source outside snapshot")
                require("custom-build" not in item["kind"], "unexpected build script")
        receipt["dependency_graph"] = graph
        (output / "cargo-metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
        run(["cargo", "fmt", "--manifest-path", manifest, "--all", "--check"])
        run(["cargo", "clippy", "--manifest-path", manifest, "--workspace", "--locked", "--offline", "--", "-D", "warnings"])

        def build(workspace, profile):
            command = ["cargo", "build", "--manifest-path", workspace / "Cargo.toml",
                       "--workspace", "--locked", "--offline"]
            if profile == "release":
                command.append("--release")
            run(command)

        for profile in ("debug", "release"):
            print(f"id-seed: {profile} build, dependency audit and semantic probe", flush=True)
            build(snapshot, profile)
            prefix = output / profile
            prefix.mkdir()
            audits = {}
            for name in ARTIFACTS:
                artifact = prefix / name
                shutil.copy2(target / profile / name, artifact)
                # otool -L includes a dylib's own LC_ID_DYLIB; it is not a load
                # dependency. Keep the exact identity in the receipt separately.
                identity = None
                if name.endswith(".dylib"):
                    ids = run(["otool", "-D", artifact]).stdout.splitlines()[1:]
                    require(len(ids) == 1, f"unexpected dylib identity: {ids}")
                    identity = ids[0].strip()
                dependencies = [line.strip().split(" (compatibility version", 1)[0]
                                for line in run(["otool", "-L", artifact]).stdout.splitlines()[1:]]
                dependencies = [name for name in dependencies if name != identity]
                require(all(system_image(name) for name in dependencies),
                        f"non-system dependency in {artifact}: {dependencies}")
                imports = run(["nm", "-u", artifact]).stdout
                require("birnam" not in imports.lower(), f"Birnam import in {artifact}")
                exported = None
                if name in EXPORTS:
                    exported = {line.split()[-1] for line in run(["nm", "-gU", artifact]).stdout.splitlines() if line.split()}
                    require(exported == EXPORTS[name], f"unexpected exports in {artifact}: {exported}")
                audits[name] = {"sha256": digest(artifact), "install_name": identity,
                                "dependencies": dependencies, "undefined_symbols": imports.splitlines(),
                                "exports": sorted(exported) if exported is not None else None}
            for variant in ("full", "integer-only"):
                if variant == "integer-only":
                    executed = prefix / variant
                    executed.mkdir()
                    names = [ARTIFACTS[index] for index in (0, 1, 3)]
                    for name in names:
                        shutil.copy2(prefix / name, executed / name)
                        require(digest(executed / name) == audits[name]["sha256"], "copied artifact differs")
                    require({p.name for p in executed.iterdir()} == set(names), "extra Integer-only artifact")
                    expected_cases = INTEGER_ONLY_CASES
                    object_model = "-"
                else:
                    executed, names, expected_cases = prefix, ARTIFACTS, CASES
                    object_model = prefix / ARTIFACTS[2]
                result = run([executed / ARTIFACTS[0], executed / ARTIFACTS[1], object_model,
                              executed / ARTIFACTS[3]], cwd=executed, timeout=30)
                lines = result.stdout.splitlines()
                passed = [line.removeprefix("PASS\t") for line in lines if line.startswith("PASS\t")]
                started = [line.removeprefix("BEGIN\t") for line in lines if line.startswith("BEGIN\t")]
                require(started == expected_cases and passed == expected_cases,
                        f"incomplete/duplicate cases: {started} / {passed}")
                images = [line.removeprefix("IMAGE\t") for line in lines if line.startswith("IMAGE\t")]
                require(bool(images), "missing runtime image inventory")
                expected = {str(executed / name) for name in names}
                actual = {name for name in images if not system_image(name)}
                require(actual == expected, f"unexpected mapped non-system images: {actual}")
                receipt["runs"].append({"profile": profile, "variant": variant, "status": "passed", "cases": passed,
                                        "artifacts": {name: audits[name] for name in names}, "loaded_images": images})
                save()

        # Mutation controls edit disposable snapshots, never production sources
        # or the normal seed. The good probe/upper library remain unchanged.
        parent_message = """unsafe {
        send(
            api,
            parent,
            seed.selectors.lookup,
            args.as_ptr(),
            args.len(),
            out,
        )
    }"""
        root_binding = """match seed.direct(seed.roots.vtable, selector) {
            Ok((Some(method), _)) => method,
            Ok(_) => return UNBOUND,
            Err(e) => return e,
        }"""
        controls = [
            ("raw-parent-walk", "custom-parent-lookup-with-effects", "seed", 1, parent_message,
             """let mut cursor = parent;
             for _ in 0..MAX_DEPTH {
                 match seed.direct(cursor, args[0].a) {
                     Ok((Some(method), _)) => return unsafe { answer(out, Value::method(method)) },
                     Ok((None, 0)) => return unsafe { answer(out, Value::none()) },
                     Ok((None, next)) => cursor = next,
                     Err(e) => return e,
                 }
             }
             RECURSION"""),
            ("fixed-root-lookup", "replace-metacircular-lookup", "seed", 1, root_binding, "Method::plain(lookup)"),
            ("wrapping-integer-add", "integer-checked-arithmetic", "integer", 3,
             "i64::checked_add", "|a, b| Some(a.wrapping_add(b))"),
        ]
        for name, expected_case, crate, artifact_index, before, after in controls:
            print(f"id-seed: negative control {name}", flush=True)
            source_relative = Path(crate) / "src/lib.rs"
            source_text = (snapshot / source_relative).read_text()
            require(source_text.count(before) == 1, f"mutation anchor changed: {name}")
            mutant = output / name
            shutil.copytree(snapshot, mutant)
            (mutant / source_relative).write_text(source_text.replace(before, after))
            env["CARGO_TARGET_DIR"] = str(mutant / "build")
            run(["cargo", "build", "--manifest-path", mutant / "Cargo.toml", "-p", f"birnam-{crate}-experiment", "--locked", "--offline"])
            paths = [output / "debug" / artifact for artifact in ARTIFACTS]
            paths[artifact_index] = mutant / "build/debug" / ARTIFACTS[artifact_index]
            result = run(paths, cwd=mutant, timeout=30, allow_failure=True)
            started = [line.removeprefix("BEGIN\t") for line in result.stdout.splitlines() if line.startswith("BEGIN\t")]
            passed = [line.removeprefix("PASS\t") for line in result.stdout.splitlines() if line.startswith("PASS\t")]
            preceding = CASES[:CASES.index(expected_case)]
            require(result.returncode != 0 and started == [*preceding, expected_case] and passed == preceding,
                    f"negative control did not fail at expected case {expected_case}: {result.stdout}{result.stderr}")
            receipt["negative_controls"].append({"name": name, "status": "detected", "case": expected_case,
                                                 "returncode": result.returncode,
                                                 "mutated_source": str(source_relative),
                                                 "mutated_source_sha256": digest(mutant / source_relative)})
            save()
        require(all(digest(ROOT / name) == value for name, value in receipt["source_sha256"].items()),
                "source changed while experiment was running")
        receipt["status"] = "passed"
        print(f"id-seed: PASS ({len(CASES)} full + {len(INTEGER_ONLY_CASES)} Integer-only cases x 2 profiles; {len(controls)} detected negative controls)", flush=True)
    except BaseException as error:
        receipt["status"] = "failed"
        receipt["error"] = str(error)
        raise
    finally:
        save()
        print(f"id-seed: receipt {receipt_path}", flush=True)


if __name__ == "__main__":
    main()
