#!/usr/bin/env bash
# Compile an installable Birnam module with the selected TCC provider, then
# activate it from a fresh process. The second process proves the emitted dylib
# does not rely on the authoring process's TCC state or workspace paths.
set -euo pipefail

bin=${BIRNAM_BIN:?BIRNAM_BIN must point to birnam}
lib_dir=${BIRNAM_LIB_DIR:?BIRNAM_LIB_DIR must point to the installed lib dir}
tcc=${BIRNAM_TCC_DYLIB:-}
if [[ -z "$tcc" ]]; then
  tcc=${BIRNAM_RUST_TCC_DYLIB:-}
fi
: "${tcc:?BIRNAM_TCC_DYLIB or BIRNAM_RUST_TCC_DYLIB must point to TCC}"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
src="$tmp/tcc-smoke.birnam"
artifact="$tmp/libtcc-smoke.dylib"
work="$tmp/child"
mkdir -p "$work"

# Nested rewriting must preserve binary embedded bytes, including NUL and 0xff.
# Both source-directory and context-free roots contain the same input here.
printf 'embedded\000bytes\377\n' >"$tmp/expected.dat"
cp "$tmp/expected.dat" "$tmp/asset.dat"
cp "$tmp/expected.dat" "$work/asset.dat"
cat >"$src" <<EOF
[Module named: #tccSmoke requires: #[]];
[System writeFile: "$work/restored.dat" contents: [[| [System embedFile: "asset.dat"]] value]];
EOF

(
  cd "$work"
  BIRNAM_LIB_DIR="$lib_dir" \
    BIRNAM_TCC_DYLIB="$tcc" \
    "$bin" --compile-module "$src" "$artifact"
)
test -s "$artifact"
rm "$tmp/asset.dat" "$work/asset.dat"

(
  cd "$work"
  BIRNAM_LIB_DIR="$lib_dir" \
    BIRNAM_TCC_DYLIB="$tcc" \
    "$bin" "$artifact" >/dev/null
)

cmp "$tmp/expected.dat" "$work/restored.dat"
echo "rust-tcc-artifact: separate-process AOT activation and binary embedding passed"

# Code and descriptor metadata must use the same parse, while each later
# compilation observes the current parser again.
cat >"$tmp/parse-count.birnam" <<EOF
[Globals at: #ArtifactOriginalParse put: [Lang methodFor: #parse:]];
[Globals at: #ArtifactParseCount put: 0];
[Reflect defineMethod: #parse: on: Lang as: [:self :source |
  [Globals at: #ArtifactParseCount put: [[Globals at: #ArtifactParseCount] + 1]];
  [[Globals at: #ArtifactOriginalParse] invokeOn: self arguments: #[source]]
]];
[Codegen compile: "[Module named: #parseCount requires: #[]];" output: "$tmp/parse-count.dylib"];
[System println: [Globals at: #ArtifactParseCount]];
[Codegen compile: "[Module named: #parseCount requires: #[]];" output: "$tmp/parse-count.dylib"];
[System println: [Globals at: #ArtifactParseCount]];
[Reflect defineMethod: #parse: on: Lang as: [:self :source | nil]];
[System println: [[Codegen compile: "[Module named: #rejected requires: #[]];" output: "$tmp/rejected.dylib"] isNil]]
EOF
status=0
BIRNAM_LIB_DIR="$lib_dir" BIRNAM_TCC_DYLIB="$tcc" \
  "$bin" "$tmp/parse-count.birnam" >"$tmp/parse-count.out" 2>"$tmp/parse-count.err" || status=$?
test "$status" -eq 1
printf '1\n2\ntrue\n' >"$tmp/parse-count.expected"
cmp "$tmp/parse-count.expected" "$tmp/parse-count.out"
printf 'birnam: script: EvaluationRejected\n' >"$tmp/parse-count-error.expected"
cmp "$tmp/parse-count-error.expected" "$tmp/parse-count.err"
test -s "$tmp/parse-count.dylib"
test ! -e "$tmp/rejected.dylib"
echo "rust-tcc-artifact: one parse per compilation and live parser replacement passed"
