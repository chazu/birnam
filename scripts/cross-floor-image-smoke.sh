#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
scratch=$(mktemp -d /tmp/birnam-cross-floor-image.XXXXXX)
trap 'rm -rf "$scratch"' EXIT

prefix="$scratch/prefix"
zig_cache="$scratch/zig-cache"
zig_global_cache="$scratch/zig-global-cache"
zig_tmp="$scratch/zig-tmp"
cargo_target="$scratch/cargo-target"
mkdir -p "$prefix" "$zig_cache" "$zig_global_cache" "$zig_tmp"

TMPDIR="$zig_tmp" zig build install \
  --prefix "$prefix" \
  --cache-dir "$zig_cache" \
  --global-cache-dir "$zig_global_cache"
CARGO_TARGET_DIR="$cargo_target" \
  cargo build --workspace --locked --offline --release

bin="$prefix/bin/birnam"
zig_floor="$prefix/lib/libbirnam_id.dylib"
rust_floor="$cargo_target/release/libbirnam_id.dylib"
test -x "$bin"
test -f "$zig_floor"
test -f "$rust_floor"

zig_digest=$(shasum -a 256 "$zig_floor" | awk '{print $1}')
rust_digest=$(shasum -a 256 "$rust_floor" | awk '{print $1}')

run_with_floor() {
  local floor=$1
  local identity=$2
  local digest=$3
  shift 3
  BIRNAM_LIB_DIR="$prefix/lib" \
    BIRNAM_ID_DYLIB="$floor" \
    BIRNAM_ID_FLOOR_IDENTITY="$identity" \
    BIRNAM_ID_FLOOR_SHA256="$digest" \
    "$bin" "$@"
}

write_save_source() {
  local path=$1
  local image=$2
  printf '%s\n' \
    'let CrossFloorBase := 40;' \
    'let CrossFloorPerson := [Object clone];' \
    '[db assert: CrossFloorPerson attr: #name value: "Ada"];' \
    '[db assert: CrossFloorPerson attr: #city value: #Boston];' \
    'let CrossFloorVector := #[CrossFloorBase CrossFloorPerson String];' \
    'let CrossFloorCanonical := String;' \
    'let CrossFloorClosure := [:x | [x + CrossFloorBase]];' \
    'let CrossFloorBehavior := [Object clone];' \
    '[Reflect defineMethod: #answer on: CrossFloorBehavior as: [:self | [CrossFloorClosure value: 2]]];' \
    'let main := [|' \
    '  [System println: CrossFloorBase];' \
    '  [System println: [CrossFloorClosure value: 2]];' \
    '  [System println: [CrossFloorBehavior answer]];' \
    '  [System println: [CrossFloorCanonical == String]];' \
    '  [System println: [[CrossFloorVector at: 1] == CrossFloorPerson]];' \
    '  [System println: [[Reflect factsOf: CrossFloorPerson] size]];' \
    '  [System println: [db currentValue: CrossFloorPerson attr: #name]];' \
    '  [System println: [db currentValue: [Reflect vtableOf: Object] attr: #clone]]' \
    '];' \
    "[System saveImage: \"$image\"];" \
    '[System println: [[System imageSaveReport] status]];' \
    '[System println: [[System imageSaveReport] savedFacts]];' \
    '[System println: [[System imageSaveReport] recoveredFacts]];' \
    '[System println: [[System imageSaveReport] recoveredSlots]];' \
    '[System println: [[System imageSaveReport] rejectedFacts]];' \
    '[System println: [[System imageSaveReport] rejectedSlots]]' >"$path"
}

expected_boot=$'40\n42\n42\ntrue\ntrue\n2\nAda\n#primitive'
zig_image="$scratch/zig.image"
rust_image="$scratch/rust.image"
zig_source="$scratch/save-zig.birnam"
rust_source="$scratch/save-rust.birnam"
write_save_source "$zig_source" "$zig_image"
write_save_source "$rust_source" "$rust_image"

zig_report=$(run_with_floor "$zig_floor" zig-id-v1 "$zig_digest" "$zig_source")
rust_report=$(run_with_floor "$rust_floor" rust-id-v1 "$rust_digest" "$rust_source")
test -s "$zig_image"
test -s "$rust_image"

if [[ $zig_report != "$rust_report" ]]; then
  printf 'cross-floor-image: save reports differ\n--- zig ---\n%s\n--- rust ---\n%s\n' \
    "$zig_report" "$rust_report" >&2
  exit 1
fi
mapfile -t report_lines <<<"$zig_report"
if [[ ${report_lines[0]:-} != '#saved' || ${report_lines[4]:-} != 0 || ${report_lines[5]:-} != 0 ]]; then
  printf 'cross-floor-image: unexpected save report:\n%s\n' "$zig_report" >&2
  exit 1
fi
if (( ${report_lines[1]:-0} <= 0 || ${report_lines[2]:-0} <= 0 )); then
  printf 'cross-floor-image: save did not record durable and recovered facts:\n%s\n' "$zig_report" >&2
  exit 1
fi
echo "cross-floor-image: matching save report $zig_report"

for producer in zig rust; do
  image_var="${producer}_image"
  image=${!image_var}
  for consumer in zig rust; do
    floor_var="${consumer}_floor"
    digest_var="${consumer}_digest"
    floor=${!floor_var}
    digest=${!digest_var}
    identity="${consumer}-id-v1"
    for boot_mode in mapped rebuild; do
      args=("$image")
      if [[ $boot_mode == rebuild ]]; then args=(--rebuild "$image"); fi
      output=$(run_with_floor "$floor" "$identity" "$digest" "${args[@]}")
      if [[ $output != "$expected_boot" ]]; then
        printf 'cross-floor-image: %s save -> %s %s boot returned:\n%s\n' \
          "$producer" "$consumer" "$boot_mode" "$output" >&2
        exit 1
      fi
      echo "cross-floor-image: $producer save -> $consumer $boot_mode boot passed"
    done
  done
done

for consumer in zig rust; do
  floor_var="${consumer}_floor"
  digest_var="${consumer}_digest"
  floor=${!floor_var}
  digest=${!digest_var}
  BIRNAM_BIN="$bin" \
    BIRNAM_LIB_DIR="$prefix/lib" \
    BIRNAM_ID_DYLIB="$floor" \
    BIRNAM_ID_FLOOR_IDENTITY="${consumer}-id-v1" \
    BIRNAM_ID_FLOOR_SHA256="$digest" \
    bash "$repo/abi/fixtures/idapi-v1/verify.sh"
  echo "cross-floor-image: frozen pre-rewrite image passed under $consumer"
done

echo "cross-floor-image: all mapped, rebuild, and frozen compatibility cells passed"
