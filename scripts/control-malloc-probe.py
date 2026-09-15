#!/usr/bin/env python3
"""Capture macOS allocator history around an isolated control workload.

Uses Apple's malloc stack logging, not a production runtime instrumentation
hook. Never use these instrumented elapsed times as performance measurements.
The two snapshots delimit startup from the loop. -allEvents includes freed
Rust scratch allocations; -allByCount shows only still-live allocations.
"""
from measurement_archive import load_json
import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--iterations", type=int, default=100)
    parser.add_argument("--captures", action="store_true")
    parser.add_argument("--gc", action="store_true")
    parser.add_argument("--all-events", action="store_true",
                        help="include freed allocations (may exceed the 256 MiB snapshot cap)")
    args = parser.parse_args()
    if args.iterations <= 0:
        parser.error("iterations must be positive")
    binary = args.binary.resolve(strict=True)
    args.output.mkdir(parents=True, exist_ok=False)
    root = Path(__file__).resolve().parent.parent
    callback = "[:index | [index + offset]]" if args.captures else "[:index | nil]"
    source = f'''let offset := 17
let callback := {callback}
[System autoGc: {str(args.gc).lower()}]
[System gc]
[System println: "malloc-probe-before"]
[System readLine]
let answer := [1 to: {args.iterations} do: callback]
[System println: "malloc-probe-after"]
[System readLine]
[[answer = 1] ifTrue: [| [System println: "malloc-probe-ok"]]
  ifFalse: [| [System println: "malloc-probe-failed"]]]
'''
    env = {k: v for k, v in os.environ.items() if not k.startswith("BIRNAM_")}
    env.update(MallocStackLogging="1", MallocStackLoggingNoCompact="1")
    with tempfile.TemporaryDirectory(prefix="birnam-malloc-probe-") as tmp:
        fixture = Path(tmp) / "probe.birnam"
        fixture.write_text(source)
        with (args.output / "runtime.stderr").open("w") as err:
            child = subprocess.Popen([str(binary), str(fixture)], cwd=root, env=env,
                                     stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=err, text=True)
            try:
                for phase in ("before", "after"):
                    line = child.stdout.readline().strip()
                    if line != f"malloc-probe-{phase}":
                        raise RuntimeError(f"unexpected marker: {line!r}; see runtime.stderr")
                    modes = ("allByCount", "allEvents") if args.all_events else ("allByCount",)
                    for mode in modes:
                        with (args.output / f"{phase}-{mode}.txt").open("w") as out:
                            subprocess.run(["/usr/bin/malloc_history", str(child.pid),
                                            "-" + mode, "-fullStacks"], stdout=out,
                                           stderr=subprocess.PIPE, text=True,
                                           check=True, timeout=120,
                                           preexec_fn=lambda: resource.setrlimit(
                                               resource.RLIMIT_FSIZE, (256 * 1024 * 1024,) * 2))
                    child.stdin.write("\n")
                    child.stdin.flush()
                output = child.communicate(timeout=30)[0].strip()
                if child.returncode or output != "malloc-probe-ok":
                    raise RuntimeError(f"probe failed: {child.returncode}: {output!r}")
            finally:
                if child.poll() is None:
                    child.kill()
                    child.wait()
    receipt = load_json(binary.parent.parent / "share/birnam/rust-install.json")
    (args.output / "receipt.json").write_text(json.dumps(dict(
        schema="birnam-control-malloc-probe-v1", iterations=args.iterations,
        automatic_gc=args.gc, captures=args.captures,
        source_manifest_sha256=receipt["source_manifest_sha256"],
        fixture_sha256=hashlib.sha256(source.encode()).hexdigest()), indent=2) + "\n")


if __name__ == "__main__":
    main()
