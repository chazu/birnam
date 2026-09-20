#!/usr/bin/env bash
# Focused debug check of the production storage boundary; no installation.
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo build -p birnam-namespace -p birnam-object-model -p birnam-method-observation -p birnam-id -p birnam-string -p birnam-storage -p birnam-integer -p birnam-float -p birnam-word -p birnam-error-value --locked --offline
storage_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-storage-observation.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror scripts/fixtures/storage-observation.c -o "$scratch/probe"
"$scratch/probe" "$storage_target/debug/libbirnam_id.dylib" \
  "$storage_target/debug/libbirnam_storage.dylib" "$storage_target/debug/libbirnam_integer.dylib"
