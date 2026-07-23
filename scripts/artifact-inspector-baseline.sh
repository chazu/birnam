#!/usr/bin/env bash
# Capture two clean-process ReleaseSafe artifact-inspection measurements.
#
# The acceptance limits are declared before measurement: elapsed time may vary
# by at most the Design 46 micro threshold (10%), and peak RSS by at most 5%.
# The script prints one machine-readable baseline document to stdout.
set -euo pipefail

cd "$(dirname "$0")/.."

scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-artifact-baseline.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
touch "$scratch/.metadata_never_index"

prefix="$scratch/prefix"
cache="$scratch/cache"
global_cache="$scratch/global-cache"
zig_tmp="$scratch/tmp"
mkdir -p "$prefix" "$cache" "$global_cache" "$zig_tmp"

TMPDIR="$zig_tmp" zig build install artifact-inspector-benchmark \
  -Doptimize=ReleaseSafe \
  -Dfloor=zig \
  -Dartifact-inspector-impl=zig \
  --prefix "$prefix" \
  --cache-dir "$cache" \
  --global-cache-dir "$global_cache"

floor_dylib="$prefix/lib/libbirnam_id.dylib"
rung_dylib="$prefix/lib/libbirnam_id_artifact.dylib"
explicit_rung="$prefix/lib/libbirnam_id_artifact_zig.dylib"
probe="$prefix/bin/artifact-inspector-benchmark"
fixture="$scratch/benchmark-candidate.dylib"

python3 - "$fixture" <<'PY'
import struct
import sys

fields = [
    b"module",
    b"benchmark-candidate",
    b"birnam-artifact-generation-v1",
    b"aarch64-macos",
    b"source-digest",
    b"revision-digest",
]
payload = b"".join(struct.pack("<I", len(value)) + value for value in fields)
payload += struct.pack("<I", 0)
payload += struct.pack("<I", 0)
payload += struct.pack("<I", 1) + struct.pack("<I", len(b"activate")) + b"activate"
with open(sys.argv[1], "wb") as output:
    output.write(b"native-rung baseline fixture\n")
    output.write(b"BIRNAM-ARTIFACT-DESCRIPTOR-V1")
    output.write(struct.pack("<I", len(payload)))
    output.write(payload)
PY

floor_digest=$(shasum -a 256 "$floor_dylib" | awk '{print $1}')
rung_digest=$(shasum -a 256 "$rung_dylib" | awk '{print $1}')
explicit_digest=$(shasum -a 256 "$explicit_rung" | awk '{print $1}')
test "$rung_digest" = "$explicit_digest"

for run in 1 2; do
  scripts/bench-host-check.sh --wait "artifact-inspector baseline run $run" >&2
  json="$scratch/run-$run.json"
  timing="$scratch/run-$run.time"
  /usr/bin/time -l env \
    BIRNAM_ID_DYLIB="$floor_dylib" \
    BIRNAM_ID_FLOOR_IDENTITY=zig-id-v1 \
    BIRNAM_ID_FLOOR_SHA256="$floor_digest" \
    BIRNAM_NATIVE_RUNG=artifact-inspector \
    BIRNAM_NATIVE_RUNG_IMPLEMENTATION=zig \
    BIRNAM_NATIVE_RUNG_DYLIB="$rung_dylib" \
    BIRNAM_NATIVE_RUNG_EXPLICIT_DYLIB="$explicit_rung" \
    "$probe" "$fixture" 5000 >"$json" 2>"$timing"
  rss=$(awk '/maximum resident set size/{print $1}' "$timing")
  test -n "$rss"
  python3 - "$json" "$rss" <<'PY'
import json
import sys
path, rss = sys.argv[1], int(sys.argv[2])
with open(path) as source:
    result = json.load(source)
result["peak_rss_bytes"] = rss
with open(path, "w") as output:
    json.dump(result, output, separators=(",", ":"))
    output.write("\n")
PY
done

python3 - \
  "$floor_digest" \
  "$rung_digest" \
  "$(stat -f %z "$rung_dylib")" \
  "$scratch/run-1.json" \
  "$scratch/run-2.json" <<'PY'
import datetime
import json
import platform
import sys

floor_digest, rung_digest, rung_size, first_path, second_path = sys.argv[1:]
with open(first_path) as source:
    first = json.load(source)
with open(second_path) as source:
    second = json.load(source)

def delta(a, b):
    return abs(b - a) / a if a else 0.0

time_delta = delta(first["ns_per_inspection"], second["ns_per_inspection"])
rss_delta = delta(first["peak_rss_bytes"], second["peak_rss_bytes"])
if time_delta > 0.10:
    raise SystemExit(f"artifact baseline time variance {time_delta:.1%} exceeds 10%")
if rss_delta > 0.05:
    raise SystemExit(f"artifact baseline RSS variance {rss_delta:.1%} exceeds 5%")

document = {
    "schema": 1,
    "purpose": "Design 62 N0.5 artifact-inspector ReleaseSafe comparator",
    "recorded_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "machine": platform.machine(),
    "mode": "ReleaseSafe",
    "floor": {
        "implementation": "zig",
        "identity": "zig-id-v1",
        "sha256": floor_digest,
    },
    "rung": {
        "name": "artifact-inspector",
        "implementation": "zig",
        "sha256": rung_digest,
        "dylib_bytes": int(rung_size),
    },
    "acceptance": {
        "time_variance_limit_percent": 10.0,
        "rss_variance_limit_percent": 5.0,
    },
    "runs": [first, second],
    "observed": {
        "time_variance_percent": round(time_delta * 100, 3),
        "rss_variance_percent": round(rss_delta * 100, 3),
    },
}
json.dump(document, sys.stdout, indent=2)
sys.stdout.write("\n")
PY
