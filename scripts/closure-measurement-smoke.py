#!/usr/bin/env python3
"""Validate repeated-closure measurement counters; timings here are unqualified."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
from closure_timing_common import source, parse_output, check_pair


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, required=True)
    parser.add_argument("--floor", type=Path, required=True)
    parser.add_argument("--profile", choices=("debug", "release"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    env = {k: v for k, v in os.environ.items() if not k.startswith(("BIRNAM_", "DYLD_"))}
    env.update(BIRNAM_ID_DYLIB=str(args.floor.resolve()), BIRNAM_TEST_CLOSURE_CONTROL="1",
               BIRNAM_DEV_CLOSURE_FORCE_RECOVERY="0", BIRNAM_DEV_CLOSURE_FORCE_AT="-1",
               BIRNAM_DEV_CLOSURE_REPORT="0", BIRNAM_CLOSURE_BATCHES="2")
    receipt = {"status": "running", "scope": __doc__,
               "floor_sha256": hashlib.sha256(args.floor.read_bytes()).hexdigest(), "rows": []}
    try:
        with tempfile.TemporaryDirectory(prefix="birnam-closure-measurement-") as work:
            fixture = Path(work) / "measurement.birnam"
            for uses in (1, 3, 32):
                code = source(uses)
                fixture.write_text(code)
                env["BIRNAM_CLOSURE_USES"] = str(uses)
                for shape in range(4):
                    pair = {}
                    for enabled in (0, 1):
                        env.update(BIRNAM_CLOSURE_SHAPE=str(shape), BIRNAM_DEV_CLOSURE_ELISION=str(enabled))
                        process = subprocess.run([str(args.prefix.resolve() / "bin/birnam"), str(fixture)],
                                                 env=env, text=True, capture_output=True)
                        assert process.returncode == 0, (uses, shape, enabled, process.stdout, process.stderr)
                        values = parse_output(process.stdout)
                        assert values["measure"]["uses"] == uses and values["measure"]["shape"] == shape, values
                        pair[enabled] = values
                        receipt["rows"].append(dict(uses=uses, shape=shape, enabled=enabled, values=values,
                                                   source_sha256=hashlib.sha256(code.encode()).hexdigest()))
                    check_pair(pair[0], pair[1], 2)
                    print(f"closure-timing-smoke: profile={args.profile} shape={shape} uses={uses} counter/allocation/result checks passed; timings unqualified", flush=True)
        receipt["status"] = "passed"
    except BaseException as error:
        receipt.update(status="failed", error=str(error))
        raise
    finally:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
