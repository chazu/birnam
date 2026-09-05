#!/usr/bin/env python3
"""Summarize live allocation snapshots, not cumulative allocation traffic.

Group by the allocating Birnam function (plus its immediate caller). Comparing
within one process keeps JIT addresses stable. Negative deltas are retained.
"""
import argparse
from collections import defaultdict
import json
from pathlib import Path
import re


def read(path):
    result = defaultdict(lambda: [0, 0])
    for block in re.split(r"\n\s*\n", path.read_text()):
        match = re.match(r"(\d+) calls for (\d+) bytes:", block)
        if not match:
            continue
        frames = block.splitlines()[1:]
        selected = []
        for index, frame in enumerate(frames):
            if "birnam" in frame and "Heap::allocate" not in frame:
                selected = frames[index:index + 2]
                break
        key = " | ".join(re.sub(r"^\d+\s+\S+\s+0x[0-9a-f]+\s+", "", f)
                         for f in selected) or "non-Birnam"
        counts = result[key]
        counts[0] += int(match[1])
        counts[1] += int(match[2])
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    before = read(args.directory / "before-allByCount.txt")
    after = read(args.directory / "after-allByCount.txt")
    rows = []
    for key in before.keys() | after.keys():
        calls = after[key][0] - before[key][0]
        size = after[key][1] - before[key][1]
        if calls or size:
            rows.append(dict(stack=key, live_count_delta=calls, live_bytes_delta=size))
    rows.sort(key=lambda row: (-row["live_count_delta"], row["stack"]))
    print(json.dumps(rows, indent=2))


if __name__ == "__main__":
    main()
