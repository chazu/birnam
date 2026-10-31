#!/usr/bin/env python3
"""Check the inventory experiment using copied libraries and an optional early seed build."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--early", action="store_true")
    args = parser.parse_args()
    prefix, output = args.prefix.resolve(), args.output.resolve()
    project = output / "probe"
    (project / "src").mkdir(parents=True, exist_ok=True)
    env = {k: v for k, v in os.environ.items()
           if not k.startswith(("BIRNAM_", "QUAL_")) and k != "CARGO_TARGET_DIR"}
    env["MISE_TRUSTED_CONFIG_PATHS"] = str(ROOT)
    commands = []

    def run(label, argv, *, cwd=ROOT, overrides=None, input=None):
        commands.append({"label": label, "argv": list(map(str, argv)),
                         "cwd": str(cwd), "environment": overrides or {}})
        with (output / f"{label}.log").open("w") as log:
            subprocess.run(argv, cwd=cwd, env=dict(env, **(overrides or {})),
                           stdout=log, stderr=subprocess.STDOUT, text=True,
                           input=input, check=True)

    dependencies = "".join(
        f'{name} = {{ path = {json.dumps(str(ROOT / "rust" / name))} }}\n'
        for name in ["birnam-abi", "birnam-siphon", "birnam-host-support", "birnam-codegen"])
    (project / "Cargo.toml").write_text(
        '[package]\nname="inventory-qualification"\nversion="0.0.0"\nedition="2024"\n'
        '[workspace]\n[dependencies]\n' + dependencies +
        'serde_json="=1.0.149"\n[profile.release]\npanic="abort"\n')
    shutil.copyfile(HERE / "qualification.rs", project / "src/main.rs")
    manifest = ["--manifest-path", str(project / "Cargo.toml")]
    run("lock", ["mise", "exec", "--", "cargo", "generate-lockfile", "--offline", *manifest])
    run("build", ["mise", "exec", "--", "cargo", "build", "--release", "--offline", "--locked", *manifest])
    binary = project / "target/release/inventory-qualification"
    for label, mode, depth in [("synchronous", "compileSynchronous:", None),
                               ("resumable", "compile:", None),
                               ("chain4095", "compile:", 4095),
                               ("chain4096", "compile:", 4096)]:
        overrides = {"QUAL_MODE": mode}
        if depth is not None:
            overrides["QUAL_CHAIN_DEPTH"] = str(depth)
        run(label, [binary, prefix, HERE / "inventory.birnam", output / f"{label}.json"],
            overrides=overrides)
    if args.early:
        source, early, target = (output / name for name in ["source", "early-prefix", "early-build"])
        if source.exists() or early.exists():
            raise SystemExit("Use a fresh output directory for --early.")
        run("worktree", ["git", "worktree", "add", "--detach", source, "HEAD"])
        patch = (HERE / "early-bootstrap.patch").read_text().replace(
            "__INVENTORY_SOURCE__", str(HERE / "inventory.birnam"))
        run("patch", ["git", "apply", "-"], cwd=source, input=patch)
        run("early-build", ["mise", "exec", "--", "cargo", "build", "--release",
            "--offline", "--locked", "-p", "birnam-bootstrap", "-p", "birnam-lang-seed",
            "--features", "lang-seed"], cwd=source,
            overrides={"CARGO_TARGET_DIR": str(target)})
        (early / "lib").mkdir(parents=True)
        names = ["id", "root_registry", "namespace", "boolean", "nil", "collect",
                 "object_model", "word", "bytes", "execution", "string", "integer", "float",
                 "prelude", "system_output", "system_input", "environment", "file_io",
                 "storage", "vector", "map", "portable_code", "peg", "artifact", "tcc",
                 "native_backend", "codegen_runtime"]
        for name in names + ["bootstrap", "lang_seed"]:
            directory = target / "release" if name in ["bootstrap", "lang_seed"] else prefix / "lib"
            shutil.copy2(directory / f"libbirnam_{name}.dylib", early / "lib")
        # No Birnam module or image is supplied to this early initialization.
        (output / "early-inputs.json").write_text(json.dumps(
            sorted(p.name for p in (early / "lib").iterdir()), indent=2) + "\n")
        run("early", [binary, early, HERE / "inventory.birnam", output / "unused.json"],
            overrides={"QUAL_BOOTSTRAP_ONLY": "1"})
        log = (output / "early.log").read_text()
        assert "EARLY_METHOD_SOURCE_INVENTORY_OK before Error GlobalsOwner SourceRegistry" in log
        assert "BOOTSTRAP_COMPLETED" in log
    digest = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
    result = {"status": "selected prototype checks passed; production cutover unqualified",
              "commands": commands,
              "source_sha256": {name: digest(HERE / name) for name in
                  ["inventory.birnam", "qualification.rs", "qualify.py", "early-bootstrap.patch"]},
              "raw_sha256": {p.name: digest(p) for p in sorted(output.iterdir())
                             if p.is_file() and p.suffix in [".json", ".log"]},
              "id_sha256": digest(prefix / "lib/libbirnam_id.dylib")}
    (output / "receipt.json").write_text(json.dumps(result, indent=2) + "\n")
    print(f"Prototype checks passed: {output / 'receipt.json'}")


if __name__ == "__main__":
    main()
