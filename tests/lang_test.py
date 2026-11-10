#!/usr/bin/env python3
"""Authored translation/takeover, with the native parser entry disabled."""
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
PREFIX = [str(ROOT / "build/siphon"), str(ROOT / "build/runtime.dylib"), str(ROOT / "build/seed.dylib")]
for name in ("peg", "lang", "lang-syntax"):
    PREFIX.extend(["--eval", str(ROOT / f"modules/{name}.birnam")])
PREFIX.append(str(ROOT / "build/lang_guard.dylib"))


def run(*args, timeout=240):
    result = subprocess.run([*PREFIX, *args], cwd="/private/tmp",
                            text=True, capture_output=True, timeout=timeout)
    assert result.returncode == 0, (args, result.stdout, result.stderr)
    return result.stdout


def main():
    assert run("--eval", str(ROOT / "tests/lang/takeover.birnam")) == "PASS authored Lang takeover and replacement\n"
    with tempfile.TemporaryDirectory(prefix="birnam-lang-utf8-") as tmp:
        source = Path(tmp) / "invalid.birnam"
        source.write_bytes(b'// \xc3\xa9\n"\xc3"')
        result = subprocess.run([*PREFIX, "--eval", str(source)], text=True,
                                capture_output=True, timeout=60)
        assert result.returncode == 1, result
        assert "invalidUtf8:" in result.stderr and "at byte 7 (line 2, byte column 2)" in result.stderr, result
    # Native entry is already disabled when every one of these is re-read.
    args = []
    for name in ("modules/peg.birnam", "modules/lang.birnam", "modules/lang-syntax.birnam",
                 "examples/lang/takeover.birnam", "tests/seed/semantics.birnam",
                 "tests/peg/semantics.birnam", "tests/peg/replacement.birnam"):
        args.extend(["--eval", str(ROOT / name)])
    args.append(str(ROOT / "build/lang_contract.dylib"))
    output = run(*args)
    assert "PASS seed source semantics\n" in output, output
    assert "PASS authored PEG semantics\n" in output, output
    assert "PASS authored PEG replacement and reentrancy\n" in output, output
    assert "PASS Lang GC:" in output and "no native parser calls" in output, output
    print(output, end="")
    print("PASS Lang takeover: PEG/Lang/translator reloaded and reinstalled with original native parser disabled")


if __name__ == "__main__":
    main()
