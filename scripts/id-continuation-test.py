#!/usr/bin/env python3
"""Build real Id/Roots/Collect and an independently declared C continuation caller.
No installed prefix, compiler fixture, shared private implementation or carrier ABI.
"""
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / ".cargo-target/id-continuation/qualification"
BUILD = ROOT / ".cargo-target/id-continuation/build"
FIXTURE = ROOT / "tests/fixtures/oop-continuation"
MARKERS = ["INCOMPATIBLE_V3_REFUSED", "CPS_NATIVE_ECHO_OK", "CONTINUATION_AS_DATA_OK",
           "MIXED_NATIVE_CONVENTIONS_OK", "PAUSED_ROOT_HANDOFF_OK", "NATIVE_BARRIER_AND_LATE_REPLY_OK",
           "OVERLAPPING_DRIVE_REFUSED", "SELECTED_ENTRY_ROOTS_AND_NO_RELOOKUP_OK",
           "CUSTOM_LOOKUP_PAUSE_REPLACEMENT_OK", "CAPTURED_CONTEXT_LIFETIME_OK",
           "CONSUMED_REPLY_RELEASES_ANSWER_OK", "INTERLEAVED_WORK_GC_OK", "ROOT_METACIRCULAR_LOOKUP_OK", "ALLOCATING_LOOKUP_GC_REFUSED_SAFELY", "TAIL_GC_OK",
           "TRANSFER_ALLOCATION_ACCOUNTING_OK", "REAL_ID_CONTINUATION_OK"]


def digest(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, MISE_TRUSTED_CONFIG_PATHS=str(ROOT), CARGO_TARGET_DIR=str(BUILD),
               BIRNAM_MODULES_DIR="/nonexistent", BIRNAM_TEST_MODULE_DIRECTORY="/nonexistent")
    report = {"status": "incomplete", "platform": platform.platform(), "commands": []}
    (OUT / "receipt.json").write_text(json.dumps(report, indent=2) + "\n")

    def run(name, command, timeout=180):
        result = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True, timeout=timeout)
        output = result.stdout + result.stderr
        (OUT / (name + ".log")).write_text(output)
        report["commands"].append({"name": name, "command": command, "returncode": result.returncode, "output": output})
        assert result.returncode == 0, (name, output[-6000:])
        return output

    run("unit", ["mise", "exec", "--", "cargo", "test", "--offline", "--locked", "-p", "birnam-id", "-p", "birnam-root-registry", "-p", "birnam-collect", "--lib"])
    run("build", ["mise", "exec", "--", "cargo", "build", "--release", "--offline", "--locked", "--lib", "-p", "birnam-id", "-p", "birnam-root-registry", "-p", "birnam-collect"])
    dylib = "dylib" if platform.system() == "Darwin" else "so"
    shared = "-dynamiclib" if dylib == "dylib" else "-shared"
    for name in ["module", "native", "rejected", "rejected-flags"]:
        source = "rejected" if name == "rejected-flags" else name
        flags = ["-DBAD_CONVENTION"] if name == "rejected-flags" else []
        run("compile-" + name, ["clang", shared, "-fPIC", "-O2", *flags, "-I" + str(FIXTURE), str(FIXTURE / (source + ".c")), "-o", str(OUT / (name + "." + dylib))])
    run("compile-caller", ["clang", "-O2", "-I" + str(FIXTURE), str(FIXTURE / "caller.c"), "-o", str(OUT / "caller")])
    libs = [BUILD / "release" / ("libbirnam_" + name + "." + dylib) for name in ["id", "root_registry", "collect"]]
    output = run("caller", [str(OUT / "caller"), *map(str, libs), *[str(OUT / (name + "." + dylib)) for name in ["module", "native", "rejected", "rejected-flags"]]])
    assert all(marker in output for marker in MARKERS), output
    if dylib == "dylib":
        actual = set(subprocess.check_output(["nm", "-gjU", str(libs[0])], text=True).splitlines())
        expected = {"_" + symbol for symbol in json.loads((ROOT / "rust/birnam-id/exports.json").read_text())["exported_symbols"]}
        assert actual == expected, actual
        report["id_exports"] = sorted(actual)
    files = list(FIXTURE.glob("*")) + list((ROOT / "rust/birnam-id/src/id").glob("*.rs"))
    files += [ROOT / "rust/birnam-abi/src/id_protocol.rs", Path(__file__).resolve()]
    report["source_sha256"] = {str(p.relative_to(ROOT)): digest(p) for p in sorted(files)}
    report["library_sha256"] = {str(p): digest(p) for p in libs}
    report["fixture_sha256"] = {str(p): digest(p) for p in OUT.iterdir() if p.name == "caller" or p.suffix == "." + dylib}
    report["status"] = "passed real Id continuation mechanism; compiler/Execution migration and complete runtime qualification remain open"
    (OUT / "receipt.json").write_text(json.dumps(report, indent=2) + "\n")
    print(output, end="")
    print("Receipt:", OUT / "receipt.json")


if __name__ == "__main__":
    main()
