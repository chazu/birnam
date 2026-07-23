#!/usr/bin/env bash
# Exercise the generic receipt verifier and the artifact-inspector build
# selector's unavailable/unknown implementation failures.
set -euo pipefail

cd "$(dirname "$0")/.."

tmp=$(mktemp -d "${TMPDIR:-/tmp}/birnam-native-rung-selection.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

printf 'zig artifact implementation\n' >"$tmp/libbirnam_id_artifact_zig.dylib"
cp "$tmp/libbirnam_id_artifact_zig.dylib" "$tmp/libbirnam_id_artifact.dylib"

receipt=$(
  ./scripts/native-rung-selection.sh \
    artifact-inspector \
    zig \
    "$tmp/libbirnam_id_artifact.dylib" \
    "$tmp/libbirnam_id_artifact_zig.dylib"
)
case "$receipt" in
  '{"schema":1,"rung":"artifact-inspector","implementation":"zig","sha256":"'*) ;;
  *)
    echo "native-rung-selection-smoke: malformed receipt: $receipt" >&2
    exit 1
    ;;
esac

if ./scripts/native-rung-selection.sh \
  artifact-inspector \
  rust \
  "$tmp/libbirnam_id_artifact.dylib" \
  "$tmp/libbirnam_id_artifact_zig.dylib" >"$tmp/mismatch.out" 2>&1
then
  echo "native-rung-selection-smoke: mismatched implementation was accepted" >&2
  exit 1
fi
grep -q 'implementation identity mismatch' "$tmp/mismatch.out"

if ./scripts/native-rung-selection.sh \
  artifact-inspector \
  other \
  "$tmp/libbirnam_id_artifact.dylib" \
  "$tmp/libbirnam_id_artifact_zig.dylib" >"$tmp/unknown.out" 2>&1
then
  echo "native-rung-selection-smoke: unknown implementation was accepted" >&2
  exit 1
fi
grep -q 'unknown implementation' "$tmp/unknown.out"

if zig build test-artifact-selection \
  -Dartifact-inspector-impl=rust \
  --cache-dir "$tmp/rust-cache" \
  --global-cache-dir "$tmp/rust-global-cache" \
  --prefix "$tmp/rust-prefix" >"$tmp/rust-build.out" 2>&1
then
  echo "native-rung-selection-smoke: unavailable Rust implementation built" >&2
  exit 1
fi
grep -q "implementation 'rust' is not available yet" "$tmp/rust-build.out"

if zig build test-artifact-selection \
  -Dartifact-inspector-impl=other \
  --cache-dir "$tmp/unknown-cache" \
  --global-cache-dir "$tmp/unknown-global-cache" \
  --prefix "$tmp/unknown-prefix" >"$tmp/unknown-build.out" 2>&1
then
  echo "native-rung-selection-smoke: unknown build implementation was accepted" >&2
  exit 1
fi
grep -q 'artifact-inspector-impl' "$tmp/unknown-build.out"

echo "native-rung-selection-smoke: receipts and fail-closed choices passed"
