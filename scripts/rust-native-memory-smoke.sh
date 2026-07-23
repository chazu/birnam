#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)

# Exercise the cross-language collector/finalizer fixture with Darwin's native
# allocator diagnostics. `leaks` is deliberately not used: id_init owns a
# process-lifetime runtime, so reporting that retained root as a leak would be a
# false positive. These settings instead poison fresh/freed storage, guard
# large allocations, and periodically validate allocator metadata.
export MallocPreScribble=1
export MallocScribble=1
export MallocGuardEdges=1
export MallocCheckHeapStart=1
export MallocCheckHeapEach=10000

bash "$repo/scripts/rust-gc-metadata-smoke.sh"
echo "rust-native-memory: Darwin scribble, guard-edge, and heap checks passed"
