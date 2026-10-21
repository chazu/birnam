#!/usr/bin/env -S uv run --script
# /// script
# dependencies = ["pyte==0.8.2"]
# ///
"""Measure the standard project REPL against ordinary console startup.

uv run scripts/presentation-startup.py --samples 3
Build time is recorded separately. Fresh processes use warm filesystem caches.
"""
import argparse
import json
import os
from pathlib import Path
import runpy
import statistics
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "examples/presentation-repl"
HARNESS = runpy.run_path(str(ROOT / "scripts/presentation-repl-test.py"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--samples", type=int, default=3)
    parser.add_argument("--output", type=Path, default=ROOT / ".cargo-target/measurements/presentation-project-startup.json")
    args = parser.parse_args()
    assert args.samples > 0
    start = time.perf_counter()
    subprocess.run(["sh", str(APP / "build.sh")], check=True)
    build_ms = (time.perf_counter() - start) * 1000
    rows = []
    for _ in range(args.samples):
        ordinary = HARNESS["plain_startup"]()
        terminal = HARNESS["Terminal"]()
        try:
            elapsed = terminal.startup_ms
            terminal.evaluate("[1 + 2]", "#1 3")
            terminal.stop()
            rows.append({"ordinary_ms": ordinary, "project_ms": elapsed})
            print(json.dumps(rows[-1]), flush=True)
        finally:
            terminal.close()
    result = {"binary": os.environ.get("BIRNAM", "birnam"), "build_ms": build_ms,
              "runs": rows, "medians_ms": {key: statistics.median(r[key] for r in rows) for key in rows[0]},
              "scope": "Standard project build/run; warm filesystem, fresh processes, real PTY and evaluated result."}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result["medians_ms"], indent=2))


if __name__ == "__main__":
    main()
