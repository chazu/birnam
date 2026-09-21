#!/usr/bin/env python3
"""Validate repeated-closure measurement counters; timings here are unqualified."""
from measurement_archive import load_json, read_bytes
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
    parser.add_argument("--numeric-body", action="store_true")
    parser.add_argument("--uses", type=int, nargs="+", choices=range(1, 33), default=[1, 3, 4, 8, 16, 32])
    args = parser.parse_args()
    args.uses = list(dict.fromkeys(args.uses))
    env = {k: v for k, v in os.environ.items() if not k.startswith(("BIRNAM_", "DYLD_"))}
    env.update(BIRNAM_ID_DYLIB=str(args.floor.resolve()), BIRNAM_TEST_CLOSURE_CONTROL="1",
               BIRNAM_DEV_CLOSURE_FORCE_RECOVERY="0", BIRNAM_DEV_CLOSURE_FORCE_AT="-1",
               BIRNAM_DEV_CLOSURE_REPORT="0", BIRNAM_CLOSURE_BATCHES="2")
    root = Path(__file__).resolve().parent.parent
    paths = [Path(__file__).resolve(), root / "scripts/closure_timing_common.py",
             root / "rust/birnam-runtime/src/runtime/closure_timing_probe.rs"]
    paths += sorted(p for p in (root / "rust/birnam-native-compiler").rglob("*") if p.is_file())
    paths += sorted(p for p in (root / "rust/birnam-id").rglob("*") if p.is_file())
    paths += sorted(p for p in (root / "rust/birnam-runtime").rglob("*") if p.is_file())
    paths += [root / "vendor/libtcc-src" / name for name in ("tcc.h", "config.h", "tccrun.c", "libtcc.c")]
    hashes = {str(p.relative_to(root)): hashlib.sha256(read_bytes(p)).hexdigest() for p in paths}
    manifest = args.prefix.resolve() / "share/birnam/rust-install.json"
    publication = load_json(manifest)
    assert publication["publication"]["state"] == "published"
    artifacts = dict(publication["staged_manifest"])
    artifacts["share/birnam/rust-install.json"] = hashlib.sha256(read_bytes(manifest)).hexdigest()
    receipt = {"status": "running", "qualified": False, "numeric_body": args.numeric_body, "installed_sha256": artifacts, "uses": args.uses, "source_sha256": hashes, "scope": __doc__,
               "floor_sha256": hashlib.sha256(read_bytes(args.floor)).hexdigest(), "rows": []}
    try:
        with tempfile.TemporaryDirectory(prefix="birnam-closure-measurement-") as work:
            fixture = Path(work) / "measurement.birnam"
            for uses in args.uses:
                code = source(uses, numeric=args.numeric_body)
                fixture.write_text(code)
                env["BIRNAM_CLOSURE_USES"] = str(uses)
                for shape in range(4):
                    pair = {}
                    for enabled in (0, 1):
                        env.update(BIRNAM_CLOSURE_SHAPE=str(shape), BIRNAM_DEV_CLOSURE_ELISION=str(enabled))
                        process = subprocess.run([str(args.prefix.resolve() / "bin/birnam"), str(fixture)],
                                                 env=env, text=True, capture_output=True, timeout=600)
                        assert process.returncode == 0, (uses, shape, enabled, process.stdout, process.stderr)
                        values = parse_output(process.stdout)
                        assert values["measure"]["uses"] == uses and values["measure"]["shape"] == shape, values
                        pair[enabled] = values
                        receipt["rows"].append(dict(uses=uses, shape=shape, enabled=enabled, values=values,
                                                   source_sha256=hashlib.sha256(code.encode()).hexdigest()))
                    check_pair(pair[0], pair[1], 2, numeric=args.numeric_body)
                    print(f"closure-timing-smoke: profile={args.profile} shape={shape} uses={uses} counter/allocation/result checks passed; timings unqualified", flush=True)
        assert hashlib.sha256(read_bytes(args.floor)).hexdigest() == receipt["floor_sha256"]
        assert all(hashlib.sha256(read_bytes(root / name)).hexdigest() == digest for name, digest in hashes.items())
        assert all(hashlib.sha256(read_bytes(args.prefix.resolve() / name)).hexdigest() == digest for name, digest in artifacts.items())
        receipt["status"] = "passed"
    except BaseException as error:
        receipt.update(status="failed", error=str(error))
        raise
    finally:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
