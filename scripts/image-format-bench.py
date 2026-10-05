#!/usr/bin/env python3
"""Measure authored ImageFormat on a primitive prefix, with a strict deadline.

Default: complete admission. --range START COUNT: only the first heap-validation
pass over that object range; this diagnostic does not prove complete admission.
Compilation is explicit and excluded from the reported process/admission times.
"""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import selectors
import subprocess
import tempfile
import time


def instrument(source, object_range):
    source = source.replace("ImageFormat", "ImageFormatBench").replace("Module named: #imageFormat ", "Module named: #imageFormatBench ")
    start = source.index("[Reflect defineMethod: #validateHeaps:")
    end = source.index("[Reflect defineMethod: #unsignedGreater:", start)
    method = source[start:end]
    if object_range:
        first, count = object_range
        if "firstLocatorOutside:" in method:
            method = method[:method.index('    [self checked: [|\n      [[behaviors')]
            method += '    [System println: "bench:end"]; #[count [codes value]]\n  ]]\n]];\n'
            method = method.replace('let all := [objects value]; let count := [all size];',
                f'let whole := [objects value]; let all := [whole copyFrom: {first} to: {first+count}]; let count := [whole size];')
        else:
            method = method[:method.index("  [[behaviors notNil]")]
            method += '  [System println: "bench:end"]; #[count [codes value]]\n  ]]\n]];\n'
            method = method.replace("[self each: all do: [:object :i |",
                f"[self each: [all copyFrom: {first} to: {first + count}] do: [:object :i |")
    else:
        method = method.replace("#[count [codes value]]", '[System println: "bench:end"]; #[count [codes value]]')
    method = re.sub(r'\blet all :=', '[System println: "bench:begin"]; let all :=', method, count=1)
    source = source[:start] + method + source[end:]
    if object_range:
        start = source.index("  let heapInfo :=")
        end = source.index("[Reflect defineMethod: #composeFields:", start)
        method = source[start:end]
        method = method[:method.index("  [[[sources value]")]
        source = source[:start] + method + "  heapInfo\n  ]]\n]];\n" + source[end:]
    return source


def run(binary, script, env, timeout):
    started = time.monotonic()
    process = subprocess.Popen([binary, script], cwd="/tmp", env=env,
                               stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ)
    output = bytearray()
    marks = {}
    try:
        while process.poll() is None and time.monotonic() - started < timeout:
            for key, _ in selector.select(0.1):
                chunk = os.read(key.fd, 65536)
                output.extend(chunk)
                for marker in ("begin", "end"):
                    if marker not in marks and f"bench:{marker}\n".encode() in output:
                        marks[marker] = time.monotonic() - started
        if process.poll() is None:
            raise TimeoutError(f"exceeded {timeout}s; output: {output.decode(errors='replace')}")
        output.extend(process.stdout.read())
        if process.returncode or b"bench:ok\n" not in output:
            raise RuntimeError((process.returncode, output.decode(errors="replace")))
        return marks["end"] - marks["begin"], time.monotonic() - started
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        selector.close()
        process.stdout.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("prefix", type=Path)
    parser.add_argument("image", type=Path)
    parser.add_argument("--source", type=Path, default=Path(__file__).resolve().parent.parent / "modules/image-format.birnam")
    parser.add_argument("--label", default="current")
    parser.add_argument("--range", type=int, nargs=2, metavar=("START", "COUNT"))
    parser.add_argument("--timeout", type=float, default=120)
    parser.add_argument("--runs", type=int, default=1)
    args = parser.parse_args()
    if args.runs < 1 or args.timeout <= 0 or (args.range and (args.range[0] < 0 or args.range[1] < 1)):
        parser.error("runs, timeout, and range must be positive (START may be zero)")
    binary = args.prefix.resolve() / "bin/birnam"
    original = args.source.read_text()
    env = {k: v for k, v in os.environ.items() if not k.startswith("BIRNAM_")}
    env["BIRNAM_NO_STDLIB"] = "1"
    with tempfile.TemporaryDirectory(prefix="birnam-image-format-bench-") as temporary:
        work = Path(temporary)
        source, artifact = work / "format.birnam", work / "format.dylib"
        source.write_text(instrument(original, args.range))
        compile_script = work / "compile.birnam"
        compile_script.write_text(
            f'let source := [System readFile: {json.dumps(str(source))}];\n'
            f'let answer := [Codegen compile: source output: {json.dumps(str(artifact))} from: {json.dumps(str(source))} synchronousBodies: true];\n'
            '[[answer isError] ifTrue: [| [System println: answer]; [System exit: 1]]];'
        )
        result = subprocess.run([binary, compile_script], cwd=work, env=env,
                                capture_output=True, timeout=args.timeout)
        if result.returncode or not artifact.exists():
            raise RuntimeError((result.returncode, result.stdout, result.stderr))
        script = work / "run.birnam"
        script.write_text(
            f'[Codegen load: {json.dumps(str(artifact))}];\n'
            f'let bytes := [[System readFile: {json.dumps(str(args.image.resolve()))}] asBytes];\n'
            'let answer := [ImageFormatBench inspect: bytes architecture: 1 abi: [bytes readU64At: 20]];\n'
            '[[answer isError] ifTrue: [| [System println: answer]; [System exit: 1]]];\n'
            '[System println: "bench:ok"];'
        )
        samples = [run(binary, script, env, args.timeout) for _ in range(args.runs)]
    print(json.dumps({
        "label": args.label,
        "scope": "first heap pass only" if args.range else "complete admission",
        "range": args.range,
        "source_sha256": hashlib.sha256(original.encode()).hexdigest(),
        "prefix": str(args.prefix.resolve()),
        "image": str(args.image.resolve()),
        "validation_seconds": [v for v, _ in samples],
        "process_seconds": [v for _, v in samples],
    }))


if __name__ == "__main__":
    main()
