#!/usr/bin/env bash
# Build a portable arm64 macOS release tarball.
#
# Stages the `birnam` siphon + its rung dylibs into a versioned directory,
# ad-hoc signs them on macOS (softens Gatekeeper for a downloaded binary), and
# tars it. The real binary resolves the fixed runtime layout relative to itself;
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
published="zig-out/dist/${name}.tar.gz"

log "building ${name} (ReleaseSafe)"
zig build install -Doptimize=ReleaseSafe --prefix "$prefix"

log "staging"
mkdir -p "$stage/bin" "$stage/lib"
cp "$prefix/bin/birnam" "$stage/bin/birnam"
# Canonical Rust installs carry the Zig host as an explicit rollback/oracle
# executable. Keep it in the portable bundle when present; Zig-selected
# rollback builds may legitimately have no second executable.
if [ -x "$prefix/bin/birnam-zig-oracle" ]; then
  cp "$prefix/bin/birnam-zig-oracle" "$stage/bin/birnam-zig-oracle"
fi
# All supported artifacts are Mach-O dylibs.
cp "$prefix"/lib/libbirnam_*.dylib "$stage/lib/"
# Native Rust-workspace/Zig-oracle source documentation + guides, in the
# install-relative layout.
cp -R "$prefix/share" "$stage/share"

# Optional wrappers are not copied opportunistically from developer build dirt.
# A deliberately published wrapper is a named self-describing module plus its
# native sibling. Project-owned [[wrapped]] outputs are selected from the local
# build receipt, then installed without that receipt.

log "ad-hoc signing"
codesign -s - --force "$stage"/lib/*.dylib "$stage"/bin/*
for artifact in "$stage"/lib/*.dylib "$stage"/bin/*; do
  codesign --verify --strict "$artifact"
done

log "smoke testing relocated bundle"
BIRNAM_PREFIX="$stage" scripts/install-smoke.sh >&2

log "tarball"
tar -C "$work" -czf "$archive" "$name"
mkdir -p zig-out/dist
mv "$archive" "$published"

log "done: $published"
echo "$published"
