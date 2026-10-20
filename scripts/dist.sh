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

prefix=
output=
while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix) prefix=${2:?--prefix requires a directory}; shift 2 ;;
    --output) output=${2:?--output requires an archive path}; shift 2 ;;
    *) echo "usage: $0 [--prefix cold-release-prefix] [--output archive.tar.gz]" >&2; exit 2 ;;
  esac
done
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

stage="$work/${name}"
archive="$work/${name}.tar.gz"
published="${output:-dist/${name}.tar.gz}"

if [[ -z "$prefix" ]]; then
  prefix="$work/prefix"
  log "building ${name} (Rust release, production distribution)"
  scripts/rust-install.sh install --root "$PWD" --prefix "$prefix" --profile release >&2
fi

# Repeated publication attempts preserve an already-packaged identical archive,
# including its gzip bytes, so its passed acceptance receipt remains reusable.
if python3 - "$prefix" "$published" "$VERSION" <<'PY_REUSE'
from pathlib import Path
import sys, tempfile
sys.path.insert(0, 'scripts')
from install_contract import verify
from release_archive import extract_candidate, verify_signatures
prefix, archive = map(Path, sys.argv[1:3])
receipt = verify(prefix)
if receipt['profile'] != 'release' or receipt['build_mode'] != 'cold':
    raise ValueError('distribution requires a cold release candidate')
if (prefix / 'share/birnam/src/VERSION').read_text().strip() != sys.argv[3]:
    raise ValueError('candidate and requested distribution versions differ')
verify_signatures(prefix, receipt)
if archive.is_file():
    try:
        with tempfile.TemporaryDirectory() as temporary:
            if verify(extract_candidate(archive, Path(temporary))) == receipt:
                sys.exit(0)
    except (ValueError, OSError):
        pass
sys.exit(3)
PY_REUSE
then
  log "reusing identical packaged candidate"
  echo "$published"
  exit 0
else
  status=$?
  [[ $status == 3 ]] || exit "$status"
fi

log "staging exact installation"
cp -R "$prefix" "$stage"
python3 scripts/install_contract.py "$stage" >&2

# Full qualification belongs to validate --candidate; archive acceptance checks
# relocation. Packaging neither rebuilds supplied artifacts nor runs that matrix.
log "tarball"
# Extended attributes are not manifest files. Do not synthesize AppleDouble
# sidecars from Spotlight/Finder metadata attached to ordinary source files.
COPYFILE_DISABLE=1 tar -C "$work" -czf "$archive" "$name"
mkdir -p "$(dirname "$published")"
mv "$archive" "$published"

log "done: $published"
echo "$published"
