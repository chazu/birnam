#!/usr/bin/env bash
# Prove selected named, method-owned BlockBody, and projected nonmethod-owned
# BlockBody entries survive activation and image publication as one generation
# authority on mapped and rebuilt recovery paths. The successor is delivered
# through candidate AOT and proves projected initializer rollback.
set -euo pipefail

cd "$(dirname "$0")/.."

prefix=${1:?usage: stackless-image-lifecycle-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
lib="$prefix/lib"
test -x "$bin"

smoke_root=$(mktemp -d "${TMPDIR:-/tmp}/birnam-stackless-image-lifecycle.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$smoke_root"' EXIT
else
  printf 'stackless smoke root: %s\n' "$smoke_root" >&2
fi

image="$smoke_root/selected.image"
candidate="$smoke_root/selected-candidate.dylib"
save_script="$smoke_root/save.birnam"

cat >"$save_script" <<EOF
[Module load: #compiledUnit];
let source := "[Module named: #stacklessImageSmoke requires: #[]]; let stacklessProjectedClosure := [| 99]; [Reflect defineMethod: #stacklessImageSmokeProbe on: Integer as: [:self | [self + 2]]]";
let baseline := "[Module named: #stacklessImageSmoke requires: #[]]; let stacklessProjectedClosure := [| 97]; [Reflect defineMethod: #stacklessImageSmokeProbe on: Integer as: [:self | [self + 1]]]";
[Codegen recordModuleSource: baseline named: #stacklessImageSmoke];
[DispatchGeneration initializeCurrent];
[Globals at: #stacklessProjectedClosure put: [| 98]];
let revision := [Codegen sourceRevisionOf: source];
let digest := [revision at: 6];
let initializerRow := #[#implementationDisposition digest #nonMethodProgram 0 #[0] #blockBody #resumableRequired];
let adapterRow := #[#implementationDisposition digest #methodOperation 0 #[] #namedAdapter #resumableRequired];
let implementationRow := #[#implementationDisposition digest #methodOperation 0 #[0] #blockBody #resumableRequired];
let selection := #[initializerRow adapterRow implementationRow];
[CompiledUnit compile: source named: #stacklessImageSmoke output: "$candidate" selection: selection];
let unit := [CompiledUnit open: "$candidate"];
[unit map];
[[unit activate] fulfill];
[db assert: #stacklessImageSmokeAcceptance attr: #origin value: unit];
[System println: [stacklessProjectedClosure value]];
[System println: [40 stacklessImageSmokeProbe]];
let saved := [System saveImage: "$image"];
[System println: [[saved isNil] printString]];
[System println: [[System imageSaveReport] rejectedFacts]]
EOF

BIRNAM_LIB_DIR="$lib" "$bin" "$save_script" >"$smoke_root/save.out"
grep -Fxq '99' "$smoke_root/save.out"
grep -Fxq '42' "$smoke_root/save.out"
grep -Fxq 'false' "$smoke_root/save.out"
grep -Fxq '0' "$smoke_root/save.out"
nm -j "$candidate" >"$smoke_root/candidate.symbols"
grep -Eq '__birnam_resumable_step_0$' "$smoke_root/candidate.symbols"
grep -Eq '__birnam_resumable_step_1$' "$smoke_root/candidate.symbols"
grep -Eq '__birnam_resumable_step_2$' "$smoke_root/candidate.symbols"
grep -Eq '__birnam_resumable_thunk_0$' "$smoke_root/candidate.symbols"
grep -Eq '__birnam_resumable_thunk_1$' "$smoke_root/candidate.symbols"
grep -Eq '__birnam_resumable_thunk_2$' "$smoke_root/candidate.symbols"
grep -Eq '__birnam_resolve_block_vtable_v1$' "$smoke_root/candidate.symbols"
nm -gj "$candidate" >"$smoke_root/candidate.exports"
if grep -Eq '__birnam_resumable_(step|thunk)_[0-9]+$' "$smoke_root/candidate.exports"; then
  echo "stackless image candidate leaked artifact-local resumable helpers" >&2
  exit 1
fi
test -s "$image"

cat >"$smoke_root/inspect.in" <<'EOF'
[stacklessProjectedClosure value]
[40 stacklessImageSmokeProbe]
[[CompiledUnit activeFor: #stacklessImageSmoke] kind]
[[CompiledUnit activeFor: #stacklessImageSmoke] state]
[[CompiledUnit activeFor: #stacklessImageSmoke] rollbackAvailable]
[[[DispatchGeneration current] predecessor] isNil]
[[[DispatchGeneration current] deltaUnit] isNil]
[[DispatchGeneration current] compositionIdentity]
[[db currentValue: #stacklessImageSmokeAcceptance attr: #origin] state]
[[db currentValue: #stacklessImageSmokeAcceptance attr: #origin] rollbackAvailable]
[[[db currentValue: #stacklessImageSmokeAcceptance attr: #origin] composedInto] == [DispatchGeneration current]]
:quit
EOF

BIRNAM_LIB_DIR="$lib" "$bin" "$image" <"$smoke_root/inspect.in" >"$smoke_root/mapped.out" 2>"$smoke_root/mapped.err"
BIRNAM_LIB_DIR="$lib" "$bin" --rebuild "$image" <"$smoke_root/inspect.in" >"$smoke_root/rebuilt.out" 2>"$smoke_root/rebuilt.err"

python3 - "$smoke_root/mapped.out" "$smoke_root/rebuilt.out" <<'PY'
import re
import sys

expected = ["99", "42", "#composed", "#active", "false", "true", "true"]
tail = ["#historical", "false", "true"]
identities = []
for path in sys.argv[1:]:
    with open(path, encoding="utf-8") as handle:
        answers = re.findall(r"^birnam> (.*)$", handle.read(), re.MULTILINE)
    assert answers[:7] == expected, (path, answers)
    assert re.fullmatch(r"[0-9a-f]{64}", answers[7]), (path, answers)
    assert answers[8:11] == tail, (path, answers)
    identities.append(answers[7])
assert identities[0] == identities[1], identities
PY

test ! -s "$smoke_root/mapped.err"
test ! -s "$smoke_root/rebuilt.err"

cat >"$smoke_root/resave.in" <<EOF
[System saveImage: "$smoke_root/resaved.image"]
:quit
EOF
BIRNAM_LIB_DIR="$lib" "$bin" "$image" <"$smoke_root/resave.in" >"$smoke_root/resave.out" 2>"$smoke_root/resave.err"
grep -q '^birnam> anObject$' "$smoke_root/resave.out"
test ! -s "$smoke_root/resave.err"
test -s "$smoke_root/resaved.image"
printf '[stacklessProjectedClosure value]\n[40 stacklessImageSmokeProbe]\n[[CompiledUnit activeFor: #stacklessImageSmoke] kind]\n:quit\n' \
  | BIRNAM_LIB_DIR="$lib" "$bin" "$smoke_root/resaved.image" >"$smoke_root/resaved.out" 2>"$smoke_root/resaved.err"
grep -Fxq 'birnam> 99' "$smoke_root/resaved.out"
grep -Fxq 'birnam> 42' "$smoke_root/resaved.out"
grep -Fxq 'birnam> #composed' "$smoke_root/resaved.out"
test ! -s "$smoke_root/resaved.err"

cat >"$smoke_root/evolve.in" <<EOF
let nextSource := "[Module named: #stacklessImageSmoke requires: #[]]; let stacklessProjectedClosure := [| 101]; [Reflect defineMethod: #stacklessImageSmokeProbe on: Integer as: [:self | [self + 3]]]"
let nextPath := "$smoke_root/next-candidate.dylib"
let nextRevision := [Codegen sourceRevisionOf: nextSource]
let nextDigest := [nextRevision at: 6]
let nextInitializerRow := #[#implementationDisposition nextDigest #nonMethodProgram 0 #[0] #blockBody #resumableRequired]
let nextAdapterRow := #[#implementationDisposition nextDigest #methodOperation 0 #[] #namedAdapter #resumableRequired]
let nextImplementationRow := #[#implementationDisposition nextDigest #methodOperation 0 #[0] #blockBody #resumableRequired]
[Codegen compile: nextSource output: nextPath selection: #[nextInitializerRow nextAdapterRow nextImplementationRow]]
let next := [CompiledUnit open: nextPath]
[next map]
[next activate]
[stacklessProjectedClosure value]
[40 stacklessImageSmokeProbe]
[next rollback]
[stacklessProjectedClosure value]
[40 stacklessImageSmokeProbe]
:quit
EOF

BIRNAM_LIB_DIR="$lib" "$bin" "$image" <"$smoke_root/evolve.in" >"$smoke_root/evolve.out" 2>"$smoke_root/evolve.err"
grep -Fxq 'birnam> 101' "$smoke_root/evolve.out"
grep -Fxq 'birnam> 43' "$smoke_root/evolve.out"
grep -Fxq 'birnam> 99' "$smoke_root/evolve.out"
grep -Fxq 'birnam> 42' "$smoke_root/evolve.out"
test ! -s "$smoke_root/evolve.err"

echo "stackless-image-lifecycle-smoke: generation-projected named+method-block+initializer-block live, candidate AOT, and image rollback passed"
