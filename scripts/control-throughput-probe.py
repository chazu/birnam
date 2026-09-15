#!/usr/bin/env python3
"""Run Design 66's control probe in fresh processes; optionally sample separately.

python3 scripts/control-throughput-probe.py /path/to/prefix/bin/birnam \
    --sample /tmp/control.sample.txt

The sample run is excluded from timing medians. This diagnostic does not alter
the frozen benchmark suite and is not release acceptance evidence.
"""
from measurement_archive import load_json, read_bytes
import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess


def probe(binary, root, sample=None):
    env = {k: v for k, v in os.environ.items() if not k.startswith("BIRNAM_")}
    with subprocess.Popen([str(binary), "bench/control-throughput-probe.birnam"],
                          cwd=root, env=env, stdin=subprocess.DEVNULL,
                          stdout=subprocess.PIPE, text=True) as child:
        if child.stdout.readline().strip() != "control-probe-ready":
            raise RuntimeError("probe failed before the timed region")
        if sample:
            subprocess.run(["/usr/bin/sample", str(child.pid), "3", "1", "-file", sample],
                           check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        output = child.communicate(timeout=120)[0].splitlines()
        if child.returncode != 0 or len(output) != 2 or output[1] != "control-probe-ok":
            raise RuntimeError(f"probe failed: exit={child.returncode}, output={output!r}")
        return int(output[0])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--sample", help="optional macOS sample output path")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    root = Path(__file__).resolve().parent.parent
    elapsed = [probe(binary, root) for _ in range(3)]
    sampled = probe(binary, root, args.sample) if args.sample else None
    receipt = load_json(binary.parent.parent / "share/birnam/rust-install.json")
    print(json.dumps({"schema": "birnam-control-throughput-probe-v1",
                      "binary_sha256": hashlib.sha256(read_bytes(binary)).hexdigest(),
                      "source_manifest_sha256": receipt["source_manifest_sha256"],
                      "iterations": 100000, "elapsed_microseconds": elapsed,
                      "median_ns_per_iteration": statistics.median(elapsed) / 100,
                      "sample_path": args.sample, "sampled_elapsed_microseconds": sampled}, indent=2))


if __name__ == "__main__":
    main()
