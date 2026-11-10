#!/usr/bin/env python3
"""Authored Id takeover: source and native modules, guarded against siphon fallback."""
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
import compile as driver

BUILD = ROOT / "build"
HOST = [str(BUILD / "siphon"), str(BUILD / "runtime.dylib"), str(BUILD / "seed.dylib")]
# Natively compiled programs run on the runtime services alone, with no interpreter.
NATIVE = [str(BUILD / "siphon"), str(BUILD / "runtime.dylib")]
GUARD = str(BUILD / "id_guard.dylib")
CONTRACT = str(BUILD / "id_contract.dylib")
TESTS = ROOT / "tests/id"
CORPORA = ("tests/seed/semantics.birnam", "tests/seed/collections.birnam", "modules/peg.birnam",
           "tests/peg/semantics.birnam", "tests/peg/replacement.birnam")
EXPECTED = ("PASS authored Id semantics\nPASS seed source semantics\nPASS general collection and control helpers\n"
            "PASS authored PEG semantics\nPASS authored PEG replacement and reentrancy\n")


def run(args, *, kind=None, timeout=300):
    result = subprocess.run(args, cwd="/private/tmp", text=True, capture_output=True, timeout=timeout)
    if kind:
        assert result.returncode == 1 and f": {kind}:" in result.stderr, result
    else:
        assert result.returncode == 0, (result.stdout, result.stderr)
    return result.stdout


def evals(*paths):
    return [part for path in paths for part in ("--eval", str(ROOT / path))]


def native(source, tmp, name):
    c, library = tmp / f"{name}.c", tmp / f"{name}.dylib"
    driver.emit(source, c, name)
    driver.compile_c(c, library)
    return c, library


def check_guarded(output, extra=""):
    assert output.startswith(EXPECTED + extra), output
    assert "PASS authored Id contract: 1000 definitions in one layer;" in output, output
    assert output.endswith("PASS authored Id takeover: 9 replaced siphon Methods poisoned and never entered\n"), output


def main():
    for name in ("id_guard", "id_contract", "compiler_guard", "compiler_contract"):
        driver.compile_c(ROOT / f"tests/{name}.c", BUILD / f"{name}.dylib")

    # Source takeover, loaded twice: reinstalling replaces the authored Methods
    # again and still never selects the siphon's.
    output = run([*HOST, GUARD, *evals("tests/id/before.birnam", "modules/id.birnam", "modules/id.birnam"), GUARD,
                  *evals("tests/id/semantics.birnam", *CORPORA), CONTRACT, GUARD])
    check_guarded(output)
    print(output, end="")
    print(run([*HOST, *evals("tests/id/before.birnam", "modules/id.birnam", "tests/id/captured.birnam")]), end="")

    with tempfile.TemporaryDirectory(prefix="birnam-id-") as directory:
        tmp = Path(directory)
        _, module = native(ROOT / "modules/id.birnam", tmp, "IdModule")
        # The module needs the runtime's Block/Method services.
        run([str(BUILD / "siphon"), str(module)], kind="unknownName")
        # The runtime services are enough: no interpreter is needed to load it.
        run([*NATIVE, str(module)])
        _, before = native(TESTS / "before.birnam", tmp, "IdBefore")
        _, semantics = native(TESTS / "semantics.birnam", tmp, "IdSemantics")
        _, compiled = native(ROOT / "tests/compiler/semantics.birnam", tmp, "NativeProgram")
        # Native Id with the interpreter absent: compiler_guard requires Seed unbound.
        output = run([*NATIVE, str(BUILD / "compiler_guard.dylib"), GUARD, str(before), str(module), GUARD,
                      str(semantics), str(compiled), str(BUILD / "compiler_contract.dylib"), CONTRACT, GUARD])
        assert output.startswith("PASS authored Id semantics\nPASS compiler semantics\n"), output
        assert "no interpreter fallback" in output, output
        assert output.endswith("PASS authored Id takeover: 9 replaced siphon Methods poisoned and never entered\n"), output
        print(output, end="")
        # The authored toolchain produces identical C on the authored Id.
        source = ROOT / "tests/compiler/semantics.birnam"
        with_id, without = tmp / "with-id.c", tmp / "without-id.c"
        driver.emit(source, with_id, "NativeProgram")
        driver.emit(source, without, "NativeProgram", modules=driver.MODULES[1:])
        assert with_id.read_bytes() == without.read_bytes()
    print("PASS authored Id: source/native takeover, identity, guarded execution, identical compiler output")


if __name__ == "__main__":
    main()
