#!/usr/bin/env python3
"""Run an isolated Design 66 counter build; never use its elapsed timings.

The fixture's first and last collections delimit the measured region. Include
automatic collections between them, excluding startup and final reporting. Zero-capacity Vec creation
is not counted as a backing allocation. Counts cover the named source sites,
not all Rust allocations in the process.
"""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import resource
import subprocess
import tempfile


def summarize(path):
    segments = []
    current = None
    with path.open() as stream:
        for line in stream:
            fields = line.split()
            if len(fields) != 3 or fields[0] != "D66":
                continue
            kind, value = fields[1], int(fields[2])
            if kind == "gc":
                current = Counter()
                segments.append(current)
            elif current is not None:
                if kind == "arena":
                    current["arena_bytes"] = value
                else:
                    current[kind] += value
                    if kind.endswith(("_words", "_pairs")) and value:
                        current[kind + "_allocations"] += 1
    if len(segments) < 2:
        raise RuntimeError("missing instrumented GC boundaries; use the diagnostic prefix")
    total = Counter()
    for segment in segments[:-1]:
        total.update({key: value for key, value in segment.items() if key != "arena_bytes"})
    result = dict(total)
    result["arena_bytes"] = segments[0]["arena_bytes"]
    result["observed_max_arena_bytes"] = max(s["arena_bytes"] for s in segments)
    result["collections_within_region"] = len(segments) - 2
    result["after_arena_bytes"] = segments[-1]["arena_bytes"]
    result["non_operand_integers"] = result.get("integer", 0) - result.get("operand", 0)
    if result["non_operand_integers"] < 0:
        raise RuntimeError("operand count exceeds Integer count")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--iterations", type=int, nargs="+", default=[100, 1000, 10000])
    parser.add_argument("--recount", action="store_true", help="recompute an existing result from retained raw logs")
    args = parser.parse_args()
    if args.recount:
        path = args.output / "results.json"
        result = json.loads(path.read_text())
        for row in result["results"]:
            log = args.output / f"gc-{row['automatic_gc']}-captures-{row['captures']}-n-{row['iterations']}.stderr"
            row["counters"] = summarize(log)
        path.write_text(json.dumps(result, indent=2) + "\n")
        return
    if any(n <= 0 for n in args.iterations):
        parser.error("iterations must be positive")
    binary = args.binary.resolve(strict=True)
    args.output.mkdir(parents=True, exist_ok=False)
    root = Path(__file__).resolve().parent.parent
    env = {k: v for k, v in os.environ.items() if not k.startswith("BIRNAM_")}
    env["BIRNAM_D66"] = "1"
    results = []
    with tempfile.TemporaryDirectory(prefix="birnam-d66-counters-") as tmp:
        fixture = Path(tmp) / "probe.birnam"
        for gc in (False, True):
            for captures in (False, True):
                for count in args.iterations:
                    callback = "[:index | [index + offset]]" if captures else "[:index | nil]"
                    body = f'''let offset := 17
let callback := {callback}
[System autoGc: {str(gc).lower()}]
[System gc]
let answer := [1 to: {count} do: callback]
[System gc]
[[answer = 1] ifTrue: [| [System println: "counter-probe-ok"]]
  ifFalse: [| [System println: "counter-probe-failed"]]]
'''
                    fixture.write_text(body)
                    log = args.output / f"gc-{gc}-captures-{captures}-n-{count}.stderr"
                    with log.open("w") as err:
                        run = subprocess.run([str(binary), str(fixture)], cwd=root, env=env,
                                             stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                             stderr=err, text=True, timeout=300,
                                             preexec_fn=lambda: resource.setrlimit(
                                                 resource.RLIMIT_FSIZE, (256 * 1024 * 1024,) * 2))
                    if run.returncode or run.stdout.strip() != "counter-probe-ok":
                        raise RuntimeError(f"probe failed: {run.returncode}: {run.stdout!r}; see {log}")
                    results.append(dict(iterations=count, automatic_gc=gc, captures=captures,
                                        fixture_sha256=hashlib.sha256(body.encode()).hexdigest(),
                                        counters=summarize(log)))
    receipt = json.loads((binary.parent.parent / "share/birnam/rust-install.json").read_text())
    (args.output / "results.json").write_text(json.dumps(dict(
        schema="birnam-allocation-counters-v1",
        source_manifest_sha256=receipt["source_manifest_sha256"], results=results), indent=2) + "\n")


if __name__ == "__main__":
    main()
