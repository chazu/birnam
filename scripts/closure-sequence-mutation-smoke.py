#!/usr/bin/env python3
"""Later-target mutation and immediate restoration in real source closure regions."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile


def source(position, collecting, precise):
    body = ["let closure := [:arg | let kept := input; let local := arg; local];",
            "let alias := closure;"]
    for call in range(3):
        selector = "closureSequenceTail:" if call == position else "value:"
        argument = "input" if call == 0 else "other"
        body.append(f"let answer := [alias {selector} {argument}];")
    body.append("alias")
    return (f"[System preciseGc: {'true' if precise else 'false'}];\n"
            "let source := [:input :other | " + "\n".join(body) + "];\n"
            "let factory := [ClosureControlProbe prepare: [[source codeObject] withoutSourceProvenance] captures: #[]];\n"
            "let input := [73 asCell]; let other := [99 asCell];\n"
            f"[ClosureControlProbe sequenceMutation: factory input: input other: other position: {position} collect: {collecting}];\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, required=True)
    parser.add_argument("--floor", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    env = {k: v for k, v in os.environ.items() if not k.startswith(("BIRNAM_", "DYLD_"))}
    env.update(BIRNAM_ID_DYLIB=str(args.floor.resolve()), BIRNAM_TEST_CLOSURE_CONTROL="1",
               BIRNAM_DEV_CLOSURE_FORCE_RECOVERY="0", BIRNAM_DEV_CLOSURE_FORCE_AT="-1",
               BIRNAM_DEV_CLOSURE_REPORT="0")
    receipt = {"status": "running", "scope": __doc__ + " No timing claim.",
               "floor_sha256": hashlib.sha256(args.floor.read_bytes()).hexdigest(), "rows": []}
    try:
        with tempfile.TemporaryDirectory(prefix="birnam-closure-mutation-") as work:
            fixture = Path(work) / "mutation.birnam"
            for precise in (0, 1):
                for position in (1, 2):
                    for collecting in (0, 1):
                        code = source(position, collecting, precise)
                        fixture.write_text(code)
                        baseline = None
                        for enabled in (0, 1):
                            env["BIRNAM_DEV_CLOSURE_ELISION"] = str(enabled)
                            process = subprocess.run([str(args.prefix.resolve() / "bin/birnam"), str(fixture)],
                                                     env=env, text=True, capture_output=True)
                            assert process.returncode == 0, (precise, position, collecting, enabled, process.stdout, process.stderr)
                            lines = [line for line in process.stdout.splitlines() if line.startswith("closure-sequence-mutation ")]
                            assert len(lines) == 1, process.stdout
                            values = {k: int(v) for k, v in re.findall(r"(\w+)=(\d+)", lines[0])}
                            row = dict(precise=precise, position=position, collecting=collecting,
                                       enabled=enabled, values=values, source_sha256=hashlib.sha256(code.encode()).hexdigest())
                            receipt["rows"].append(row)
                            assert values["position"] == position and values["collecting"] == collecting, row
                            assert values["invocations"] == values["effects"] == 8, row
                            if baseline is None:
                                baseline = values["bytes"]
                            assert values["bytes"] == baseline and (collecting or baseline > 0), row
                            for counter in ("attempts", "hits", "materialized"):
                                assert values[counter] == enabled * 8, row
                            assert values["completed_calls"] == enabled * 8 * position, row
                            assert values["physical_saved"] == 0, row
                            assert values["restored_hits"] == values["restored_materialized"] == enabled, row
                            assert values["restored_calls"] == enabled * 3, row
                        print(f"closure-sequence-mutation-smoke: precise={precise} position={position} collecting={collecting} baseline/enabled passed", flush=True)
        receipt["status"] = "passed"
    except BaseException as error:
        receipt.update(status="failed", error=str(error))
        raise
    finally:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
