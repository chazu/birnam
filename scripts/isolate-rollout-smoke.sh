#!/usr/bin/env bash
set -euo pipefail

root=$(pwd)
bin="${BIRNAM_BIN:-$root/zig-out/bin/birnam}"
lib_dir="${BIRNAM_LIB_DIR:-$root/zig-out/lib}"
base=/tmp/birnam-l2-base.image
successor=/tmp/birnam-l2-successor.image
candidate=/tmp/birnam-l2-v2.birnam
candidate3=/tmp/birnam-l2-v3.dylib
map_dir=/tmp/birnam-l2-mapfail
missing_dir=/tmp/birnam-l2-missing

cleanup() {
  rm -f "$base" "$successor" "$candidate" "$candidate3" "$candidate3.candidate"
  rm -rf "$map_dir" "$missing_dir"
}
trap cleanup EXIT
cleanup

"$bin" scripts/isolate-rollout-baseline.birnam >/dev/null
test -s "$base"

# Publish one ordinary candidate first, then derive a checksum-valid native
# bundle which is intentionally not a Birnam candidate. It passes inspection
# and fails at the explicit map/ABI boundary inside the worker.
prep=$(printf '%s\n' \
  '[System writeFile: "/tmp/birnam-l2-v2.birnam" contents: [LiveModules envelopeFor: [System readFile: "scripts/isolate-rollout-v2-module.birnam"] base: [LiveModules currentDigest]]]' \
  'let prepUnit := [LiveModules import: "/tmp/birnam-l2-v2.birnam"]' \
  'let prepRequest := [LiveModules evolutionRequestFor: prepUnit]' \
  '["PATH=" , [prepRequest at: 5]]' \
  '["CHECKSUM=" , [prepRequest at: 4]]' \
  ':q' | "$bin" "$base")
valid_path=$(sed -n 's/.*PATH=\([^[:space:]]*\.dylib\).*/\1/p' <<<"$prep" | tail -1)
valid_checksum=$(sed -n 's/.*CHECKSUM=\([0-9a-f]\{64\}\).*/\1/p' <<<"$prep" | tail -1)
test -n "$valid_path"
test -n "$valid_checksum"
test -s "$valid_path.candidate"

map_source="$lib_dir/libbirnam_store.dylib"
test -s "$map_source"
map_checksum=$(LC_ALL=C LANG=C shasum -a 256 "$map_source" | awk '{print $1}')
map_path="$map_dir/$map_checksum.dylib"
mkdir -p "$map_dir" "$missing_dir"
cp "$map_source" "$map_path"
awk -v checksum="$map_checksum" 'NR == 2 {$0 = "artifact=" checksum} {print}' \
  "$valid_path.candidate" >"$map_path.candidate"

