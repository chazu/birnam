#!/usr/bin/env bash
# Verify one selected native-rung artifact before its tests run, then emit a
# machine-readable receipt. Identity comes from the explicit implementation
# filename; content identity comes from the selected and explicit dylib hashes.
set -euo pipefail

if [ "$#" -ne 4 ]; then
  echo "usage: $0 <rung> <zig|rust> <canonical-dylib> <explicit-dylib>" >&2
  exit 2
fi

rung=$1
implementation=$2
canonical=$3
explicit=$4

case "$implementation" in
  zig|rust) ;;
  *)
    echo "native-rung-selection: unknown implementation: $implementation" >&2
    exit 1
    ;;
esac

expected_explicit="$(basename "${canonical%.dylib}")_${implementation}.dylib"
if [ "$(basename "$explicit")" != "$expected_explicit" ]; then
  echo "native-rung-selection: implementation identity mismatch" >&2
  echo "  expected explicit artifact: $expected_explicit" >&2
  echo "  actual explicit artifact:   $(basename "$explicit")" >&2
  exit 1
fi

for artifact in "$canonical" "$explicit"; do
  if [ ! -f "$artifact" ]; then
    echo "native-rung-selection: missing artifact: $artifact" >&2
    exit 1
  fi
done

canonical_digest=$(shasum -a 256 "$canonical" | awk '{print $1}')
explicit_digest=$(shasum -a 256 "$explicit" | awk '{print $1}')
if [ "$canonical_digest" != "$explicit_digest" ]; then
  echo "native-rung-selection: selected artifact digest mismatch" >&2
  echo "  canonical: $canonical_digest" >&2
  echo "  explicit:  $explicit_digest" >&2
  exit 1
fi

printf '{"schema":1,"rung":"%s","implementation":"%s","sha256":"%s"}\n' \
  "$rung" "$implementation" "$canonical_digest"
