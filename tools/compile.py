#!/usr/bin/env python3
"""Orchestrate the authored C compiler and the pinned local C toolchain."""
import argparse
from pathlib import Path
import subprocess
import tempfile
import build

ROOT = Path(__file__).resolve().parents[1]


def quote(text: str) -> str:
    # Only the five escapes supported by Birnam; remaining UTF-8 bytes are
    # literal String content (including control bytes). Never shell-escape code.
    return '"' + text.replace('\\', '\\\\').replace('"', '\\"').replace('\n', '\\n').replace('\r', '\\r').replace('\t', '\\t') + '"'


def emit(source: Path, output: Path, name: str) -> None:
    # The wrapper passes source as data. Parsing, IR lowering, and C emission
    # belong to the authored modules; this driver does none of those operations.
    text = source.read_bytes().decode("utf-8")
    wrapper = ('let generated := [Compiler compile: ' + quote(text) + ' as: ' + quote(name) + ']; '
               '[[generated isError] ifTrue: [| generated] ifFalse: [| [System print: generated]]];')
    with tempfile.TemporaryDirectory(prefix="birnam-compile-") as tmp:
        request = Path(tmp) / "compile.birnam"
        request.write_text(wrapper)
        command = [str(ROOT / "build/siphon"), str(ROOT / "build/seed.dylib")]
        for module in ("peg", "lang", "lang-syntax", "compiler-ir", "compiler-c"):
            command.extend(["--eval", str(ROOT / f"modules/{module}.birnam")])
        command.extend(["--eval", str(ROOT / "examples/lang/takeover.birnam"), "--eval", str(request)])
        result = subprocess.run(command, cwd=tmp, capture_output=True, timeout=300)
        if result.returncode:
            raise RuntimeError(result.stderr.decode(errors="replace").strip())
        output.write_bytes(result.stdout)


def compile_c(source: Path, output: Path) -> None:
    cflags, ldflags, _ = build.configuration()
    build.run("clang", "-std=c11", "-Wall", "-Wextra", "-Werror", "-Wno-unused-parameter",
              "-O2", "-g", "-I" + str(ROOT / "include"), *cflags,
              "-dynamiclib", "-undefined", "dynamic_lookup", str(source), *ldflags, "-o", str(output))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("-o", "--output", required=True, type=Path, help="output C file or dylib")
    parser.add_argument("--name", default="NativeProgram", help="namespace export for the compiled program")
    parser.add_argument("--emit-c", action="store_true", help="emit C without invoking Clang")
    args = parser.parse_args()
    output = args.output.resolve()
    c_path = output if args.emit_c else output.with_suffix(".c")
    emit(args.source.resolve(), c_path, args.name)
    if not args.emit_c:
        compile_c(c_path, output)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, UnicodeDecodeError, OSError, subprocess.SubprocessError) as error:
        raise SystemExit(f"compile: {error}")
