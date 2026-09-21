#!/usr/bin/env bash
# Exercise production image startup with the source compiler/evaluator disabled.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: compiled-image-startup-smoke.sh PREFIX}
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-compiled-image.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$scratch"' EXIT
else
  printf 'compiled image smoke root: %s\n' "$scratch" >&2
fi
cat >"$scratch/app.birnam" <<SOURCE
let ImageCompiledExample := [Object clone];
[Reflect defineMethod: #answer on: ImageCompiledExample as: [:self | 42]];
let banner := "compiled image entry";
let main := [| [System println: banner]; [System println: [ImageCompiledExample answer]]];
[System saveImage: "$scratch/app.image"]
SOURCE
"$prefix/bin/birnam" "$scratch/app.birnam" >"$scratch/save.out" 2>"$scratch/save.err"
test ! -s "$scratch/save.err"
test -s "$scratch/app.image"
rm "$scratch/app.birnam"
cache_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
mise exec -- cargo build --offline --locked -p birnam-host
mise exec -- rustc --edition=2024 -Cpanic=abort scripts/fixtures/compiled-image-host.rs --extern "birnam_host=$cache_target/debug/libbirnam_host.rlib" --extern "birnam_abi=$cache_target/debug/libbirnam_abi.rlib" -L "dependency=$cache_target/debug/deps" -o "$scratch/host"
printf 'compiled image entry\n42\n' >"$scratch/expected"
for mode in mapped rebuild baked; do
  artifact="$scratch/app.image"
  if [[ $mode == baked ]]; then
    artifact="$scratch/baked-app"
    "$prefix/bin/birnam" --bake "$scratch/app.image" "$artifact"
  fi
  "$scratch/host" "$prefix" "$artifact" "$mode" >"$scratch/$mode.out" 2>"$scratch/$mode.err"
  test ! -s "$scratch/$mode.err"
  cmp "$scratch/expected" "$scratch/$mode.out"
done
python3 scripts/fixtures/compiled-image-records.py "$scratch/app.image"
"$scratch/host" "$prefix" "$scratch/missing.image" missing >"$scratch/missing.out" 2>"$scratch/missing.err"
test ! -s "$scratch/missing.err"
cmp "$scratch/expected" "$scratch/missing.out"
for rejected in bad-manifest bad-size trailing; do
  "$scratch/host" "$prefix" "$scratch/$rejected.image" rejected >"$scratch/$rejected.out" 2>"$scratch/$rejected.err"
  test ! -s "$scratch/$rejected.out"
  test ! -s "$scratch/$rejected.err"
done
printf 'compiled image startup: mapped/baked without compilation; explicit/missing-code recovery; malformed code rejection passed\n'
