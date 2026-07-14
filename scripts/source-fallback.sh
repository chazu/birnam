#!/usr/bin/env bash
set -euo pipefail

BIN=${BIRNAM_BIN:-zig-out/bin/birnam}

generation_digest() {
  local mode=$1
  if [[ "$mode" == source ]]; then
    printf '[Codegen currentGenerationDigest]\n:quit\n' |
      BIRNAM_FORCE_SOURCE=1 "$BIN" --repl 2>&1 |
      grep -Eo '[0-9a-f]{64}' | tail -1
  else
    printf '[Codegen currentGenerationDigest]\n:quit\n' |
      "$BIN" --repl 2>&1 |
      grep -Eo '[0-9a-f]{64}' | tail -1
  fi
}

aot=$(generation_digest aot)
source=$(generation_digest source)

[[ -n "$aot" ]]
[[ "$aot" == "$source" ]]
printf 'source-fallback: AOT and embedded source publish %s\n' "$source"
