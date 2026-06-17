#!/usr/bin/env bash
# Build a portable release tarball for the current platform.
#
# Stages `bin/birnam` + the module dylibs into a versioned directory, ad-hoc
# signs them on macOS (softens Gatekeeper for a downloaded binary), and tars
# it. The bundle is portable: `bin/birnam` finds its modules via the
# `<exe>/../lib` search-path probe, so it runs from wherever it's unpacked.
#
# Prints the tarball path on stdout (progress goes to stderr) so callers can
# capture it: TARBALL="$(scripts/dist.sh)".
set -euo pipefail
cd "$(dirname "$0")/.."

log() { echo "==> $*" >&2; }

VERSION="$(cat VERSION)"

# Detect the build target from the host, normalised to Zig-style names.
arch="$(uname -m)"; case "$arch" in arm64|aarch64) arch=aarch64;; x86_64|amd64) arch=x86_64;; esac
kernel="$(uname -s)"; case "$kernel" in Darwin) os=macos;; Linux) os=linux;; *) os="$kernel";; esac
target="${arch}-${os}"

name="birnam-${VERSION}-${target}"
prefix="zig-out/_dist-prefix"
stage="zig-out/dist/${name}"

log "building ${name} (ReleaseSafe)"
zig build install -Doptimize=ReleaseSafe --prefix "$prefix"

log "staging"
rm -rf "$stage"
mkdir -p "$stage/bin" "$stage/lib"
cp "$prefix/bin/birnam" "$stage/bin/"
# dylibs on macOS, .so on Linux
cp "$prefix"/lib/libbirnam_*.dylib "$stage/lib/" 2>/dev/null \
  || cp "$prefix"/lib/libbirnam_*.so "$stage/lib/"

if [ "$os" = macos ]; then
  log "ad-hoc signing"
  codesign -s - --force "$stage"/lib/*.dylib "$stage/bin/birnam"
fi

log "tarball"
tar -C zig-out/dist -czf "zig-out/dist/${name}.tar.gz" "$name"

log "done: zig-out/dist/${name}.tar.gz"
echo "zig-out/dist/${name}.tar.gz"
