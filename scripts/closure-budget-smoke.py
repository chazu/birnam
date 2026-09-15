#!/usr/bin/env python3
"""Compare real automatic GC boundaries around source closure regions."""
from measurement_archive import read_bytes
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile


def source(calls, escaping, precise, gap, numeric=False, *, numeric_copy_region=False):
    assert not numeric_copy_region or numeric
    arithmetic = ("let first := [20 + 15]; let second := [first + first]; "
                  if numeric_copy_region else
                  "let first := [20 + 15]; let second := [first + 7]; ") if numeric else ""
    body = ["let closure := [:arg | let kept := input; " + arithmetic + "let local := arg; local];",
            "let alias := closure;"]
    for _ in range(calls):
        body.append("let answer := [alias value: input];")
    body.append("alias" if escaping else "answer")
    return (f"[System preciseGc: {'true' if precise else 'false'}];\n"
            "let source := [:input | " + "\n".join(body) + "];\n"
            "let factory := [ClosureControlProbe prepare: [[source codeObject] withoutSourceProvenance] captures: #[]];\n"
            f"[ClosureControlProbe sequenceBudget: factory gap: {gap} escape: {int(escaping)}];\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, required=True)
    parser.add_argument("--floor", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--quick", action="store_true")
    parser.add_argument("--numeric-body", action="store_true")
    parser.add_argument("--numeric-copy-region", action="store_true")
    parser.add_argument("--counted-frame-indices", action="store_true")
    parser.add_argument("--coverage", choices=("routine", "exhaustive"), default="exhaustive")
    args = parser.parse_args()
    if args.numeric_copy_region and not args.numeric_body:
        parser.error('--numeric-copy-region requires --numeric-body')
    env = {k: v for k, v in os.environ.items() if not k.startswith(("BIRNAM_", "DYLD_"))}
    env.update(BIRNAM_ID_DYLIB=str(args.floor.resolve()), BIRNAM_TEST_CLOSURE_CONTROL="1",
               BIRNAM_DEV_CLOSURE_FORCE_RECOVERY="0", BIRNAM_DEV_CLOSURE_FORCE_AT="-1",
               BIRNAM_DEV_CLOSURE_REPORT="0")
    env['BIRNAM_DEV_COUNTED_FRAME_INDICES'] = str(int(args.counted_frame_indices))
    receipt = {"status": "running", "scope": __doc__ + " No timing claim.",
               "floor_sha256": hashlib.sha256(read_bytes(args.floor)).hexdigest(),
               "coverage": args.coverage, "counted_frame_indices": args.counted_frame_indices,
               "numeric_copy_region": args.numeric_copy_region, "rows": []}
    try:
        with tempfile.TemporaryDirectory(prefix="birnam-closure-budget-") as work:
            fixture = Path(work) / "budget.birnam"
            for precise in ([1] if args.quick else [0, 1]):
                for calls in (3, 32):
                    for escaping in (False, True):
                        completed = set()
                        collections = set()
                        counted_events = set()
                        initial_gap = 65536 if args.numeric_body else 16384
                        gaps = [initial_gap, 64, 128, 256, 512, 1024, 2048, 4096, 8192]
                        if args.numeric_body:
                            gaps.extend([16384, 32768])
                        if args.coverage == "routine":
                            gaps = [initial_gap, 64]
                        for gap in gaps:
                            code = source(calls, escaping, precise, gap, numeric=args.numeric_body,
                                          numeric_copy_region=args.numeric_copy_region)
                            fixture.write_text(code)
                            baseline = None
                            for enabled in (0, 1):
                                env["BIRNAM_DEV_CLOSURE_ELISION"] = str(enabled)
                                process = subprocess.run([str(args.prefix.resolve() / "bin/birnam"), str(fixture)],
                                                         env=env, text=True, capture_output=True, timeout=60)
                                assert process.returncode == 0, (precise, calls, escaping, gap, enabled, process.stdout, process.stderr)
                                lines = [line for line in process.stdout.splitlines() if line.startswith("closure-budget ")]
                                assert len(lines) == 1, process.stdout
                                values = {k: int(v) for k, v in re.findall(r"(\w+)=(\d+)", lines[0])}
                                row = dict(precise=precise, calls=calls, escaping=escaping, gap=gap,
                                           enabled=enabled, values=values, source_sha256=hashlib.sha256(code.encode()).hexdigest())
                                receipt["rows"].append(row)
                                observable = {k: v for k, v in values.items() if k in (
                                    "gap", "threshold", "escaping", "collections", "trace_bytes", "trace_pc", "trace_pcs", "trace_slots", "remaining_bytes")}
                                if baseline is None: baseline = observable
                                assert observable == baseline, (baseline, row)
                                if gap == initial_gap and not enabled:
                                    assert values["collections"] == 0, row
                                    body_bytes = values["remaining_bytes"] - (values["threshold"] - gap)
                                    assert body_bytes > 64 and body_bytes + 8 < gap and body_bytes % 8 == 0, row
                                    # Baseline execution supplies the exact byte
                                    # extent, independent of optimizer metadata.
                                    boundaries = [body_bytes - 8, body_bytes, body_bytes + 8]
                                    if args.coverage == "routine": boundaries.insert(0, body_bytes // 2 // 8 * 8)
                                    gaps.extend(n for n in boundaries if n not in gaps)
                                assert values["attempts"] == enabled, row
                                n = values["completed_calls"]
                                assert 0 <= n <= calls and (enabled or n == 0), row
                                assert values["hits"] == int(n > 0), row
                                assert values["materialized"] == int(n > 0 and (escaping or n < calls)), row
                                indices = values['counted_indices']
                                assert 0 <= indices <= values['physical_saved'], row
                                assert indices <= values['counted_recorded'], row
                                if not args.counted_frame_indices:
                                    assert indices == values['counted_recorded'] == 0, row
                                assert values["physical_saved"] - indices == int(n == calls and not escaping and values["collections"] == 0), row
                                counted_events.add(values['counted_recorded'])
                                if enabled:
                                    completed.add(n)
                                    collections.add(values["collections"])
                        assert 0 in completed and calls in completed and any(0 < n < calls for n in completed), completed
                        assert collections == {0, 1}, collections
                        if args.numeric_copy_region and args.counted_frame_indices:
                            assert any(count > 0 for count in counted_events), counted_events
                        print(f"closure-budget-smoke: precise={precise} calls={calls} escaping={int(escaping)} prefixes={sorted(completed)} baseline/enabled GC traces passed", flush=True)
        receipt["status"] = "passed"
    except BaseException as error:
        receipt.update(status="failed", error=str(error))
        raise
    finally:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
