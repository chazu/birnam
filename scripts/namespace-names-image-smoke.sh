#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:?usage: namespace-names-image-smoke.sh PREFIX}
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-namespace-names.XXXXXX")
if [[ -z ${BIRNAM_KEEP_SMOKE_ROOT:-} ]]; then trap 'rm -rf "$scratch"' EXIT; else printf 'namespace names root: %s\n' "$scratch"; fi
cc -std=c11 -Wall -Wextra -Werror -dynamiclib -Iabi/fixtures/idapi-v19 scripts/fixtures/namespace-native.c -o "$scratch/names.dylib"
cat >"$scratch/probe.birnam" <<BIRNAM
[System loadModule: "$scratch/names.dylib"];
[System namespaceRename: System];
let renamed := [[Reflect allMethodsOf: System] detect: [:entry | [[entry at: #selector] = #println:]]];
[System println: [[[renamed at: #target] = #NamespaceReplacement] printString]];
[System println: [[System namespaceRestoreNames] printString]];
[System namespaceHide: System];
let hidden := [[Reflect allMethodsOf: System] detect: [:entry | [[entry at: #selector] = #println:]]];
[System println: [[[[hidden at: #target] = #NamespaceReplacement] not] printString]];
[System println: [[System namespaceRestoreNames] printString]];
[System namespaceObserveNames];
[Alien wrap: "strlen" signature: "l s" as: #namespaceLength on: String];
[System println: [[System namespaceRestoreNames] printString]];
[System println: [[["three" namespaceLength] = 5] printString]];
[System namespaceObserveNames];
[System saveImage: "$scratch/observed.image"];
[System println: [[System namespaceRestoreNames] printString]]
BIRNAM
"$prefix/bin/birnam" "$scratch/probe.birnam" >"$scratch/out" 2>"$scratch/err"
printf 'true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n' >"$scratch/expected"
cmp "$scratch/expected" "$scratch/out"
test ! -s "$scratch/err"
test -s "$scratch/observed.image"
printf 'namespace-names: live callback/context replacement, Reflect alternate/null results, Alien and Image consumers passed\n'
