#!/usr/bin/env python3
"""Authored PEG acceptance through explicit source loading in the real host."""
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
PREFIX = [str(ROOT / "build/siphon"), str(ROOT / "build/seed.dylib"),
          "--eval", str(ROOT / "modules/peg.birnam")]


def main():
    cases = [("examples/peg/word.birnam", "#root\n0\n3\n#word\n"),
             ("tests/peg/semantics.birnam", "PASS authored PEG semantics\n"),
             ("tests/peg/replacement.birnam", "PASS authored PEG replacement and reentrancy\n")]
    with tempfile.TemporaryDirectory(prefix="birnam-peg-") as tmp:
        for path, expected in cases:
            result = subprocess.run([*PREFIX, "--eval", str(ROOT / path)], cwd=tmp,
                                    text=True, capture_output=True, timeout=60)
            assert result.returncode == 0, (path, result.stdout, result.stderr)
            assert result.stdout == expected, (path, result.stdout, expected)
        failure = Path(tmp) / "failure.birnam"
        failure.write_text('let g := [Peg grammar]; [g rule: #root is: [Peg literal: "abc"]]; '
                           '[g parse: "abd" from: #root]; [System println: "must not run"];')
        result = subprocess.run([*PREFIX, "--eval", str(failure)], cwd=tmp,
                                text=True, capture_output=True, timeout=60)
        assert result.returncode == 1 and not result.stdout, result
        assert "syntaxError: syntax error at line 1, col 3" in result.stderr, result
    print("PASS PEG CLI: sample, semantics, replacement/reentrancy, reported mismatch; unrelated working directory")


if __name__ == "__main__":
    main()
