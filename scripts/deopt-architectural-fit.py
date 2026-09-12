#!/usr/bin/env python3
"""74O bounded architectural-fit probes; no production admission or timing claim."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import tempfile

from compatibility_authority import unchanged_authority

ROOT = Path(__file__).resolve().parent.parent
BASE = "24e200a3"
AUTHORITIES = (
    "rust/birnam-abi/src/lib.rs", "abi/idapi-v1.json",
    "abi/fixtures/idapi-v1/idapi_v1.h", "rust/birnam-image/src/lib.rs",
    "rust/birnam-image/src/v8.rs", "rust/birnam-artifact/src/frame_descriptor.rs",
    "rust/birnam-id/src/registration.rs", "rust/birnam-codegen/src/resumable_c.rs",
)
PREFIXES = ("ARCHITECTURAL_FIT ", "ARCHITECTURAL_FIT_OBJECT ",
            "ARCHITECTURAL_FIT_CONTINUATION ")


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def manifest():
    paths = [ROOT / "Cargo.toml", ROOT / "Cargo.lock", Path(__file__).resolve(),
             ROOT / "scripts/fixtures/deopt-recovery-policy.birnam"]
    for folder in ("rust", "src/id", "modules"):
        paths.extend(path for path in (ROOT / folder).rglob("*") if path.is_file())
    return {str(path.relative_to(ROOT)): sha(path) for path in sorted(paths)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work-dir", type=Path, default=ROOT / ".cargo-target/74o-fit")
    parser.add_argument("--output", type=Path, default=ROOT / "docs/measurements/74o-deoptimization-fit.json")
    parser.add_argument("--verify", action="store_true", help="also run the full production gate")
    args = parser.parse_args()
    work = args.work_dir.resolve()
    work.mkdir(parents=True, exist_ok=True)
    env = {key: value for key, value in os.environ.items()
           if not key.startswith(("BIRNAM_", "DYLD_", "NATIVE_TIER_"))}
    env.update(CARGO_TARGET_DIR=str(ROOT / ".cargo-target"), MISE_TRUSTED_CONFIG_PATHS=str(ROOT))
    result = {"design": "74O", "status": "running", "platform": platform.platform(),
              "scope": "test-only reconstruction and semantic counterexamples; no speed qualification",
              "commands": [], "sources": manifest(), "authorities": {}}

    def run(command, name, additions=None, timeout=1800):
        command = [str(part) for part in command]
        print(f"74O: {name}", file=sys.stderr, flush=True)
        completed = subprocess.run(command, cwd=ROOT, env=dict(env, **(additions or {})),
                                   capture_output=True, text=True, timeout=timeout)
        log = completed.stdout + completed.stderr
        (work / f"{name}.log").write_text(log)
        result["commands"].append({"name": name, "argv": command,
                                   "environment_additions": additions or {},
                                   "exit_code": completed.returncode,
                                   "output": log})
        if completed.returncode:
            raise RuntimeError(f"{name} failed; see {work / (name + '.log')}")
        return completed.stdout

    try:
        for name in AUTHORITIES:
            baseline = subprocess.check_output(["git", "show", f"{BASE}:{name}"], cwd=ROOT)
            assert unchanged_authority(name, (ROOT / name).read_bytes(), baseline), f"frozen authority changed: {name}"
            result["authorities"][name] = sha(ROOT / name)
        result["base"] = subprocess.check_output(["git", "rev-parse", BASE], cwd=ROOT, text=True).strip()
        result["head"] = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
        cargo = ["cargo", "test", "-p", "birnam-id", "--locked", "--offline"]
        facts_dir = work / "compiler-facts"
        run(cargo + ["reconstructs_compiler_colored", "--", "--nocapture"], "compiler-facts",
            {"BIRNAM_74O_DUMP": str(facts_dir)})
        facts = [[int(field) for field in row.split()]
                 for row in (facts_dir / "callee.facts").read_text().splitlines()]
        slot_count, nil_spill = facts[0]
        live = facts[1:]
        result["compiler_facts"] = {path.name: {"sha256": sha(path), "hex": path.read_bytes().hex()}
                                    for path in sorted(facts_dir.iterdir()) if path.is_file()}
        # The normal installer deliberately rejects prefixes inside its source
        # tree. Use an isolated external prefix, never the user's installation.
        prefix = Path(tempfile.mkdtemp(prefix="birnam-74o-prefix-")) / "install"
        run(["bash", "scripts/rust-install.sh", "install", "--root", ROOT,
             "--prefix", prefix, "--profile", "release"], "policy-prefix-build")
        result["policy_prefix"] = {str(path.relative_to(prefix)): sha(path)
                                   for folder in ("bin", "lib")
                                   for path in sorted((prefix / folder).rglob("*")) if path.is_file()}
        policy = (ROOT / "scripts/fixtures/deopt-recovery-policy.birnam").read_text()
        literal = "#[" + " ".join(f"#[{slot} {spill}]" for slot, spill in live) + "]"
        produced = {}
        for mode in ("ordinary", "portable"):
            producer = "recoveryRecipe"
            source = policy
            if mode == "portable":
                source += "\nlet compiledPolicy := [Codegen materializeResumable: [[recoveryRecipe codeObject] withoutSourceProvenance] captures: #[]];\n"
                producer = "compiledPolicy"
            source += f"\nlet recipe := [{producer} value: {slot_count} value: {nil_spill} value: {literal}];\n"
            source += '[recipe do: [:row | [System println: [[[row at: 0] printString] , " " , [[row at: 1] printString]]]]];\n'
            path = work / f"policy-{mode}.birnam"
            path.write_text(source)
            output = run([prefix / "bin/birnam", path], f"policy-{mode}")
            produced[mode] = {"source": source, "output": output}
        assert produced["ordinary"]["output"] == produced["portable"]["output"]
        expected = dict(enumerate([nil_spill] * slot_count))
        expected.update(live)
        expected_text = "".join(f"{slot} {spill}\n" for slot, spill in expected.items())
        assert produced["portable"]["output"] == expected_text
        recipe = work / "id-produced.recipe"
        recipe.write_text(produced["portable"]["output"])
        result["id_producer"] = produced
        result["profiles"] = {}
        for profile in ("debug", "release"):
            command = cargo + (["--release"] if profile == "release" else [])
            output = run(command + ["architectural_fit", "--", "--nocapture"], profile,
                         {"BIRNAM_74O_RECIPE": str(recipe)})
            rows = []
            for line in output.splitlines():
                for prefix_text in PREFIXES:
                    if line.startswith(prefix_text):
                        rows.append({"kind": prefix_text.strip(), "data": json.loads(line[len(prefix_text):])})
            rows.sort(key=lambda row: json.dumps(row, sort_keys=True))
            assert len(rows) == 24, f"expected 22 execution rows and two semantic summaries: {len(rows)}"
            assert "5 passed; 0 failed" in output
            result["profiles"][profile] = rows
        assert result["profiles"]["debug"] == result["profiles"]["release"]
        if args.verify:
            run(["mise", "run", "verify"], "full-verification", timeout=3600)
            result["full_verification"] = "passed"
        else:
            result["full_verification"] = "not requested"
        assert manifest() == result["sources"], "source changed during experiment"
        result["status"] = "passed"
    except Exception as error:
        result["status"] = "failed"
        result["error"] = str(error)
        raise
    finally:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(f"74O: passed; receipt {args.output}")


if __name__ == "__main__":
    main()
