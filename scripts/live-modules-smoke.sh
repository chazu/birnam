#!/usr/bin/env bash
set -euo pipefail

root=$(pwd)
bin="${BIRNAM_BIN:-$root/zig-out/bin/birnam}"
base_lib="${BIRNAM_LIB_DIR:-$root/zig-out/lib}"
bin=$(cd "$(dirname "$bin")" && pwd)/$(basename "$bin")
base_lib=$(cd "$base_lib" && pwd)
tmp=$(mktemp -d /tmp/birnam-live-host.XXXXXX)
lib="$tmp/lib"
candidate="$tmp/host-live-v2.birnam"
image="$tmp/host-live.image"
mkdir -p "$lib"
trap 'rm -rf "$tmp"' EXIT

for dylib in "$base_lib"/*.dylib; do
  ln -s "$dylib" "$lib/$(basename "$dylib")"
done
cp scripts/live-host-birnam.toml "$tmp/birnam.toml"

BIRNAM_LIB_DIR="$base_lib" "$bin" --compile-module \
  scripts/live-host-fixture-v1.birnam "$lib/libbirnam_hostLiveFixture.dylib" >/dev/null

prepare="[System writeFile: \"$candidate\" contents: [LiveModules envelopeFor: [System readFile: \"$root/scripts/live-host-fixture-v2.birnam\"] base: [LiveModules currentDigest]]]"
printf '%s\n:q\n' "$prepare" | (cd "$tmp" && BIRNAM_LIB_DIR="$lib" "$bin" --repl) >/dev/null
test -s "$candidate"

session=$(printf ':live stage %s\n:live activate %s\n[7 hostLiveProbe]\n["GEN=" , [LiveModules currentDigest]]\n["REV=" , [[LiveModules revisionFor: #hostLiveFixture] at: 6]]\n:live save %s\n:q\n' \
  "$candidate" "$candidate" "$image" | (cd "$tmp" && BIRNAM_LIB_DIR="$lib" "$bin" --repl))
test -s "$image"
grep -q 'candidate(#hostLiveFixture #candidate)' <<<"$session"
grep -q 'candidate(#hostLiveFixture #active)' <<<"$session"
grep -q 'birnam> 200' <<<"$session"
grep -q '#status -> #saved' <<<"$session"
generation=$(grep -o 'GEN=[0-9a-f]\{64\}' <<<"$session" | tail -1)
revision=$(grep -o 'REV=[0-9a-f]\{64\}' <<<"$session" | tail -1)
test -n "$generation"
test -n "$revision"

checks='[7 hostLiveProbe]
["GEN=" , [LiveModules currentDigest]]
["REV=" , [[LiveModules revisionFor: #hostLiveFixture] at: 6]]
:q'
mapped=$(printf '%s\n' "$checks" | (cd "$tmp" && BIRNAM_LIB_DIR="$lib" "$bin" "$image"))
rebuilt=$(printf '%s\n' "$checks" | (cd "$tmp" && BIRNAM_LIB_DIR="$lib" "$bin" --rebuild "$image") 2>/dev/null)
grep -q 'birnam> 200' <<<"$mapped"
grep -q 'birnam> 200' <<<"$rebuilt"
grep -q "$generation" <<<"$mapped"
grep -q "$generation" <<<"$rebuilt"
grep -q "$revision" <<<"$mapped"
grep -q "$revision" <<<"$rebuilt"

printf 'live-modules-smoke: host activation and mapped/rebuilt baselines passed\n'