session=$(printf '%s\n' \
  'let worker1 := [Isolate start: #Isolate selector: #_l2Probe: image: "/tmp/birnam-l2-base.image"]' \
  'let worker2 := [Isolate start: #Isolate selector: #_l2Probe: image: "/tmp/birnam-l2-base.image"]' \
  'let baseDigest := [worker1 request: #digest timeout: 2000]' \
  '["BEFORE=" , [[worker1 request: #version timeout: 2000] printString]]' \
  '[System writeFile: "/tmp/birnam-l2-v2.birnam" contents: [LiveModules envelopeFor: [System readFile: "scripts/isolate-rollout-v2-module.birnam"] base: [LiveModules currentDigest]]]' \
  'let unit := [LiveModules import: "/tmp/birnam-l2-v2.birnam"]' \
  'let request := [LiveModules evolutionRequestFor: unit]' \
  'let zeroBudget := [worker1 evolve: request timeout: 0]' \
  '["ZERO=" , [[zeroBudget class] printString]]' \
  '["ZERO_STILL=" , [[worker1 request: #version timeout: 2000] printString]]' \
  'let wrongBase := #[#activate [request at: 1] [request at: 2] [request at: 3] [request at: 4] [request at: 5] "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"]' \
  'let wrongBaseResult := [worker1 evolve: wrongBase timeout: 10000]' \
  '["BASE=" , [[wrongBaseResult errorKind] printString]]' \
  '["BASE_STILL=" , [[worker1 request: #version timeout: 2000] printString]]' \
  "let missingPath := [\"$missing_dir/\" , [[request at: 4] , \".dylib\"]]" \
  'let missing := #[#activate [request at: 1] [request at: 2] [request at: 3] [request at: 4] missingPath [request at: 6]]' \
  'let missingResult := [worker1 evolve: missing timeout: 10000]' \
  '["MISSING=" , [[missingResult errorKind] printString]]' \
  '["MISSING_STILL=" , [[worker1 request: #version timeout: 2000] printString]]' \
  'let invalidSource := #[#activate [request at: 1] [request at: 2] "not the authored source" [request at: 4] [request at: 5] [request at: 6]]' \
  'let invalidSourceResult := [worker1 evolve: invalidSource timeout: 10000]' \
  '["SOURCE=" , [[invalidSourceResult errorKind] printString]]' \
  '["SOURCE_STILL=" , [[worker1 request: #version timeout: 2000] printString]]' \
  'let checksumResult := [[| let artifactBytes := [System readFile: [request at: 5]]; [System writeFile: [request at: 5] contents: [artifactBytes , "x"]]; let result := [worker1 evolve: request timeout: 10000]; [System writeFile: [request at: 5] contents: artifactBytes]; result] value]' \
  '["CHECKSUM=" , [[checksumResult errorKind] printString]]' \
  '["CHECKSUM_STILL=" , [[worker1 request: #version timeout: 2000] printString]]' \
  "let mapFailure := #[#activate [request at: 1] [request at: 2] [request at: 3] \"$map_checksum\" \"$map_path\" [request at: 6]]" \
  'let mapResult := [worker1 evolve: mapFailure timeout: 10000]' \
  '["MAP=" , [[mapResult errorKind] printString]]' \
  '["MAP_STILL=" , [[worker1 request: #version timeout: 2000] printString]]' \
  'let accepted := [worker1 evolve: request timeout: 10000]' \
  '["ACT=" , [[accepted outcome] printString]]' \
  '["RECEIPT_OP=" , [[accepted operation] printString]]' \
  '["RECEIPT_MODULE=" , [[accepted module] printString]]' \
  '["RECEIPT_BASE=" , [[[accepted acceptedBaseDigest] = baseDigest] printString]]' \
  '["RECEIPT_PRIOR=" , [[[accepted priorDigest] = baseDigest] printString]]' \
  '["RECEIPT_CURRENT=" , [[[[accepted currentDigest] = baseDigest] not] printString]]' \
  '["RECEIPT_ERROR=" , [[[accepted errorKind] isNil] printString]]' \
  '["RECEIPT_MESSAGE=" , [[[[accepted message] size] > 0] printString]]' \
  '["AFTER=" , [[worker1 request: #version timeout: 2000] printString]]' \
  '["INDEPENDENT=" , [[worker2 request: #version timeout: 2000] printString]]' \
  '["SAVE=" , [[worker1 request: #save timeout: 10000] printString]]' \
  'let worker3 := [Isolate start: #Isolate selector: #_l2Probe: image: "/tmp/birnam-l2-successor.image"]' \
  '["BOOTED=" , [[worker3 request: #version timeout: 2000] printString]]' \
  'let lost := [worker3 evolve: [LiveModules rollbackRequestForDigest: [accepted currentDigest]] timeout: 10000]' \
  '["LOST_ROLLBACK=" , [[lost errorKind] printString]]' \
  'let rolled := [worker1 evolve: [LiveModules rollbackRequestForDigest: [accepted currentDigest]] timeout: 10000]' \
  '["ROLL=" , [[rolled outcome] printString]]' \
  '["RESTORED=" , [[worker1 request: #version timeout: 2000] printString]]' \
  'let secondRollback := [worker1 evolve: [LiveModules rollbackRequestForDigest: [rolled currentDigest]] timeout: 10000]' \
  '["SECOND_ROLLBACK=" , [[secondRollback errorKind] printString]]' \
  'let acceptedAgain := [worker1 evolve: request timeout: 10000]' \
  '["REACT=" , [[acceptedAgain outcome] printString]]' \
  'let source3 := [System readFile: "scripts/isolate-rollout-v3-module.birnam"]' \
  '[CompiledUnit compile: source3 named: #rolloutFixture output: "/tmp/birnam-l2-v3.dylib"]' \
  'let unit3 := [CompiledUnit open: "/tmp/birnam-l2-v3.dylib"]' \
  '[db assert: unit3 attr: #expectedBaseDigest value: [acceptedAgain currentDigest]]' \
  'let request3 := [LiveModules evolutionRequestFor: unit3]' \
  'let accepted3 := [worker1 evolve: request3 timeout: 10000]' \
  '["ACT3=" , [[accepted3 outcome] printString]]' \
  '["AFTER3=" , [[worker1 request: #version timeout: 2000] printString]]' \
  'let intervening := [worker1 evolve: [LiveModules rollbackRequestForDigest: [acceptedAgain currentDigest]] timeout: 10000]' \
  '["INTERVENING=" , [[intervening errorKind] printString]]' \
  '["INTERVENING_STILL=" , [[worker1 request: #version timeout: 2000] printString]]' \
  '[worker1 stop]' \
  '[worker2 stop]' \
  '[worker3 stop]' \
  ':q' | "$bin" "$base")

