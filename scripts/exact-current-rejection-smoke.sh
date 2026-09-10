#!/usr/bin/env bash
# Prove packaged image admission rejects obsolete versions at the fixed header.
set -euo pipefail

cd "$(dirname "$0")/.."
prefix=${1:?usage: exact-current-rejection-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
test -x "$bin"
root=$(mktemp -d "${TMPDIR:-/tmp}/birnam-exact-current.XXXXXX")
trap 'rm -rf "$root"' EXIT
current="$root/current.image"
printf '[System saveImage: "%s"];\n:quit\n' "$current" | "$bin" --repl >"$root/save.out" 2>"$root/save.err"
test ! -s "$root/save.err"
test -s "$current"

for version in 4 5 6 7 9; do
  rejected="$root/version-${version}.image"
  cp "$current" "$rejected"
  python3 - "$rejected" "$version" <<'PY'
import struct
import sys
path, version = sys.argv[1], int(sys.argv[2])
with open(path, "r+b") as image:
    image.seek(8)
    image.write(struct.pack("<I", version))
PY
  if printf ':quit\n' | "$bin" "$rejected" >"$root/version-${version}.out" 2>"$root/version-${version}.err"; then
    printf 'obsolete image version %s was accepted\n' "$version" >&2
    exit 1
  fi
  grep -Fq "VersionMismatch { expected: 8, observed: $version }" "$root/version-${version}.err"
done

# Runtime ABI identity (offset 20): an image saved under another runtime ABI
# is rejected by name before load (Design 74E §3.1).
abi="$root/runtime-abi.image"
cp "$current" "$abi"
python3 - "$abi" <<'PY'
import struct
import sys
with open(sys.argv[1], "r+b") as image:
    image.seek(20)
    image.write(struct.pack("<Q", 0))
PY
if printf ':quit\n' | "$bin" "$abi" >"$root/abi.out" 2>"$root/abi.err"; then
  printf 'image with a foreign runtime ABI hash was accepted\n' >&2
  exit 1
fi
grep -Fq 'RuntimeAbiMismatch {' "$root/abi.err"
grep -Fq 'observed: 0 }' "$root/abi.err"

invalid="$root/invalid-magic.image"
cp "$current" "$invalid"
printf X | dd of="$invalid" bs=1 seek=0 conv=notrunc status=none
if printf ':quit\n' | "$bin" "$invalid" >"$root/invalid.out" 2>"$root/invalid.err"; then
  printf 'invalid image header was accepted\n' >&2
  exit 1
fi
grep -Fq 'InvalidHeader' "$root/invalid.err"

printf 'exact-current-rejection-smoke: v4, v5, v6, v7, future, foreign-runtime-ABI, and invalid images rejected before load\n'
