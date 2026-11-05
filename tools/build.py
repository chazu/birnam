#!/usr/bin/env python3
"""Pinned, local-only build of the first macOS siphon slice."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import platform
import shlex
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
BUILD = ROOT / "build"


def output(*args: str) -> str:
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def run(*args: str) -> None:
    print("+", shlex.join(args), flush=True)
    subprocess.run(args, cwd=ROOT, check=True)


def configuration() -> tuple[list[str], list[str], dict]:
    pin = json.loads((ROOT / "toolchain.json").read_text())
    odin = output("odin", "version")
    clang = output("clang", "--version").splitlines()[0]
    version = output("pkg-config", "--modversion", "bdw-gc")
    libdir = Path(output("pkg-config", "--variable=libdir", "bdw-gc"))
    digest = hashlib.sha256((libdir / "libgc.1.dylib").read_bytes()).hexdigest()
    sdk = Path(pin["sdk"])
    sdk_digest = hashlib.sha256((sdk / "SDKSettings.json").read_bytes()).hexdigest()
    os.environ["SDKROOT"] = str(sdk)
    observed = {
        "platform": platform.system(), "architecture": platform.machine(),
        "odin": odin.split(" version ", 1)[-1], "clang": clang,
        "bdw_gc": version, "bdw_gc_dylib_sha256": digest,
        "sdk": str(sdk), "sdk_settings_sha256": sdk_digest,
    }
    for key, expected in pin.items():
        if observed[key] != expected:
            raise RuntimeError(f"toolchain mismatch: {key}: expected {expected!r}, got {observed[key]!r}")
    observed["macos"] = output("sw_vers", "-productVersion")
    observed["bdw_gc_library"] = str((libdir / "libgc.1.dylib").resolve())
    cflags = ["-isysroot", str(sdk), *shlex.split(output("pkg-config", "--cflags", "bdw-gc"))]
    ldflags = shlex.split(output("pkg-config", "--libs", "bdw-gc"))
    return cflags, ldflags, observed


def build() -> tuple[list[str], list[str]]:
    cflags, ldflags, observed = configuration()
    BUILD.mkdir(exist_ok=True)
    run("odin", "build", "src/siphon", "-out:build/siphon", "-o:speed", "-debug", "-vet",
        "-extra-linker-flags:" + " ".join([f for f in ldflags if f != "-lgc"] + ["-Wl,-export_dynamic"]))
    run("odin", "build", "src/seed", "-build-mode:dll", "-out:build/seed.dylib", "-o:speed", "-debug", "-vet",
        "-extra-linker-flags:" + " ".join([f for f in ldflags if f != "-lgc"] + ["-Wl,-undefined,dynamic_lookup"]))
    (BUILD / "toolchain-observed.json").write_text(json.dumps(observed, indent=2) + "\n")
    return cflags, ldflags


def test() -> None:
    cflags, ldflags = build()
    common = ["clang", "-std=c11", "-Wall", "-Wextra", "-Werror", "-Wno-unused-parameter",
              "-O2", "-g", "-Iinclude", *cflags, "-dynamiclib", "-undefined", "dynamic_lookup"]
    for name in ("contract", "handover", "after_handover", "no_entry", "fail_init", "seed_contract", "peg_contract"):
        run(*common, f"tests/{name}.c", *ldflags, "-o", f"build/{name}.dylib")
    run(*common, "examples/native/hello.c", *ldflags, "-o", "build/hello.dylib")
    run("build/siphon", "--version")
    run("build/siphon", str(BUILD / "contract.dylib"), str(BUILD / "handover.dylib"),
        str(BUILD / "after_handover.dylib"))
    hello = subprocess.run([str(BUILD / "siphon"), str(BUILD / "hello.dylib")],
                           cwd="/private/tmp", text=True, capture_output=True, check=True)
    if hello.stdout != "Hello from a native Method in Birnam's bootstrap world.\n":
        raise RuntimeError(f"unexpected native example output: {hello.stdout!r}")
    print("PASS native example from a different working directory")
    for name, expected in (("missing.dylib", "moduleOpenFailed"),
                           ("no_entry.dylib", "moduleEntryMissing"),
                           ("fail_init.dylib", "unknownName")):
        result = subprocess.run([str(BUILD / "siphon"), str(BUILD / name)], cwd=ROOT,
                                text=True, capture_output=True)
        if result.returncode != 1 or expected not in result.stderr:
            raise RuntimeError(f"bad failure path {name}: {result.returncode}: {result.stdout} {result.stderr}")
        print(f"PASS loader failure: {expected}")
    print("PASS siphon native acceptance (no source interpreter or fiber qualification)")
    run("python3", "tests/seed_test.py")
    run("build/siphon", str(BUILD / "seed.dylib"), str(BUILD / "seed_contract.dylib"))
    run("python3", "tests/peg_test.py")
    run("build/siphon", str(BUILD / "seed.dylib"), "--eval", "modules/peg.birnam",
        str(BUILD / "peg_contract.dylib"))


def main() -> None:
    os.chdir(ROOT)
    command = sys.argv[1] if len(sys.argv) == 2 else "build"
    if command == "clean":
        shutil.rmtree(BUILD, ignore_errors=True)
    elif command == "doctor":
        print(json.dumps(configuration()[2], indent=2))
    elif command == "build":
        build()
    elif command == "test":
        test()
    else:
        raise RuntimeError("usage: build.py [build|test|doctor|clean]")


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, subprocess.CalledProcessError, FileNotFoundError) as error:
        print(f"build: {error}", file=sys.stderr)
        sys.exit(1)
