#!/usr/bin/env python3
"""Source closure capture lifetime and observing-finalizer journal settlement."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile


def source(calls, observing, precise, native, capacity):
    body = ["let closure := [:arg | let kept := input; let local := arg; local];",
            "let alias := closure;"]
    body += ["let answer := [alias value: input];"] * calls
    body.append("answer")
    return (f"[System preciseGc: {'true' if precise else 'false'}];\n"
            "let source := [:input | " + "\n".join(body) + "];\n"
            "let factory := [ClosureControlProbe prepare: [[source codeObject] withoutSourceProvenance] captures: #[]];\n"
            f"[ClosureControlProbe sequenceJournal: factory calls: {calls} observe: {observing} capacity: {capacity}];\n"
            f"[ClosureControlProbe inspectJournal: {native}];\n"
            # Keep the factory's owner live through collection, independently
            # of the private input allocated within the test-floor call.
            f"[ClosureControlProbe journalRestored: factory calls: {calls}];\n")


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
        with tempfile.TemporaryDirectory(prefix="birnam-closure-journal-") as work:
            fixture = Path(work) / "journal.birnam"
            for precise in (0, 1):
                for calls, capacity in ((3, 0), (32, 0), (3, 1)):
                    for observing in (0, 1):
                        for native in (0, 1, 2):
                            code = source(calls, observing, precise, native, capacity)
                            fixture.write_text(code)
                            baseline = None
                            for enabled in (0, 1):
                                env["BIRNAM_DEV_CLOSURE_ELISION"] = str(enabled)
                                process = subprocess.run([str(args.prefix.resolve() / "bin/birnam"), str(fixture)],
                                                         env=env, text=True, capture_output=True)
                                assert process.returncode == 0, (precise, calls, observing, native, capacity, enabled, process.stdout, process.stderr)
                                values = {}
                                for phase in ("run", "inspect", "restored"):
                                    lines = [line for line in process.stdout.splitlines() if line.startswith(f"closure-journal-{phase} ")]
                                    assert len(lines) == 1, process.stdout
                                    values[phase] = {k: int(v) for k, v in re.findall(r"(\w+)=(-?\d+)", lines[0])}
                                row = dict(precise=precise, calls=calls, observing=observing, native=native, capacity=capacity,
                                           enabled=enabled, values=values, source_sha256=hashlib.sha256(code.encode()).hexdigest())
                                receipt["rows"].append(row)
                                run = values["run"]
                                assert run["calls"] == calls and run["observing"] == observing, row
                                assert run["bytes"] > 0 and run["materialized"] == 0, row
                                iterations = run["limit"] + 1 if capacity else 1
                                assert run["iterations"] == iterations, row
                                assert run["attempts"] == enabled * iterations, row
                                for counter in ("hits", "physical_saved"):
                                    assert run[counter] == enabled * min(iterations, run["limit"]), row
                                assert run["completed_calls"] == enabled * calls * min(iterations, run["limit"]), row
                                assert values["restored"] == dict(hits=enabled, materialized=0, completed_calls=enabled*calls), row
                                assert 0 <= values["inspect"]["journal_storage"] <= run["journal_storage"], row
                                if native == 2:
                                    assert values["inspect"]["first_input_alive"] == values["inspect"]["first_child_alive"] == 1, row
                                    assert values["inspect"]["first_finalized"] == 0, row
                                # Keep incidental native-root retention in the
                                # receipt. Compare exact accounting and lifetime
                                # after the explicit native root has unwound.
                                observable = dict(bytes=run["bytes"], **{k:v for k,v in values["inspect"].items()
                                                  if k != "journal_storage" and not k.startswith("first_")})
                                if baseline is None: baseline = observable
                                assert observable == baseline, (baseline, row)
                            print(f"closure-journal-smoke: precise={precise} calls={calls} observing={observing} native={native} capacity={capacity} baseline/enabled lifetime and finalizer checks passed", flush=True)
        receipt["status"] = "passed"
    except BaseException as error:
        receipt.update(status="failed", error=str(error))
        raise
    finally:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
