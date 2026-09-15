#!/usr/bin/env python3
"""Real source repeated-closure state/accounting checks, without timing claims."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile


def source(calls, escaping, precise, numeric=False, *, numeric_copy_region=False):
    assert not numeric_copy_region or numeric
    arithmetic = ("let first := [20 + 15]; let second := [first + first]; "
                  if numeric_copy_region else
                  "let first := [20 + 15]; let second := [first + 7]; ") if numeric else ""
    body = ["let closure := [:arg | let kept := input; " + arithmetic + "let local := arg; local];",
            "let alias := closure;"]
    for call in range(calls):
        argument = ("input", "other", "answer")[call % 3]
        body += ["let alias := alias;", f"let answer := [alias value: {argument}];"]
    body.append("alias" if escaping else "answer")
    return (f"[System preciseGc: {'true' if precise else 'false'}];\n"
            "let source := [:input :other | " + "\n".join(body) + "];\n"
            "let factory := [ClosureControlProbe prepare: [[source codeObject] withoutSourceProvenance] captures: #[]];\n"
            "let input := [73 asCell]; let other := [99 asCell];\n"
            f"[ClosureControlProbe sequence: factory input: input other: other calls: {calls} escape: {int(escaping)}];\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, required=True)
    parser.add_argument("--floor", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--quick", action="store_true")
    parser.add_argument("--numeric-body", action="store_true")
    parser.add_argument("--numeric-copy-region", action="store_true",
                        help="Use a second numeric operation without a new literal, allowing counted copy regions")
    parser.add_argument("--counted-frame-indices", action="store_true")
    args = parser.parse_args()
    if args.numeric_copy_region and not args.numeric_body:
        parser.error('--numeric-copy-region requires --numeric-body')
    env = {k: v for k, v in os.environ.items() if not k.startswith(("BIRNAM_", "DYLD_"))}
    env.update(BIRNAM_ID_DYLIB=str(args.floor.resolve()), BIRNAM_TEST_CLOSURE_CONTROL="1",
               BIRNAM_DEV_CLOSURE_FORCE_RECOVERY="0", BIRNAM_DEV_CLOSURE_REPORT="0")
    env['BIRNAM_DEV_COUNTED_FRAME_INDICES'] = str(int(args.counted_frame_indices))
    receipt = {"status": "running", "scope": "Source sequence correctness and allocation accounting; no timing claim",
               "floor_sha256": hashlib.sha256(args.floor.read_bytes()).hexdigest(),
               "counted_frame_indices": args.counted_frame_indices,
               "numeric_copy_region": args.numeric_copy_region, "rows": []}
    try:
        with tempfile.TemporaryDirectory(prefix="birnam-closure-sequence-") as work:
            fixture = Path(work) / "sequence.birnam"
            for precise in ([1] if args.quick else [0, 1]):
                for calls in ([3, 33] if args.quick else [1, 2, 3, 32, 33]):
                    for escaping in [False, True]:
                        code = source(calls, escaping, precise, numeric=args.numeric_body,
                                      numeric_copy_region=args.numeric_copy_region)
                        fixture.write_text(code)
                        baseline = None
                        force_points = [0, 1, 2] if args.quick else list(range(min(calls, 32)))
                        for enabled, force in [(False, -1), (True, -1)] + [(True, n) for n in force_points]:
                            env.update(BIRNAM_DEV_CLOSURE_ELISION=str(int(enabled)), BIRNAM_DEV_CLOSURE_FORCE_AT=str(force))
                            process = subprocess.run([str(args.prefix.resolve() / "bin/birnam"), str(fixture)],
                                                     env=env, text=True, capture_output=True)
                            assert process.returncode == 0, (calls, escaping, force, process.stdout, process.stderr)
                            lines = [line for line in process.stdout.splitlines() if line.startswith("closure-sequence ")]
                            assert len(lines) == 1, process.stdout
                            values = {k: int(v) for k, v in re.findall(r"(\w+)=(\d+)", lines[0])}
                            row = dict(precise=precise, calls=calls, escaping=escaping, enabled=enabled,
                                       force=force, values=values, source_sha256=hashlib.sha256(code.encode()).hexdigest())
                            receipt["rows"].append(row)
                            assert values["calls"] == calls and values["escaping"] == int(escaping)
                            assert values["invocations"] == 8
                            if baseline is None: baseline = values["bytes"]
                            assert values["bytes"] == baseline > 0, row
                            forced = enabled and 0 <= force < min(calls, 32)
                            assert values["attempts"] == (8 if enabled else 0), row
                            assert values["hits"] == (8 if enabled and not forced else 0), row
                            assert values["completed_calls"] == (8 * (force if forced else min(calls, 32)) if enabled else 0), row
                            assert values["materialized"] == (8 if enabled and not forced and (escaping or calls > 32) else 0), row
                            indices = values['counted_indices']
                            assert 0 <= indices <= values['physical_saved'], row
                            if not args.counted_frame_indices:
                                assert indices == 0, row
                            elif args.numeric_copy_region:
                                assert indices > 0, row
                            assert values["physical_saved"] - indices == (8 if enabled and not forced and not escaping and calls <= 32 else 0), row
                        print(f"closure-sequence-smoke: precise={precise} calls={calls} escaping={int(escaping)} baseline/enabled/forced passed", flush=True)
        receipt["status"] = "passed"
    except BaseException as error:
        receipt.update(status="failed", error=str(error))
        raise
    finally:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
