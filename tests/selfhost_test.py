#!/usr/bin/env python3
"""Qualify the native compiler generations, generated programs, and failures."""
from __future__ import annotations
import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
import build
import compile as driver
import selfhost


def validate_receipt(directory: Path) -> dict:
    report = json.loads((directory / "receipt.json").read_text())
    assert report["status"] == "passed", report
    assert report["toolchain"] == build.configuration()[2], "toolchain changed"
    for key, name in (("siphon_sha256", "siphon"), ("seed_sha256", "seed.dylib"),
                      ("request_adapter_sha256", "compile-request.dylib")):
        assert report[key] == selfhost.digest(ROOT / "build" / name), f"changed bootstrap artifact: {name}"
    for module in driver.MODULES:
        assert report["modules"][module]["source_sha256"] == selfhost.digest(ROOT / f"modules/{module}.birnam"), module
        for stage in ("stage1", "stage2"):
            for suffix, key in (("c", "c_sha256"), ("dylib", "library_sha256")):
                assert report["stages"][stage][module][key] == selfhost.digest(directory / stage / f"{module}.{suffix}"), (stage, module, key)
        generated = [(directory / stage / f"{module}.c").read_bytes() for stage in ("stage1", "stage2", "stage3-c")]
        assert generated[0] == generated[1] == generated[2], module
    return report


def qualify(directory: Path) -> None:
    report = validate_receipt(directory)
    report["acceptance"] = {"status": "running"}
    (directory / "receipt.json").write_text(json.dumps(report, indent=2) + "\n")
    native = directory / "stage2"
    guard = directory / "test-guard.dylib"
    driver.compile_c(ROOT / "tests/selfhost_guard.c", guard)
    base = [str(ROOT / "build/siphon"), str(ROOT / "build/seed.dylib"), str(guard)]
    modules = [str(native / f"{module}.dylib") for module in driver.MODULES]
    tests = (("compiler/semantics", "PASS compiler semantics\n"),
             ("peg/semantics", "PASS authored PEG semantics\n"),
             ("peg/replacement", "PASS authored PEG replacement and reentrancy\n"),
             ("lang/grammar", "PASS Lang grammar and CST spans\n"),
             ("compiler/emission", "PASS replaceable compiler emission and unsupported factories\n"))
    for name, expected in tests:
        stem = name.replace("/", "-")
        c, library = directory / f"test-{stem}.c", directory / f"test-{stem}.dylib"
        driver.emit(ROOT / f"tests/{name}.birnam", c, "SelfhostAcceptance", native_dir=native, guard=guard)
        driver.compile_c(c, library)
        result = subprocess.run([*base, *modules, str(library), str(guard)], cwd="/private/tmp",
                                text=True, capture_output=True, timeout=180)
        assert result.returncode == 0 and result.stdout == expected, (name, result.stdout, result.stderr)
        print(expected, end="", flush=True)
    with tempfile.TemporaryDirectory(prefix="birnam-selfhost-errors-") as tmp:
        path = Path(tmp) / "input.birnam"
        for i, (source, kind) in enumerate(((b'1.5', 'unsupportedSyntax'), (b'[x]', 'syntaxError'),
                                           (b'9223372036854775808', 'integerRange'),
                                           (b'[:x :x | x]', 'duplicateBinding'), (b'"\xc3"', 'invalidUtf8'))):
            path.write_bytes(source)
            target = Path(tmp) / f"failure{i}.c"
            try:
                driver.emit(path, target, "Failure", native_dir=native, guard=guard)
            except RuntimeError as error:
                assert f": {kind}:" in str(error), str(error)
            else:
                raise AssertionError(f"native compiler accepted {source!r}")
            assert not target.exists()
        path.write_text("42")
        try:
            driver.emit(path, Path(tmp) / "missing.c", "Missing", native_dir=Path(tmp) / "absent")
        except RuntimeError as error:
            assert ": moduleOpenFailed:" in str(error), str(error)
        else:
            raise AssertionError("missing native modules silently fell back")
    # Exercise the public CLI, not just its Python API.
    result = subprocess.run([sys.executable, str(ROOT / "tools/compile.py"),
                             str(ROOT / "examples/compiler/answer.birnam"),
                             "--native-dir", str(native), "--emit-c", "--name", "NativeProgram",
                             "-o", str(directory / "cli-answer.c")], cwd="/private/tmp",
                            text=True, capture_output=True, timeout=180)
    assert result.returncode == 0, result.stderr
    assert (directory / "cli-answer.c").read_bytes() == (directory / "answer.c").read_bytes()
    report["acceptance"] = {"status": "passed", "native_corpora": [name for name, _ in tests],
                            "compile_failures": 5, "missing_module_fallback": False,
                            "native_cli": True, "guard_sha256": selfhost.digest(guard)}
    (directory / "receipt.json").write_text(json.dumps(report, indent=2) + "\n")
    print("PASS self-hosted compiler acceptance: native PEG/Lang/compiler, guarded execution, failures, CLI", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, default=ROOT / "build/selfhost")
    parser.add_argument("--existing", action="store_true", help="validate and test a completed, matching build receipt")
    args = parser.parse_args()
    directory = args.directory.resolve()
    if not args.existing:
        selfhost.build_pipeline(directory)
    qualify(directory)


if __name__ == "__main__":
    main()
