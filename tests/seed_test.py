#!/usr/bin/env python3
"""End-to-end source tests through the separately loaded Odin seed module."""
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SIPHON = ROOT / "build/siphon"
SEED = ROOT / "build/seed.dylib"


def run_source(source, expected=None, kind=None):
    with tempfile.TemporaryDirectory(prefix="birnam-source-") as tmp:
        path = Path(tmp) / "test.birnam"
        path.write_bytes(source if isinstance(source, bytes) else source.encode())
        result = subprocess.run([str(SIPHON), str(SEED), "--eval", str(path)],
                                cwd=tmp, text=True, capture_output=True, timeout=20)
        if kind:
            assert result.returncode == 1 and f": {kind}:" in result.stderr, (source[:160], result)
        else:
            assert result.returncode == 0, (source[:160], result.stdout, result.stderr)
        if expected is not None:
            assert result.stdout == expected, (source[:160], result.stdout, expected)
        return result


def main():
    run_source((ROOT / "examples/seed/answer.birnam").read_text(), "42\n")
    run_source((ROOT / "tests/seed/semantics.birnam").read_text(), "PASS seed source semantics\n")
    run_source((ROOT / "tests/seed/collections.birnam").read_text(), "PASS general collection and control helpers\n")
    cases = [
        ("[x]", "syntaxError"), ("[[1 + 2]]", "syntaxError"),
        ("let := 3", "syntaxError"), ("[1 + 2", "syntaxError"),
        ('"unfinished', "syntaxError"), ('"bad\\q"', "syntaxError"),
        ("9223372036854775808", "integerRange"),
        ("-9223372036854775809", "integerRange"),
        ("1.5", "unsupportedSyntax"), ("{#a -> 1}", "unsupportedSyntax"),
        ("0x10", "unsupportedSyntax"), ("1e3", "unsupportedSyntax"),
        ("?variable", "unsupportedSyntax"), ("[1 => printString]", "unsupportedSyntax"),
        ("missing", "unknownName"), ("[1 missing]", "messageNotUnderstood"),
        ("[1 / 0]", "zeroDivide"), ("[1 % 0]", "zeroDivide"),
        ("[9223372036854775807 + 1]", "integerOverflow"),
        ("[[:x | x] value]", "arityMismatch"),
        ("[:x :x | x]", "duplicateBinding"),
        ("let x := 1; let x := 2", "duplicateBinding"),
        ("[[| let x := 1; let x := 2] value]", "duplicateBinding"),
        ("[1 + true]", "typeMismatch"), ("[#[1] at: -1]", "indexOutOfBounds"),
        (b'"\xc3"', "invalidUtf8"),
    ]
    for source, kind in cases:
        result = run_source(source, kind=kind)
        if kind in {"syntaxError", "unsupportedSyntax", "integerRange", "invalidUtf8"}:
            assert "byte " in result.stderr and "line " in result.stderr, result.stderr
    run_source('[System println: "before"]; [1 / 0]; [System println: "after"]', "before\n", "zeroDivide")
    run_source('[System println: "must not run"]; [', "", "syntaxError")
    run_source('[System println: [1 + 2]]\n[System println: [[| 4; 5] value]]', "3\n5\n")
    run_source('[System println: [[|] value]]', "nil\n")
    run_source('[System println: [[1 / 0] ifError: [:e | 9]]]', "9\n")
    run_source('[System println: [[[| [1 / 0]; missing] value] ifError: [:e | 1]]]', '1\n')
    with tempfile.TemporaryDirectory(prefix="birnam-multi-source-") as tmp:
        first, second = Path(tmp)/"first.birnam", Path(tmp)/"second.birnam"
        first.write_text('let shared := 41; let reader := [| shared];')
        second.write_text('[Namespace at: #shared put: 42]; [GC collect]; [System println: [reader value]];')
        result = subprocess.run([str(SIPHON), str(SEED), "--eval", str(first), "--eval", str(second)],
                                cwd=tmp, text=True, capture_output=True, timeout=20)
        assert result.returncode == 0 and result.stdout == "42\n", result
        result = subprocess.run([str(SIPHON), str(SEED), "--eval", str(Path(tmp)/"absent.birnam")],
                                cwd=tmp, text=True, capture_output=True, timeout=20)
        assert result.returncode == 1 and "sourceOpenFailed" in result.stderr, result
        result = subprocess.run([str(SIPHON), "--eval", str(first)], text=True, capture_output=True, timeout=20)
        assert result.returncode == 1 and "unknownName" in result.stderr, result
        result = subprocess.run([str(SIPHON), str(SEED), "--eval"], text=True, capture_output=True, timeout=20)
        assert result.returncode == 2 and "requires a source path" in result.stderr, result
    print(f"PASS seed CLI: sample, semantic corpus, {len(cases)} failure cases, ordering and diagnostics")


if __name__ == "__main__":
    main()
