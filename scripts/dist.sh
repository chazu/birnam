#!/usr/bin/env bash
# Build a portable arm64 macOS release tarball.
#
# Stages the `birnam` siphon + its rung dylibs into a versioned directory,
# ad-hoc signs them on macOS (softens Gatekeeper for a downloaded binary), and
# tars it. `birnam` resolves its dylibs through $BIRNAM_LIB_DIR /
# $BIRNAM_ID_DYLIB (no <exe>/../lib auto-probe), so the bundle ships a tiny
# `bin/birnam` wrapper that points those at the bundle's own lib/ before
# exec'ing the real binary (bin/birnam.bin). The bundle is portable: it runs
# from wherever it's unpacked.
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
prefix="zig-out/_dist-prefix"
stage="zig-out/dist/${name}"

log "building ${name} (ReleaseSafe)"
zig build install -Doptimize=ReleaseSafe --prefix "$prefix"

log "staging"
rm -rf "$stage"
mkdir -p "$stage/bin" "$stage/lib"
cp "$prefix/bin/birnam" "$stage/bin/birnam.bin"
# All supported artifacts are Mach-O dylibs.
cp "$prefix"/lib/libbirnam_*.dylib "$stage/lib/"
# Source-backed tools + guide, in the same prefix-relative layout as install.
cp -R "$prefix/share" "$stage/share"

# Wrapped C libs (built via FFI absorption) into the same lib dir, so alien
# resolves them by bare name in the bundle. Best-effort — they may not be built.
cp wrapped/*/lib*.dylib "$stage/lib/" 2>/dev/null || true

# Launcher: point the dylib search at the bundle's own lib/ (overridable), then
# exec the real binary. Resolves its own directory so it works from anywhere.
cat > "$stage/bin/birnam" <<'WRAP'
#!/usr/bin/env bash
here="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
export BIRNAM_LIB_DIR="${BIRNAM_LIB_DIR:-$here/../lib}"
export BIRNAM_ID_DYLIB="${BIRNAM_ID_DYLIB:-$here/../lib/libbirnam_id.dylib}"
exec "$here/birnam.bin" "$@"
WRAP
chmod +x "$stage/bin/birnam"

log "ad-hoc signing"
codesign -s - --force "$stage"/lib/*.dylib "$stage/bin/birnam.bin"

log "tarball"
tar -C zig-out/dist -czf "zig-out/dist/${name}.tar.gz" "$name"

log "done: zig-out/dist/${name}.tar.gz"
echo "zig-out/dist/${name}.tar.gz"
