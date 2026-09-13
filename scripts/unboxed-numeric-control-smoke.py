#!/usr/bin/env python3
"""Observe recovered numeric Frames, then collect/evict/capture/suspend/resume."""
import argparse
import ast
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


def source(shape, operations, precise, pc, capture):
    if shape == "equal":
        body = "let first := [a + b]; let second := [a + b]; let last := [first + second]; last"
        expected = 28
    elif shape == "continue":
        body = "let answer := [a + b]; " + "let answer := answer; " * 110 + "let last := [answer + b]; last"
        expected = 17
    else:
        body = " ".join(f"let answer := [{'a' if n == 0 else 'answer'} + b];" for n in range(operations)) + " answer"
        expected = 11 + operations * 3
    return (f"[System preciseGc: {'true' if precise else 'false'}];\n"
            f"let source := [:a :b | {body}];\n"
            "let block := [Codegen materializeResumable: [[source codeObject] withoutSourceProvenance] captures: #[]];\n"
            f"[UnboxedNumericProbe control: block input: 11 other: 3 expected: {expected} at: {pc} capture: {'true' if capture else 'false'}];\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, required=True)
    parser.add_argument("--floor", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--quick", action="store_true")
    args = parser.parse_args()
    prefix, floor = args.prefix.resolve(), args.floor.resolve()
    env = {k: v for k, v in os.environ.items() if not k.startswith(("BIRNAM_", "DYLD_"))}
    env.update(BIRNAM_TEST_UNBOXED_NUMERIC="1", BIRNAM_ID_DYLIB=str(floor))
    paths = [Path(__file__).resolve(), ROOT / "rust/birnam-id/src/runtime.rs",
             ROOT / "rust/birnam-id/src/runtime/unboxed_numeric_probe.rs",
             ROOT / "rust/birnam-id/src/runtime/unboxed_numeric_probe/control.rs",
             ROOT / "rust/birnam-id/src/runtime/native_tier/unboxed_numeric.rs",
             ROOT / "rust/birnam-id/src/runtime/native_tier/fused_numeric.rs",
             ROOT / "rust/birnam-id/src/execution.rs",
             ROOT / "rust/birnam-id/src/public_api.rs"]
    paths += sorted((ROOT / "rust/birnam-codegen/src/numeric_chain").rglob("*.rs"))
    paths += sorted((ROOT / "rust/birnam-id/src/runtime/native_tier/unboxed_numeric").rglob("*.rs"))
    installed = [prefix / "bin/birnam", prefix / "share/birnam/rust-install.json"] + sorted((prefix / "lib").glob("*.dylib"))
    receipt = dict(status="running", scope="Forced source recovery followed by test-floor V2 control; no timing claim",
                   floor_sha256=sha(floor), source_sha256={str(p.relative_to(ROOT)): sha(p) for p in paths},
                   installed_sha256={str(p): sha(p) for p in installed}, rows=[])
    try:
        with tempfile.TemporaryDirectory(prefix="birnam-numeric-control-") as work:
            fixture = Path(work) / "control.birnam"
            shapes = [("chain", 3), ("equal", 3), ("continue", 2)]
            if not args.quick:
                shapes += [("chain", 16), ("chain", 32)]
            for precise in ([1] if args.quick else [0, 1]):
                for shape, operations in shapes:
                    points = [0, 2] if shape == "continue" else list(range(operations))
                    if args.quick:
                        points = [points[-1]]
                    for pc in points:
                        for capture in [False, True]:
                            code = source(shape, operations, precise, pc, capture)
                            fixture.write_text(code)
                            baseline = None
                            for enabled in [False, True]:
                                env.update(BIRNAM_DEV_UNBOXED_NUMERIC=str(int(enabled)), BIRNAM_DEV_UNBOXED_NUMERIC_FORCE_AT=str(pc))
                                process = subprocess.run([str(prefix / "bin/birnam"), str(fixture)], env=env,
                                                         cwd=ROOT, text=True, capture_output=True)
                                assert process.returncode == 0, (process.returncode, shape, pc, capture, enabled, process.stdout, process.stderr)
                                lines = [s for s in process.stdout.splitlines() if s.startswith("unboxed-control ")]
                                assert len(lines) == 1, process.stdout
                                values = {k: int(v) for k, v in re.findall(r"(\w+)=(-?\d+)", lines[0].split(" aliases=")[0])}
                                aliases = ast.literal_eval(lines[0].split(" aliases=")[1])
                                row = dict(precise=precise, shape=shape, operations=operations, pc=pc, capture=capture, enabled=enabled,
                                           values=values, aliases=aliases, source_sha256=hashlib.sha256(code.encode()).hexdigest())
                                receipt["rows"].append(row)
                                completed = (int(pc > 0) if shape == "continue" else pc) if enabled else 0
                                materialized = (2 if shape == "equal" and pc == 2 else int(pc > 0)) if enabled else 0
                                assert values["completed"] == completed and values["materialized"] == materialized, row
                                last = pc == (2 if shape == "continue" else operations - 1)
                                answer = (733 if last else 747) if shape == "equal" else 733 + 3 * (int(not last) if shape == "continue" else operations - pc - 1)
                                assert values["answer"] == answer and values["supplied_identity"] == int(last), row
                                assert values["pins_restored"] == 1 and values["pc"] == pc and values["capture"] == int(capture), row
                                if shape == "equal" and pc == 2:
                                    assert len({identity for _, identity, value in aliases if value == 14}) == 2, row
                                observed = (values["answer"], values["supplied_identity"], aliases)
                                if baseline is None:
                                    baseline = observed
                                else:
                                    assert observed == baseline, row
                        print(f"unboxed-control-smoke: precise={precise} shape={shape} operations={operations} pc={pc} capture/suspend passed", flush=True)
        assert sha(floor) == receipt["floor_sha256"]
        assert all(sha(ROOT / p) == digest for p, digest in receipt["source_sha256"].items())
        assert all(sha(p) == digest for p, digest in receipt["installed_sha256"].items())
        receipt["status"] = "passed"
    except BaseException as error:
        receipt.update(status="failed", error=str(error))
        raise
    finally:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
