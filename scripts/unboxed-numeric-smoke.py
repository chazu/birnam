#!/usr/bin/env python3
"""Check installed numeric source, actual box savings and every recovery PC."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def source(shape, operations, precise, captured=False):
    if shape == "equal":
        body = "let first := [a + b]; let second := [a + b]; [first + second]"
        expected = 28
    elif shape == "continue":
        body = "let answer := [a + b]; " + "let answer := answer; " * 110 + "[answer + b]"
        expected = 17
    else:
        statements = []
        for index in range(operations):
            send = f"[{'a' if index == 0 else 'answer'} + b]"
            statements.append(send if shape == "tail" and index + 1 == operations
                              else f"let answer := {send};")
        if shape == "return":
            statements.append("answer")
        body = " ".join(statements)
        expected = 11 + operations * 3
    if captured:
        body = body.replace(" + b", " + constant")
    captures = "#[constant]" if captured else "#[]"
    return (("let constant := 3;\n" if captured else "") + f"[System preciseGc: {'true' if precise else 'false'}];\n"
            f"let source := [:a :b | {body}];\n"
            f"let block := [Codegen materializeResumable: [[source codeObject] withoutSourceProvenance] captures: {captures}];\n"
            f"[UnboxedNumericProbe run: block input: 11 other: 3 expected: {expected}];\n")


def check(values, baseline, shape, operations, enabled, force):
    assert values["invocations"] == 8
    assert values["bytes"] == baseline > 0
    tail = shape != "return"
    completed = 0 if not enabled else (operations if force < 0 else
                                      int(force > 0) if shape == "continue" else force)
    materialized = (0 if not enabled or force == 0 or tail and force < 0 else
                    2 if shape == "equal" and force == 2 else 1)
    saved = completed - materialized - int(enabled and tail and force < 0)
    assert values["completed"] == 8 * completed, values
    assert values["materialized"] == 8 * materialized, values
    assert values["physical_saved"] == 8 * saved, values


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, required=True)
    parser.add_argument("--floor", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--quick", action="store_true")
    parser.add_argument("--captured-inputs", action="store_true")
    args = parser.parse_args()
    prefix, floor = args.prefix.resolve(), args.floor.resolve()
    env = {k: v for k, v in os.environ.items() if not k.startswith(("BIRNAM_", "DYLD_"))}
    env.update(BIRNAM_TEST_UNBOXED_NUMERIC="1")
    installed = [prefix / "bin/birnam", prefix / "share/birnam/rust-install.json"]
    installed += sorted((prefix / "lib").glob("*.dylib"))
    sources = [Path(__file__).resolve(), ROOT / "rust/birnam-id/src/public_api.rs",
               ROOT / "rust/birnam-id/src/runtime.rs", ROOT / "rust/birnam-id/src/runtime/unboxed_numeric_probe.rs",
               ROOT / "rust/birnam-id/src/runtime/native_tier.rs",
               ROOT / "rust/birnam-id/src/runtime/native_tier/unboxed_numeric.rs",
               ROOT / "rust/birnam-id/src/heap/dead_allocations.rs"]
    sources += sorted((ROOT / "rust/birnam-codegen/src/numeric_chain").rglob("*.rs"))
    receipt = dict(status="running", captured_inputs=args.captured_inputs, scope="Installed Prelude and Codegen with a test-only floor; no timing claim",
                   floor_sha256=sha(floor), installed_sha256={str(p): sha(p) for p in installed},
                   source_sha256={str(p.relative_to(ROOT)): sha(p) for p in sources}, rows=[])
    try:
        with tempfile.TemporaryDirectory(prefix="birnam-unboxed-smoke-") as work:
            fixture = Path(work) / "numeric.birnam"
            fixture.write_text('[System println: [Globals includes: #UnboxedNumericProbe]];\n')
            absent = subprocess.run([str(prefix / "bin/birnam"), str(fixture)], env=env,
                                    capture_output=True, text=True, cwd=ROOT)
            assert absent.returncode == 0 and absent.stdout.strip() == "false", (absent.stdout, absent.stderr)
            receipt["production_probe_absent"] = True
            env["BIRNAM_ID_DYLIB"] = str(floor)
            shapes = [("tail", 3), ("return", 3), ("equal", 3), ("continue", 2)]
            if not args.quick:
                shapes += [("tail", 16), ("tail", 32)]
            for precise in ([1] if args.quick else [0, 1]):
                for shape, operations in shapes:
                    code = source(shape, operations, precise, args.captured_inputs)
                    fixture.write_text(code)
                    baseline = None
                    pcs = 3 if shape == "continue" else operations + int(shape == "return")
                    for enabled, force in [(False, -1), (True, -1)] + [(True, n) for n in range(pcs)]:
                        env.update(BIRNAM_DEV_UNBOXED_NUMERIC=str(int(enabled)),
                                   BIRNAM_DEV_UNBOXED_NUMERIC_FORCE_AT=str(force))
                        process = subprocess.run([str(prefix / "bin/birnam"), str(fixture)],
                                                 env=env, text=True, capture_output=True, cwd=ROOT)
                        assert process.returncode == 0, (process.returncode, precise, shape, operations, enabled, force,
                                                         process.stdout, process.stderr)
                        lines = [line for line in process.stdout.splitlines() if line.startswith("unboxed-numeric ")]
                        assert len(lines) == 1, process.stdout
                        values = {k: int(v) for k, v in re.findall(r"(\w+)=(-?\d+)", lines[0])}
                        row = dict(precise=precise, shape=shape, operations=operations, enabled=enabled,
                                   force=force, values=values, source_sha256=hashlib.sha256(code.encode()).hexdigest())
                        receipt["rows"].append(row)
                        if baseline is None:
                            baseline = values["bytes"]
                        check(values, baseline, shape, operations, enabled, force)
                    print(f"unboxed-numeric-smoke: precise={precise} shape={shape} operations={operations} baseline/enabled/forced passed", flush=True)
        assert sha(floor) == receipt["floor_sha256"]
        assert all(sha(p) == digest for p, digest in receipt["installed_sha256"].items())
        assert all(sha(ROOT / p) == digest for p, digest in receipt["source_sha256"].items())
        receipt["status"] = "passed"
    except BaseException as error:
        receipt.update(status="failed", error=str(error))
        raise
    finally:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
