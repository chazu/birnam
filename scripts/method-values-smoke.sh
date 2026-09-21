#!/usr/bin/env bash
# Owned callable reflection through source, compiled code and image recovery.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: method-values-smoke.sh PREFIX}
bin="$prefix/bin/birnam"
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-method-values.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then
  trap 'rm -rf "$scratch"' EXIT
else
  printf 'method values smoke root: %s\n' "$scratch"
fi
cat >"$scratch/body.birnam" <<'BIRNAM'
let main := [|
  [System preciseGc: true];
  let addition := [Integer methodFor: #+];
  [System println: [addition invokeOn: 3 arguments: #[4]]];
  let target := [Object clone];
  [Reflect defineMethod: #methodValueAnswer on: target as: [:self | [System gc]; 71]];
  let original := [target methodFor: #methodValueAnswer];
  [Reflect defineMethod: #methodValueAnswer on: target as: [:self | 92]];
  let replacement := [target methodFor: #methodValueAnswer];
  [System println: [target methodValueAnswer]];
  [Reflect removeMethod: #methodValueAnswer on: target];
  [System gc];
  [System println: [[target methodFor: #methodValueAnswer] isNil]];
  [System println: [original invokeOn: target]];
  [System println: [replacement invokeOn: target]];
  let behavior := [Reflect vtableOf: target];
  [behavior addMethod: original as: #methodValueAlias];
  [System gc];
  [System println: [target methodValueAlias]];
  [System println: [[[original clone] == original] printString]];
  [behavior removeMethod: #methodValueAlias];
  let custom := [Object clone];
  let customBehavior := [Vtable ensurePrivateVtableFor: custom];
  let customMeta := [Vtable delegated];
  [Vtable reassociateBehaviorOf: customBehavior with: customMeta];
  let chooser := [Object clone];
  [Reflect defineMethod: #choose: on: chooser as: [:self :selector | original]];
  [customMeta addMethod: [chooser methodFor: #choose:] as: #lookup];
  [System println: [custom anything]];
  let malformed := [Object clone];
  let malformedBehavior := [Vtable ensurePrivateVtableFor: malformed];
  let malformedMeta := [Vtable delegated];
  [Vtable reassociateBehaviorOf: malformedBehavior with: malformedMeta];
  [Reflect defineMethod: #lookup on: malformedMeta as: [:self :selector | 42]];
  [System println: [[malformed anything] kind]];
  [System preciseGc: false];
  [System println: "method-values-ok"]
];
BIRNAM
cat "$scratch/body.birnam" >"$scratch/probe.birnam"
printf '[main value]\n' >>"$scratch/probe.birnam"
printf '7\n92\ntrue\n71\n92\n71\ntrue\n71\n#nativeInvalidRequest\nmethod-values-ok\n' >"$scratch/expected.out"
"$bin" "$scratch/probe.birnam" >"$scratch/source.out" 2>"$scratch/source.err"
test ! -s "$scratch/source.err"
cmp "$scratch/expected.out" "$scratch/source.out"
"$bin" --compile-module "$scratch/probe.birnam" "$scratch/probe.dylib"
rm "$scratch/probe.birnam"
"$bin" "$scratch/probe.dylib" >"$scratch/compiled.out" 2>"$scratch/compiled.err"
test ! -s "$scratch/compiled.err"
cmp "$scratch/expected.out" "$scratch/compiled.out"
cat "$scratch/body.birnam" >"$scratch/save.birnam"
printf '[System saveImage: "%s/app.image"]\n' "$scratch" >>"$scratch/save.birnam"
"$bin" "$scratch/save.birnam" >"$scratch/save.out" 2>"$scratch/save.err"
test ! -s "$scratch/save.err"
test -s "$scratch/app.image"
rm "$scratch/body.birnam" "$scratch/save.birnam"
for mode in mapped rebuilt; do
  arguments=()
  if [[ $mode == rebuilt ]]; then arguments+=(--rebuild); fi
  "$bin" "${arguments[@]}" "$scratch/app.image" >"$scratch/$mode.out" 2>"$scratch/$mode.err"
  test ! -s "$scratch/$mode.err"
  cmp "$scratch/expected.out" "$scratch/$mode.out"
done
"$bin" --bake "$scratch/app.image" "$scratch/app"
BIRNAM_LIB_DIR="$prefix/lib" "$scratch/app" >"$scratch/baked.out" 2>"$scratch/baked.err"
test ! -s "$scratch/baked.err"
cmp "$scratch/expected.out" "$scratch/baked.out"
for variant in plain private; do
  customization=''
  if [[ $variant == private ]]; then
    customization='[Reflect defineMethod: #probe on: method as: [:self | true]];'
  fi
  cat >"$scratch/reject.birnam" <<BIRNAM
let method := [Integer methodFor: #+];
$customization
[db assert: #methodValue attr: #value value: method];
let result := [System saveImage: "$scratch/rejected.image"];
[System println: [result kind]]
BIRNAM
  "$bin" "$scratch/reject.birnam" >"$scratch/reject.out" 2>"$scratch/reject.err"
  test ! -s "$scratch/reject.err"
  grep -Fxq '#methodNotImageable' "$scratch/reject.out"
  test ! -e "$scratch/rejected.image"
done
printf 'method-values-smoke: exact native/generated callables, replacement, removal, aliases, GC, source-free compiled/mapped/rebuilt/baked entry and image rejection passed\n'
