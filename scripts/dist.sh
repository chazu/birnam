#!/usr/bin/env bash
# Build a portable release tarball for the current platform.
#
# Stages the `birnam-id` siphon + its rung dylibs into a versioned directory,
# ad-hoc signs them on macOS (softens Gatekeeper for a downloaded binary), and
# tars it. `birnam-id` resolves its dylibs through $BIRNAM_LIB_DIR /
# $BIRNAM_ID_DYLIB (no <exe>/../lib auto-probe), so the bundle ships a tiny
# `bin/birnam-id` wrapper that points those at the bundle's own lib/ before
# exec'ing the real binary (bin/birnam-id.bin). The bundle is portable: it runs
# from wherever it's unpacked.
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
cp "$prefix/bin/birnam-id" "$stage/bin/birnam-id.bin"
# rung dylibs on macOS, .so on Linux
cp "$prefix"/lib/libbirnam_*.dylib "$stage/lib/" 2>/dev/null \
  || cp "$prefix"/lib/libbirnam_*.so "$stage/lib/"

# Wrapped C libs (built via FFI absorption) into the same lib dir, so alien
# resolves them by bare name in the bundle. Best-effort — they may not be built.
cp wrapped/*/lib*.dylib "$stage/lib/" 2>/dev/null || true
cp wrapped/*/lib*.so "$stage/lib/" 2>/dev/null || true

# Launcher: point the dylib search at the bundle's own lib/ (overridable), then
# exec the real binary. Resolves its own directory so it works from anywhere.
cat > "$stage/bin/birnam-id" <<'WRAP'
#!/usr/bin/env bash
here="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
export BIRNAM_LIB_DIR="${BIRNAM_LIB_DIR:-$here/../lib}"
export BIRNAM_ID_DYLIB="${BIRNAM_ID_DYLIB:-$here/../lib/libbirnam_id.dylib}"
exec "$here/birnam-id.bin" "$@"
WRAP
chmod +x "$stage/bin/birnam-id"

if [ "$os" = macos ]; then
  log "ad-hoc signing"
  codesign -s - --force "$stage"/lib/*.dylib "$stage/bin/birnam-id.bin"
fi

log "tarball"
tar -C zig-out/dist -czf "zig-out/dist/${name}.tar.gz" "$name"

log "done: zig-out/dist/${name}.tar.gz"
echo "zig-out/dist/${name}.tar.gz"
