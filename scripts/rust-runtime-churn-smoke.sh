#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
scratch=$(mktemp -d /tmp/birnam-rust-runtime-churn.XXXXXX)
trap 'rm -rf "$scratch"' EXIT

prefix="$scratch/prefix"
zig_cache="$scratch/zig-cache"
zig_global_cache="$scratch/zig-global-cache"
zig_tmp="$scratch/zig-tmp"
mkdir -p "$prefix" "$zig_cache" "$zig_global_cache" "$zig_tmp"

TMPDIR="$zig_tmp" zig build install \
  -Dfloor=rust \
  -Doptimize=ReleaseSafe \
  --prefix "$prefix" \
  --cache-dir "$zig_cache" \
  --global-cache-dir "$zig_global_cache"

bin="$prefix/bin/birnam"
floor="$prefix/lib/libbirnam_id.dylib"
digest=$(shasum -a 256 "$floor" | awk '{print $1}')
export BIRNAM_LIB_DIR="$prefix/lib"
export BIRNAM_ID_DYLIB="$floor"
export BIRNAM_ID_FLOOR_IDENTITY=rust-id-v1
export BIRNAM_ID_FLOOR_SHA256="$digest"

module_churn="$scratch/module-churn.birnam"
printf '%s\n' \
  'let failures := [0 asCell];' \
  '[1000 timesRepeat: [|' \
  '  let loaded := [Module load: #time];' \
  '  [[loaded isError] ifTrue: [| [failures inc]] ifFalse: [| nil]]' \
  ']];' \
  '[System println: [failures value]];' \
  '[System println: [Module isLoaded: #time]]' >"$module_churn"
module_output=$("$bin" "$module_churn")
if [[ $module_output != $'0\ntrue' ]]; then
  printf 'rust-runtime-churn: repeated module load failed:\n%s\n' "$module_output" >&2
  exit 1
fi
echo "rust-runtime-churn: 1000 repeated module loads passed"

image="$scratch/churn.image"
image_source="$scratch/save-image.birnam"
printf '%s\n' \
  "[System load: \"$repo/scripts/isolate-fixture-module.birnam\"];" \
  '[DispatchGeneration initializeCurrent];' \
  "[System saveImage: \"$image\"]" >"$image_source"
"$bin" "$image_source" >/dev/null
test -s "$image"

for iteration in $(seq 1 16); do
  printf ':quit\n' | "$bin" "$image" >/dev/null
  printf ':quit\n' | "$bin" --rebuild "$image" >/dev/null
done
echo "rust-runtime-churn: 16 mapped and 16 rebuilt image boots passed"

isolate_source="$scratch/isolate-cycle.birnam"
printf '%s\n' \
  "let worker := [Isolate start: #IsolateFixture selector: #handle: image: \"$image\"];" \
  'let answer := [worker request: 42 timeout: 5000];' \
  '[worker stop];' \
  '[System println: answer]' >"$isolate_source"

for iteration in $(seq 1 32); do
  output=$("$bin" "$isolate_source")
  if [[ $output != 42 ]]; then
    echo "rust-runtime-churn: isolate cycle $iteration returned $output" >&2
    exit 1
  fi
done
echo "rust-runtime-churn: 32 isolate start/request/stop cycles passed"
echo "rust-runtime-churn: release floor $digest passed"
