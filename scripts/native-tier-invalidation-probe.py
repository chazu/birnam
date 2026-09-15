#!/usr/bin/env python3
"""74M follow-up: compare existing/new symbol interning after native warmup."""
from measurement_archive import read_bytes

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile-prefix", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    prefix = args.profile_prefix.resolve()
    baseline = (ROOT / "bench/native-tier-steady.birnam").read_text()
    rows = []
    with tempfile.TemporaryDirectory(prefix="birnam-74m-symbol-") as temporary:
        for name, symbol in (("existing", "oopAt:"), ("new", "notPreviouslyInterned74mProbeSymbol")):
            source = baseline.replace("let portable :=", f'let symbolName := "{symbol}";\nlet portable :=')
            source = source.replace("[samples set: #[]];\n[run value];", "[samples set: #[]];\n[symbolName asSymbol];\n[run value];")
            path = Path(temporary) / f"{name}.birnam"
            path.write_text(source)
            env = {key: value for key, value in os.environ.items() if not key.startswith("BIRNAM_DEV_")}
            result = subprocess.run([prefix / "bin/birnam", path], cwd=ROOT, env=env, capture_output=True, text=True, timeout=120)
            assert result.returncode == 0 and "focus-ok steady-chain" in result.stdout, result.stdout + result.stderr
            observations = [json.loads(line.removeprefix("NATIVE_TIER_PROFILE ")) for line in result.stderr.splitlines() if line.startswith("NATIVE_TIER_PROFILE ")]
            assert len(observations) == 1
            rows.append({"case": name, "source": source, "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
                         "profile": observations[0], "stdout": result.stdout, "stderr": result.stderr})
    # Counts locate execution before/after the one interning send. Times from
    # this instrumented follow-up are deliberately not compared.
    assert rows[0]["profile"]["entries"] == 122817, rows[0]["profile"]
    assert rows[1]["profile"]["entries"] == 61377, rows[1]["profile"]
    receipt = {"schema": "birnam-native-tier-symbol-invalidation-v1", "result": "confirmed",
               "scope": "diagnostic entry counts and checked answers; no timing claim",
               "floor_sha256": hashlib.sha256(read_bytes(prefix / "lib/libbirnam_id.dylib")).hexdigest(),
               "runner_sha256": hashlib.sha256(read_bytes(Path(__file__))).hexdigest(), "rows": rows}
    args.output.write_text(json.dumps(receipt, indent=2) + "\n")
    print("native-tier-invalidation: a new interned symbol withdraws the candidate; existing symbol retains it")


if __name__ == "__main__":
    main()
