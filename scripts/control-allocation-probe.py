#!/usr/bin/env python3
"""Measure control-loop live bytes separately from the frozen throughput suite.

Fresh processes, varying iteration counts, empty/captured callbacks, GC on/off.
These are live floor-payload bytes, not allocation counts or RSS; temporary Rust
vectors and registry/arena capacity require separate allocator evidence.
"""
from measurement_archive import load_json
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile


def program(iterations, gc, captures):
    callback = "[:index | [index + offset]]" if captures else "[:index | nil]"
    return f'''[System load: "modules/time.birnam"]
let offset := 17
let callback := {callback}
[System autoGc: {str(gc).lower()}]
[System gc]
let before := [Codegen liveHeapBytes]
let started := [Time monotonicMicros]
let answer := [1 to: {iterations} do: callback]
let elapsed := [[Time monotonicMicros] - started]
let after := [Codegen liveHeapBytes]
[System gc]
let retained := [Codegen liveHeapBytes]
[System println: [before printString]]
[System println: [after printString]]
[System println: [retained printString]]
[System println: [elapsed printString]]
[[answer = 1] ifTrue: [| [System println: "allocation-probe-ok"]]
  ifFalse: [| [System println: "allocation-probe-failed"]]]
[System autoGc: true]
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--iterations", type=int, nargs="+", default=[1000, 10000, 100000])
    args = parser.parse_args()
    if any(n <= 0 for n in args.iterations):
        parser.error("iteration counts must be positive")
    binary = args.binary.resolve(strict=True)
    root = Path(__file__).resolve().parent.parent
    env = {k: v for k, v in os.environ.items() if not k.startswith("BIRNAM_")}
    results = []
    with tempfile.TemporaryDirectory(prefix="birnam-allocation-probe-") as tmp:
        source = Path(tmp) / "probe.birnam"
        for gc in (True, False):
            for captures in (False, True):
                for count in args.iterations:
                    body = program(count, gc, captures)
                    source.write_text(body)
                    run = subprocess.run([str(binary), str(source)], cwd=root, env=env,
                                         capture_output=True, text=True, timeout=180)
                    lines = run.stdout.splitlines()
                    if run.returncode or len(lines) != 5 or lines[-1] != "allocation-probe-ok":
                        raise RuntimeError(f"probe failed: {run.returncode}: {run.stdout}\n{run.stderr}")
                    before, after, retained, elapsed = map(int, lines[:4])
                    results.append(dict(iterations=count, automatic_gc=gc, captures=captures,
                                        before_bytes=before, after_bytes=after,
                                        post_gc_bytes=retained, elapsed_microseconds=elapsed,
                                        fixture_sha256=hashlib.sha256(body.encode()).hexdigest()))
    receipt = load_json(binary.parent.parent / "share/birnam/rust-install.json")
    print(json.dumps(dict(schema="birnam-control-allocation-probe-v1",
                          source_manifest_sha256=receipt["source_manifest_sha256"],
                          results=results), indent=2))


if __name__ == "__main__":
    main()
