#!/usr/bin/env python3
"""Orchestrate the authored C compiler and the pinned local C toolchain."""
from __future__ import annotations
import argparse
import os
from pathlib import Path
import subprocess
import tempfile
import build

ROOT = Path(__file__).resolve().parents[1]


def quote(text: str) -> str:
    # Only the five escapes supported by Birnam; remaining UTF-8 bytes are
    # literal String content (including control bytes). Never shell-escape code.
    return '"' + text.replace('\\', '\\\\').replace('"', '\\"').replace('\n', '\\n').replace('\r', '\\r').replace('\t', '\\t') + '"'


# The authored Id loads first, so the toolchain runs on its takeover.
MODULES = ("id", "peg", "lang", "lang-syntax", "compiler-ir", "compiler-c")


def emit(source: Path, output: Path, name: str, *, native_dir: Path | None = None,
         witness: Path | None = None, modules: tuple[str, ...] = MODULES) -> None:
    if native_dir is not None:
        # Compiled modules run on the runtime services alone; the interpreter
        # (seed.dylib) is not loaded. An optional witness, loaded before and
        # after the compiler, fails the run if Seed is bound.
        command = [str(ROOT / "build/siphon"), str(ROOT / "build/runtime.dylib")]
        if witness:
            command.append(str(witness.resolve()))
        command.extend(str(native_dir.resolve() / f"{module}.dylib") for module in modules)
        command.append(str(ROOT / "build/compile-request.dylib"))
        if witness:
            command.append(str(witness.resolve()))
        environment = dict(os.environ, BIRNAM_COMPILE_NAME=name)
        result = subprocess.run(command, input=source.read_bytes(), env=environment,
                                cwd="/private/tmp", capture_output=True, timeout=600)
        if result.returncode:
            raise RuntimeError(result.stderr.decode(errors="replace").strip())
        output.write_bytes(result.stdout)
        return
    # The wrapper passes source as data. Parsing, IR lowering, and C emission
    # belong to the authored modules; this driver does none of those operations.
    text = source.read_bytes().decode("utf-8")
    wrapper = ('let generated := [Compiler compile: ' + quote(text) + ' as: ' + quote(name) + ']; '
               '[[generated isError] ifTrue: [| generated] ifFalse: [| [System print: generated]]];')
    with tempfile.TemporaryDirectory(prefix="birnam-compile-") as tmp:
        request = Path(tmp) / "compile.birnam"
        request.write_text(wrapper)
        command = [str(ROOT / "build/siphon"), str(ROOT / "build/runtime.dylib"), str(ROOT / "build/seed.dylib")]
        for module in modules:
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
    parser.add_argument("--native-dir", type=Path, help="use compiled PEG/Lang/compiler modules from this directory")
    parser.add_argument("--emit-c", action="store_true", help="emit C without invoking Clang")
    args = parser.parse_args()
    output = args.output.resolve()
    c_path = output if args.emit_c else output.with_suffix(".c")
    emit(args.source.resolve(), c_path, args.name, native_dir=args.native_dir)
    if not args.emit_c:
        compile_c(c_path, output)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, ValueError, OSError, subprocess.SubprocessError) as error:
        raise SystemExit(f"compile: {error}")
