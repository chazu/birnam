#!/usr/bin/env python3
"""Native C acceptance, compared with direct and authored-IR execution."""
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
import compile as driver

HOST = [str(ROOT / "build/siphon"), str(ROOT / "build/seed.dylib")]
AUTHORED = HOST.copy()
for module in ("peg", "lang", "lang-syntax", "compiler-ir", "compiler-c"):
    AUTHORED.extend(["--eval", str(ROOT / f"modules/{module}.birnam")])
GUARD = str(ROOT / "build/compiler_guard.dylib")
PROBE = str(ROOT / "build/compiler_probe.dylib")
CHECKED = str(ROOT / "build/compiler_checked.dylib")


def run(args, *, expected=None, kind=None):
    result = subprocess.run(args, cwd="/private/tmp", text=True, capture_output=True, timeout=300)
    if kind:
        assert result.returncode == 1 and f": {kind}:" in result.stderr, result
    else:
        assert result.returncode == 0, (result.stdout, result.stderr)
    if expected is not None:
        assert result.stdout == expected, (result.stdout, expected)
    return result.stdout


def compile_source(source, tmp, stem):
    path = tmp / f"{stem}.birnam"
    path.write_text(source)
    c = tmp / f"{stem}.c"
    lib = tmp / f"{stem}.dylib"
    driver.emit(path, c, "NativeProgram")
    driver.compile_c(c, lib)
    return lib


def main():
    run([*AUTHORED, "--eval", str(ROOT / "tests/compiler/ir.birnam")], expected="PASS authored compiler IR\n")
    run([*AUTHORED, "--eval", str(ROOT / "tests/compiler/emission.birnam")], expected="PASS replaceable compiler emission and unsupported factories\n")
    with tempfile.TemporaryDirectory(prefix="birnam-compiler-") as directory:
        tmp = Path(directory)
        source = (ROOT / "tests/compiler/semantics.birnam").read_text()
        run([*HOST, "--eval", str(ROOT / "tests/compiler/semantics.birnam")], expected="PASS compiler semantics\n")
        ir = tmp / "ir.birnam"
        ir.write_text('[Compiler evaluate: ' + driver.quote(source) + ' in: Namespace];')
        run([*AUTHORED, "--eval", str(ir)], expected="PASS compiler semantics\n")
        native = compile_source(source, tmp, "semantics")
        output = run([*HOST, GUARD, str(native), str(ROOT / "build/compiler_contract.dylib")])
        assert output.count("PASS compiler semantics\n") == 2, output
        assert "no interpreter fallback" in output, output
        print(output, end="")

        probe_source = (ROOT / "tests/compiler/dispatch.birnam").read_text()
        run([*HOST, PROBE, "--eval", str(ROOT / "tests/compiler/dispatch.birnam"), CHECKED], expected="PASS effectful lookup and direct selected entry\n")
        ir.write_text('[Compiler evaluate: ' + driver.quote(probe_source) + ' in: Namespace];')
        run([*AUTHORED, PROBE, "--eval", str(ir), CHECKED], expected="PASS effectful lookup and direct selected entry\n")
        native = compile_source(probe_source, tmp, "dispatch")
        run([*HOST, GUARD, PROBE, str(native), CHECKED], expected="PASS effectful lookup and direct selected entry\n")

        cases = [
            ('[System println: "before"]; [1 / 0]; [System println: "after"]', "before\n", "zeroDivide"),
            ('', '', None),
            ('[System println: "é\x00\\\"\\\\"]; [System println: #foo:bar:]', 'é\x00"\\\n#foo:bar:\n', None),
        ]
        for i, (text, expected, kind) in enumerate(cases):
            native = compile_source(text, tmp, f"case{i}")
            run([*HOST, GUARD, str(native)], expected=expected, kind=kind)
        for i, (text, kind) in enumerate((('1.5', 'unsupportedSyntax'), ('[x]', 'syntaxError'),
                                         ('9223372036854775808', 'integerRange'), ('[:x :x | x]', 'duplicateBinding'))):
            path, target = tmp / "invalid.birnam", tmp / f"invalid{i}.c"
            path.write_text(text)
            try:
                driver.emit(path, target, "NativeProgram")
            except RuntimeError as error:
                assert f": {kind}:" in str(error), str(error)
            else:
                raise AssertionError(f"compiler accepted {text}")
            assert not target.exists()
    print("PASS authored compiler: direct/IR/native witnesses, effects, byte escaping, native failures, rejected source")


if __name__ == "__main__":
    main()
