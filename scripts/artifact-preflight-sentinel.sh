#!/usr/bin/env bash
set -euo pipefail

test_binary=$1
sentinel_dylib=$2

export BIRNAM_PREFLIGHT_SENTINEL_DYLIB="$sentinel_dylib"
exec "$test_binary"
