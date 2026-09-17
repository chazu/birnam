#!/usr/bin/env python3
"""Build a worker image and run the Birnam multicore prime-counting example."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("limit", nargs="?", type=int, default=10_000,
                        help="count primes through this integer (default: 10000)")
    args = parser.parse_args()
    if not 3 <= args.limit <= 10_000_000:
        parser.error("limit must be between 3 and 10000000")
    binary = shutil.which(os.environ.get("BIRNAM_BIN", "birnam"))
    if binary is None:
        parser.error("birnam was not found; install the current release or set BIRNAM_BIN")
    example = Path(__file__).resolve().parent
    quote = json.dumps

    # Only launch/setup/reporting live in Python. Both counting jobs and their
    # task orchestration live in the two .birnam files beside this runner.
    with tempfile.TemporaryDirectory(prefix="birnam-multicore-demo-") as temporary:
        work = Path(temporary)
        image = work / "workers.image"
        clock = work / "clock.birnam"
        clock.write_text(f'''
[Alien wrap: "clock_gettime" signature: "i i p" as: #demoTimeInto: on: Integer];
[Globals at: #DemoClock put: [Object clone]];
[Reflect defineMethod: #read: on: DemoClock as: [:self :clock |
  let stamp := [Alien alloc: 16];
  [clock demoTimeInto: stamp];
  let micros := [[[stamp peekIntAt: 0] * 1000000] + [[stamp peekIntAt: 8] / 1000]];
  [stamp free]; micros]];
[Reflect defineMethod: #now on: DemoClock as: [:self | [self read: {time.CLOCK_MONOTONIC}]]];
[Reflect defineMethod: #cpu on: DemoClock as: [:self | [self read: {time.CLOCK_PROCESS_CPUTIME_ID}]]];
''')
        setup = f'[System load: {quote(str(clock))}];\n'
        save = work / "save.birnam"
        save.write_text(setup + f'''
[System load: {quote(str(example / 'worker.birnam'))}];
[DispatchGeneration initializeCurrent];
[System saveImage: {quote(str(image))}];
[System println: [[System imageSaveReport] status]];
''')
        print(f"Counting primes through {args.limit:,} with two Birnam worker processes.", flush=True)
        print("Preparing the worker image and resumable coordinator...", flush=True)
        saved = subprocess.run([binary, str(save)], check=True, text=True, capture_output=True)
        if "#saved" not in saved.stdout.splitlines():
            raise RuntimeError(f"worker image was not saved:\n{saved.stdout}\n{saved.stderr}")
        source = (example / "main.birnam").read_text()
        candidate = work / "coordinator.dylib"
        run = work / "run.birnam"
        run.write_text(setup + f'''
let source := {quote(source)};
[Codegen eval: source];
[Codegen recordModuleSource: source named: #multicoreDemo];
[DispatchGeneration initializeCurrent];
[CompiledUnit compile: source named: #multicoreDemo output: {quote(str(candidate))} selection: #resumableRequired];
let unit := [CompiledUnit open: {quote(str(candidate))}];
[unit map]; [[unit activate] fulfill];
[MulticoreDemo run: {quote(str(image))} limit: {args.limit}];
''')
        match = None
        with subprocess.Popen([binary, str(run)], text=True, stdout=subprocess.PIPE) as process:
            for line in process.stdout:
                measurement = re.fullmatch(
                    r"#\[#primeDemo (\d+) (\d+) (\d+) (\d+) (\d+)\]\n?", line)
                if measurement:
                    match = measurement
                else:
                    print(line, end="", flush=True)
            if process.wait() != 0:
                raise RuntimeError("Birnam exited before completing the demo")
        if match is None:
            raise RuntimeError("demo did not produce results; use a release with asynchronous process workers")
        sequential_count, parallel_count, sequential, parallel, cpu = map(int, match.groups())
        if sequential_count != parallel_count:
            raise RuntimeError("sequential and parallel counts disagree")
        print(f"\nBoth runs found {parallel_count:,} primes.")
        print(f"Sequential: {sequential / 1e6:.3f} s")
        print(f"Parallel:   {parallel / 1e6:.3f} s")
        print(f"Speedup:    {sequential / max(parallel, 1):.2f}x")
        print(f"Worker CPU / parallel elapsed: {cpu / max(parallel, 1):.2f} cores on average")
        print("CPU / elapsed above 1 confirms simultaneous execution on multiple cores.")
        print("Image creation, compilation, and worker startup are excluded from these timings.")


if __name__ == "__main__":
    main()
