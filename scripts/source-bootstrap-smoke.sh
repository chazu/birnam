#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
scratch=$(mktemp -d /tmp/birnam-source-bootstrap.XXXXXX)
trap 'rm -rf "$scratch"' EXIT

# A git archive is the clean-checkout boundary. Compatibility fixtures are
# deliberately removed: they are old-binary evidence, never bootstrap inputs.
git -C "$repo" archive HEAD | tar -x -C "$scratch"
fixture_dir="$scratch/abi/fixtures/idapi-v1"
case "$fixture_dir" in
  "$scratch"/*) rm -rf "$fixture_dir" ;;
  *) echo "source-bootstrap: refused unsafe fixture path" >&2; exit 1 ;;
esac

prebuilt=$(find "$scratch" -type f \
  \( -name birnam -o -name '*.dylib' -o -name '*.so' -o \
     -name '*.image' -o -name '*.bim' \) -print -quit)
if [[ -n "$prebuilt" ]]; then
  echo "source-bootstrap: prebuilt Birnam artifact present: $prebuilt" >&2
  exit 1
fi

[[ $(zig version) == 0.15.1 ]]
rustc --version | grep -q '^rustc 1\.95\.0 '

(
  cd "$scratch"
  CARGO_TARGET_DIR="$scratch/.cargo-target" \
    cargo check --workspace --locked --offline
  CARGO_TARGET_DIR="$scratch/.cargo-target" \
    cargo test --workspace --locked --offline
  zig build
)

test -x "$scratch/zig-out/bin/birnam"
test -f "$scratch/zig-out/lib/libbirnam_id.dylib"
echo "source-bootstrap: fixture-free Zig/Rust cold build passed"