grep -q 'BEFORE=100' <<<"$session"
grep -q 'ZERO=#timeout' <<<"$session"
grep -q 'ZERO_STILL=100' <<<"$session"
grep -q 'BASE=#staleGeneration' <<<"$session"
grep -q 'BASE_STILL=100' <<<"$session"
grep -q 'MISSING=#candidateManifestUnreadable' <<<"$session"
grep -q 'MISSING_STILL=100' <<<"$session"
grep -q 'SOURCE=#evolutionIdentityMismatch' <<<"$session"
grep -q 'SOURCE_STILL=100' <<<"$session"
grep -q 'CHECKSUM=#candidateChecksumMismatch' <<<"$session"
grep -q 'CHECKSUM_STILL=100' <<<"$session"
grep -q 'MAP=#candidateAbiMismatch' <<<"$session"
grep -q 'MAP_STILL=100' <<<"$session"
grep -q 'ACT=#activated' <<<"$session"
grep -q 'RECEIPT_OP=#activate' <<<"$session"
grep -q 'RECEIPT_MODULE=#rolloutFixture' <<<"$session"
grep -q 'RECEIPT_BASE=true' <<<"$session"
grep -q 'RECEIPT_PRIOR=true' <<<"$session"
grep -q 'RECEIPT_CURRENT=true' <<<"$session"
grep -q 'RECEIPT_ERROR=true' <<<"$session"
grep -q 'RECEIPT_MESSAGE=true' <<<"$session"
grep -q 'AFTER=200' <<<"$session"
grep -q 'INDEPENDENT=100' <<<"$session"
grep -q 'SAVE=#saved' <<<"$session"
grep -q 'BOOTED=200' <<<"$session"
grep -q 'LOST_ROLLBACK=#rollbackUnavailable' <<<"$session"
grep -q 'ROLL=#rolledBack' <<<"$session"
grep -q 'RESTORED=100' <<<"$session"
grep -q 'SECOND_ROLLBACK=#rollbackUnavailable' <<<"$session"
grep -q 'REACT=#activated' <<<"$session"
grep -q 'ACT3=#activated' <<<"$session"
grep -q 'AFTER3=300' <<<"$session"
grep -q 'INTERVENING=#staleGeneration' <<<"$session"
grep -q 'INTERVENING_STILL=300' <<<"$session"

terminal_timeout=$(printf '%s\n' \
  'let worker := [Isolate start: #Isolate selector: #_l2Probe: image: "/tmp/birnam-l2-base.image"]' \
  '[System writeFile: "/tmp/birnam-l2-v2.birnam" contents: [LiveModules envelopeFor: [System readFile: "scripts/isolate-rollout-v2-module.birnam"] base: [LiveModules currentDigest]]]' \
  'let timeoutUnit := [LiveModules import: "/tmp/birnam-l2-v2.birnam"]' \
  'let timeoutRequest := [LiveModules evolutionRequestFor: timeoutUnit]' \
  'let timedOut := [worker evolve: timeoutRequest timeout: 10]' \
  '["POST_TIMEOUT=" , [[timedOut class] printString]]' \
  '["POST_STATE=" , [[worker state] printString]]' \
  'let afterTimeout := [worker request: #version timeout: 100]' \
  '["POST_REUSE=" , [[afterTimeout class] printString]]' \
  ':q' | BIRNAM_TEST_ISOLATE_FAULTS=evolution-delay "$bin" "$base")
grep -q 'POST_TIMEOUT=#timeout' <<<"$terminal_timeout"
grep -q 'POST_STATE=#exited' <<<"$terminal_timeout"
grep -q 'POST_REUSE=#stopped' <<<"$terminal_timeout"

printf 'isolate-rollout-smoke: L2 failure matrix, terminal timeout, receipts, independence, persistence, rollback lifetime, and intervening generation passed\n'
