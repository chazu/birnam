#!/usr/bin/env bash
# Build a portable arm64 macOS release tarball.
#
# Stages the signed `birnam` host and native modules without changing receipt
# bytes, verifies their signatures, and tars the exact published closure.
# The real binary resolves the fixed runtime layout relative to itself;
# there is no launcher or external layout manifest. The bundle is portable: it
# runs from wherever it is unpacked.
#
# Prints the tarball path on stdout (progress goes to stderr) so callers can
# capture it: TARBALL="$(scripts/dist.sh)".
set -euo pipefail
cd "$(dirname "$0")/.."

log() { echo "==> $*" >&2; }

VERSION="$(cat VERSION)"

# The build is intentionally fail-fast outside its one supported target. Keep
# this check here too: a release script must never stage a partly-working bundle.
host_arch="$(uname -m)"
host_os="$(uname -s)"
if [ "$host_os" != "Darwin" ] || { [ "$host_arch" != "arm64" ] && [ "$host_arch" != "aarch64" ]; }; then
  echo "error: birnam releases currently require arm64 macOS; got ${host_arch}-${host_os}" >&2
  echo "       Linux and Intel ports need target-specific libtcc and dylib-loader support." >&2
  exit 1
fi
target="aarch64-macos"

name="birnam-${VERSION}-${target}"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
prefix="$work/prefix"
stage="$work/${name}"
archive="$work/${name}.tar.gz"
published="dist/${name}.tar.gz"

log "building ${name} (Rust release, production distribution)"
scripts/rust-install.sh install --root "$PWD" --prefix "$prefix" --profile release >&2

log "staging exact installation"
cp -R "$prefix" "$stage"

# Authored modules are ELF relocatable objects, not Mach-O dylibs, despite the
# .dylib suffix. Only the host and native shared libraries carry OS signatures.
# Preserve all bytes (including explicitly shipped wrappers) from the receipt.
log "verifying native signatures and installed receipt"
python3 - "$stage" <<'PY_SIGNATURES'
from pathlib import Path
import subprocess
import sys
sys.path.insert(0, 'scripts')
from install_contract import verify
prefix = Path(sys.argv[1])
receipt = verify(prefix)
for relative in receipt['staged_manifest']:
    if not relative.startswith(('bin/', 'lib/')):
        continue
    path = prefix / relative
    with path.open('rb') as stream:
        magic = stream.read(4)
    if relative.startswith('lib/') and magic == b'\x7fELF':
        continue
    subprocess.run(['codesign', '--verify', '--strict', path], check=True)
PY_SIGNATURES

log "smoke testing relocated bundle"
BIRNAM_PREFIX="$stage" scripts/install-smoke.sh >&2

log "tarball"
tar -C "$work" -czf "$archive" "$name"
mkdir -p dist
mv "$archive" "$published"

log "done: $published"
echo "$published"
