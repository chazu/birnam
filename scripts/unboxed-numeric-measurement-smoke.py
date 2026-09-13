#!/usr/bin/env python3
"""Verify numeric measurement counters on installed source before timing."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
from unboxed_numeric_timing_common import source, parse_output, check_pair, peak_rss

ROOT = Path(__file__).resolve().parent.parent


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, required=True)
    parser.add_argument("--floor", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--profile", choices=("debug", "release"), required=True)
    parser.add_argument("--quick", action="store_true")
    args = parser.parse_args()
    prefix, floor = args.prefix.resolve(), args.floor.resolve()
    env = {k: v for k, v in os.environ.items() if not k.startswith(("BIRNAM_", "DYLD_"))}
    env.update(BIRNAM_TEST_UNBOXED_NUMERIC="1", BIRNAM_ID_DYLIB=str(floor), BIRNAM_NUMERIC_BATCHES="2")
    paths = [Path(__file__).resolve(), ROOT / "scripts/unboxed_numeric_timing_common.py",
             ROOT / "rust/birnam-id/src/runtime/native_tier.rs",
             ROOT / "rust/birnam-id/src/runtime/native_tier/unboxed_numeric.rs",
             ROOT / "rust/birnam-id/src/runtime/unboxed_numeric_probe.rs",
             ROOT / "rust/birnam-id/src/runtime/unboxed_numeric_probe/measure.rs",
             ROOT / "rust/birnam-codegen/src/numeric_chain/recovery_maps.rs"]
    paths += sorted((ROOT / "rust/birnam-id/src/runtime/native_tier/unboxed_numeric").rglob("*.rs"))
    installed = [prefix / "bin/birnam", prefix / "share/birnam/rust-install.json"] + sorted((prefix / "lib").glob("*.dylib"))
    receipt = dict(status="running", profile=args.profile, scope="Measurement correctness only; no qualified timing claim",
                   floor_sha256=sha(floor), source_sha256={str(p.relative_to(ROOT)): sha(p) for p in paths},
                   installed_sha256={str(p): sha(p) for p in installed}, rows=[])
    try:
        with tempfile.TemporaryDirectory(prefix="birnam-numeric-measure-") as work:
            fixture = Path(work) / "measure.birnam"
            for operations in ([3] if args.quick else [3, 16, 32]):
                code = source(operations)
                fixture.write_text(code)
                for shape in range(5):
                    pair = []
                    for enabled in (0, 1):
                        env.update(BIRNAM_DEV_UNBOXED_NUMERIC=str(enabled), BIRNAM_NUMERIC_SHAPE=str(shape))
                        process = subprocess.run(["/usr/bin/time", "-l", str(prefix / "bin/birnam"), str(fixture)], env=env,
                                                 cwd=ROOT, text=True, capture_output=True)
                        assert process.returncode == 0, (process.returncode, operations, shape, enabled, process.stdout, process.stderr)
                        values = parse_output(process.stdout)
                        receipt["rows"].append(dict(operations=operations, shape=shape, enabled=enabled,
                                                   source_sha256=hashlib.sha256(code.encode()).hexdigest(),
                                                   peak_rss_bytes=peak_rss(process.stderr), **values))
                        pair.append(values)
                    check_pair(*pair, batches=2)
                    print(f"unboxed-measurement-smoke: profile={args.profile} operations={operations} shape={shape} passed", flush=True)
        assert sha(floor) == receipt["floor_sha256"]
        assert all(sha(ROOT / p) == digest for p, digest in receipt["source_sha256"].items())
        assert all(sha(p) == digest for p, digest in receipt["installed_sha256"].items())
        receipt["status"] = "passed"
    except BaseException as error:
        receipt.update(status="failed", error=str(error))
        raise
    finally:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
