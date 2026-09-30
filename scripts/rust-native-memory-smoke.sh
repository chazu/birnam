#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
prefix=${1:?installed prefix required}
export BIRNAM_TEST_MODULE_DIRECTORY="$(cd "$prefix/lib" && pwd)"
cd "$repo"

# Exercise the message-based collector and root owners with Darwin allocator
# diagnostics. Poison fresh/freed storage, guard large allocations, and
# periodically validate allocator metadata.
export MallocPreScribble=1
export MallocScribble=1
export MallocGuardEdges=1
export MallocCheckHeapStart=1
export MallocCheckHeapEach=10000

cargo test --offline --locked -p birnam-siphon \
  --test memory --test memory_activation --test roots --test system_gc \
  -- --include-ignored
echo "rust-native-memory: Darwin scribble, guard-edge, and heap checks passed"
