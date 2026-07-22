#!/usr/bin/env bash
# Manual compatibility-fixture rebuild. Ordinary build/test never runs this.
set -euo pipefail
script_dir=$(cd "$(dirname "$0")" && pwd -P)

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <output-directory>" >&2
  echo "review rebuilt artifacts before deliberately replacing the frozen files" >&2
  exit 2
fi

out=$1
mkdir -p "$out"
out=$(cd "$out" && pwd -P)
cd "$script_dir"

cc -std=c11 -O2 -Wall -Wextra -Werror -Wl,-no_uuid \
  client.c -o "$out/idapi-v1-client.arm64-macos"
cc -std=c11 -O2 -Wall -Wextra -Werror -fvisibility=hidden \
  -dynamiclib -Wl,-no_uuid -Wl,-install_name,@rpath/idapi-v1-module.arm64-macos.dylib \
  module.c -o "$out/idapi-v1-module.arm64-macos.dylib"

file "$out/idapi-v1-client.arm64-macos" "$out/idapi-v1-module.arm64-macos.dylib"
