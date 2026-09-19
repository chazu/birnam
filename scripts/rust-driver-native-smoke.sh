#!/usr/bin/env bash
# Validate a normal installed release prefix and relocated execution.
# With no prefix argument, create an independent cold installation first.
set -euo pipefail

cd "$(dirname "$0")/.."

prefix_parent=$(mktemp -d "${TMPDIR:-/tmp}/birnam-rust-driver-smoke.XXXXXX")
trap 'rm -rf "$prefix_parent"' EXIT
prefix=${1:-"$prefix_parent/prefix"}

if [[ $# == 0 ]]; then
  scripts/rust-install.sh install --root "$PWD" --prefix "$prefix" --profile release
fi

test -x "$prefix/bin/birnam"
test "$(find "$prefix/lib" -maxdepth 1 -type f -name 'libbirnam_id_*.dylib' | wc -l | tr -d ' ')" = 12
test "$(find "$prefix/lib" -maxdepth 1 -type f -name 'libbirnam_*.dylib' | wc -l | tr -d ' ')" = 50

cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v6 -Iscripts/fixtures \
  abi/fixtures/idapi-v6/client.c -o "$prefix_parent/idapi-client"
cc -std=c11 -Wall -Wextra -Werror -fvisibility=hidden -dynamiclib \
  abi/fixtures/idapi-v6/module.c -o "$prefix_parent/idapi-module.dylib"
"$prefix_parent/idapi-client" "$prefix/lib/libbirnam_id.dylib" "$prefix_parent/idapi-module.dylib"

run_root="$prefix_parent/run"
mkdir -p "$run_root"
cp examples/fizzbuzz.birnam "$run_root/fizzbuzz.birnam"
(cd /private/tmp && "$prefix/bin/birnam" "$run_root/fizzbuzz.birnam" </dev/null >"$prefix_parent/fizzbuzz.out")
grep -Fxq 'FizzBuzz' "$prefix_parent/fizzbuzz.out"
mkdir -p "$prefix_parent/home"
(
  cd /private/tmp
  /usr/bin/env -i PATH=/usr/bin:/bin HOME="$prefix_parent/home" \
    "$prefix/bin/birnam" "$run_root/fizzbuzz.birnam" </dev/null >"$prefix_parent/fizzbuzz-clean-env.out"
)
grep -Fxq 'FizzBuzz' "$prefix_parent/fizzbuzz-clean-env.out"

scripts/stackless-image-lifecycle-smoke.sh "$prefix"
scripts/portable-resumable-lifecycle-smoke.sh "$prefix"
scripts/portable-code-binding-lifecycle-smoke.sh "$prefix"
scripts/portable-image-v8-smoke.sh "$prefix"
scripts/stackless-control-smoke.sh "$prefix"
bash scripts/task-control-smoke.sh "$prefix"
bash scripts/multicore-smoke.sh "$prefix"
bash scripts/worker-service-smoke.sh "$prefix"
bash scripts/closure-elision-smoke.sh "$prefix"
cc -Wall -Wextra -Werror -Iabi/fixtures/idapi-v6 scripts/fixtures/closure-api-context.c -o "$prefix_parent/closure-api-context"
"$prefix_parent/closure-api-context" "$prefix/lib/libbirnam_id.dylib" \
  "$prefix/lib/libbirnam_id_tcc.dylib" "$prefix/lib/libbirnam_id_codegen.dylib"
"$prefix/bin/birnam" scripts/fixtures/projection-recovery.birnam >"$prefix_parent/projection.out"
grep -Fxq "production-recovery: portable projections, captures, identity, eviction and GC passed" "$prefix_parent/projection.out"
cat "$prefix_parent/projection.out"
"$prefix/bin/birnam" scripts/fixtures/small-callee-recovery.birnam >"$prefix_parent/small-callee.out"
grep -Fxq "small-callee-recovery: aliases, captures, effects, eviction and GC passed" "$prefix_parent/small-callee.out"
cat "$prefix_parent/small-callee.out"
"$prefix/bin/birnam" scripts/fixtures/native-regions.birnam >"$prefix_parent/native-regions.out"
grep -Fxq "native-regions: copies, argument identity, replacement, eviction and GC passed" "$prefix_parent/native-regions.out"
cat "$prefix_parent/native-regions.out"
"$prefix/bin/birnam" scripts/fixtures/numeric-specialization.birnam >"$prefix_parent/numeric-specialization.out"
grep -Fxq "numeric-specialization: arithmetic, mixed values, errors, identity and retirement passed" "$prefix_parent/numeric-specialization.out"
cat "$prefix_parent/numeric-specialization.out"
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v6 scripts/fixtures/integer-module.c -o "$prefix_parent/integer-module"
"$prefix_parent/integer-module" "$prefix/lib/libbirnam_id.dylib" "$prefix/lib/libbirnam_integer.dylib" \
  "$prefix/lib/libbirnam_id_error.dylib" "$prefix/lib/libbirnam_object_model.dylib" "$prefix/lib/libbirnam_id_prelude.dylib"
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v6 -Iscripts/fixtures scripts/fixtures/float-module.c -o "$prefix_parent/float-module"
"$prefix_parent/float-module" "$prefix/lib/libbirnam_id.dylib" "$prefix/lib/libbirnam_float.dylib" \
  "$prefix/lib/libbirnam_object_model.dylib" "$prefix/lib/libbirnam_id_prelude.dylib" "$prefix/lib/libbirnam_id_error.dylib"
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v6 -Iscripts/fixtures scripts/fixtures/word-module.c -o "$prefix_parent/word-module"
"$prefix_parent/word-module" "$prefix/lib/libbirnam_id.dylib" "$prefix/lib/libbirnam_word.dylib" \
  "$prefix/lib/libbirnam_object_model.dylib" "$prefix/lib/libbirnam_id_prelude.dylib" "$prefix/lib/libbirnam_id_error.dylib"
cc -Iabi/fixtures/idapi-v6 scripts/fixtures/native-numeric-api-context.c -o "$prefix_parent/numeric-api-context"
"$prefix_parent/numeric-api-context" "$prefix/lib/libbirnam_id.dylib" "$prefix/lib/libbirnam_integer.dylib"
cc -Wall -Wextra -Werror -Iabi/fixtures/idapi-v6 scripts/fixtures/native-block-construction.c -o "$prefix_parent/block-construction"
"$prefix_parent/block-construction" "$prefix/lib/libbirnam_id.dylib" \
  "$prefix/lib/libbirnam_id_prelude.dylib" "$prefix/lib/libbirnam_id_collection.dylib" \
  "$prefix/lib/libbirnam_id_error.dylib" "$prefix/lib/libbirnam_portable_code.dylib" \
  abi/fixtures/portable-code-v1/program-explicit-captures.bin
"$prefix_parent/block-construction" "$prefix/lib/libbirnam_id.dylib" \
  "$prefix/lib/libbirnam_id_prelude.dylib" "$prefix/lib/libbirnam_id_collection.dylib" \
  "$prefix/lib/libbirnam_id_error.dylib" "$prefix/lib/libbirnam_portable_code.dylib" \
  abi/fixtures/portable-code-v1/program-explicit-captures.bin without-prelude

python3 - "$prefix/share/birnam/rust-install.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    receipt = json.load(handle)
assert receipt["schema"] == 3
assert receipt["authority"] == "rust"
assert len(receipt["source_manifest_sha256"]) == 64
assert len(receipt["staged_manifest_sha256"]) == 64
assert receipt["admission_contract"]["image_version"] == 8
assert receipt["admission_contract"]["candidate_manifest_version"] == 4
assert len(receipt["admission_contract"]["id_api_fingerprint"]) == 64
assert receipt["publication"]["method"] == "atomic-sibling-rename"
assert receipt["publication"]["durability"] == "fsync-staged-files-and-directories-before-and-after-rename"
assert len(receipt["dynamic_libraries"]) == 18
assert len(receipt["stage1_modules"]) == 28
PY

echo "rust-driver-native-smoke: Rust-only release prefix, relocation, and stage1 closure passed"
