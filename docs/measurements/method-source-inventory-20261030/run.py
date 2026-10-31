#!/usr/bin/env python3
"""Replay this bounded experiment against an explicit baseline prefix/source tree."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def run(argv, *, log, cwd, env=None):
    with log.open("w") as output:
        subprocess.run(argv, cwd=cwd, env=env, stdout=output,
                       stderr=subprocess.STDOUT, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, required=True)
    parser.add_argument("--source-root", type=Path, default=ROOT)
    parser.add_argument("--output", type=Path,
                        default=ROOT / ".cargo-target/measurements/method-source-inventory-20261030/replay")
    parser.add_argument("--mode", choices=["default", "synchronous", "cold"], default="default")
    args = parser.parse_args()
    source, prefix, output = (p.resolve() for p in (args.source_root, args.prefix, args.output))
    output.mkdir(parents=True, exist_ok=True)
    receipt = json.loads((prefix / "share/birnam/rust-install.json").read_text())
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source, text=True).strip()
    if receipt["source_identity"]["git_commit"] != revision:
        raise SystemExit("Select the source revision recorded by the supplied prefix.")

    probe = output / "probe"
    (probe / "src").mkdir(parents=True, exist_ok=True)
    dependencies = "".join(
        f'{name} = {{ path = {json.dumps(str(source / "rust" / name))} }}\n'
        for name in ["birnam-abi", "birnam-siphon", "birnam-host-support", "birnam-codegen"])
    (probe / "Cargo.toml").write_text(
        '[package]\nname="method-source-probe"\nversion="0.0.0"\nedition="2024"\n'
        '[workspace]\n[dependencies]\n' + dependencies +
        'serde_json="=1.0.149"\n[profile.release]\npanic="abort"\n')
    shutil.copyfile(HERE / "probe.rs", probe / "src/main.rs")
    env = {k: v for k, v in os.environ.items()
           if not k.startswith(("BIRNAM_", "INVENTORY_")) and k != "CARGO_TARGET_DIR"}
    env["MISE_TRUSTED_CONFIG_PATHS"] = os.pathsep.join([str(ROOT), str(source)])
    run(["mise", "exec", "--", "cargo", "build", "--release", "--offline",
         "--manifest-path", str(probe / "Cargo.toml")],
        log=output / "probe-build.log", cwd=source, env=env)

    if args.mode == "cold":
        # All mutation is confined to a newly created detached checkout and copies.
        cold_source, cold_prefix, target = (output / p for p in ("cold-source", "cold-prefix", "cold-target"))
        if cold_source.exists() or cold_prefix.exists():
            raise SystemExit("Use a fresh output directory for a cold replay.")
        subprocess.run(["git", "worktree", "add", "--detach", str(cold_source), revision],
                       cwd=source, check=True)
        patch = (HERE / "cold-bootstrap.patch").read_text().replace(
            "__INVENTORY_SOURCE__", str(HERE / "inventory.birnam"))
        subprocess.run(["git", "apply", "-"], cwd=cold_source, input=patch, text=True, check=True)
        cold_env = dict(env, CARGO_TARGET_DIR=str(target),
                        MISE_TRUSTED_CONFIG_PATHS=os.pathsep.join([str(ROOT), str(source), str(cold_source)]))
        run(["mise", "exec", "--", "cargo", "build", "--release", "--offline", "--locked",
             "-p", "birnam-bootstrap", "-p", "birnam-lang-seed", "--features", "lang-seed"],
            log=output / "cold-build.log", cwd=cold_source, env=cold_env)
        shutil.copytree(prefix / "lib", cold_prefix / "lib")
        for name in ["bootstrap", "lang_seed"]:
            shutil.copyfile(target / "release" / f"libbirnam_{name}.dylib",
                            cold_prefix / "lib" / f"libbirnam_{name}.dylib")
        for name in ["error", "globalsOwner", "sourceRegistry"]:
            (cold_prefix / "lib" / f"libbirnam_{name}.dylib").unlink()
        prefix = cold_prefix
        env["INVENTORY_BOOTSTRAP_ONLY"] = "1"
    else:
        env["INVENTORY_COMPILE_MODE"] = "compileSynchronous:" if args.mode == "synchronous" else "compile:"

    log = output / f"{args.mode}.log"
    run([str(probe / "target/release/method-source-probe"), str(prefix),
         str(HERE / "inventory.birnam"), str(output / f"{args.mode}.json")],
        log=log, cwd=source, env=env)
    if args.mode == "cold":
        text = log.read_text()
        assert "EARLY_METHOD_SOURCE_INVENTORY_OK before Error GlobalsOwner SourceRegistry" in text
        assert "BOOTSTRAP_COMPLETED" in text
    print(f"{args.mode} replay passed: {log}")


if __name__ == "__main__":
    main()
