#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo test --offline --locked -p birnam-allocation-budget --lib
mise exec -- cargo test --offline --locked -p birnam-abi --lib allocation_budget
mise exec -- cargo test --offline --locked -p birnam-id --lib heap::allocation_budget_tests
mise exec -- cargo build --offline --locked --workspace
budget_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
budget_lib=${1:+$1/lib}
budget_lib=${budget_lib:-$budget_target/debug}
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-allocation-budget.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v20 -Iscripts/fixtures scripts/fixtures/allocation-budget.c -o "$scratch/budget"
python3 - "$scratch/budget" "$budget_lib/libbirnam_id.dylib" <<'PY'
import resource,subprocess,sys
resource.setrlimit(resource.RLIMIT_CORE,(0,0))
subprocess.run([*sys.argv[1:],'normal'],check=True)
result=subprocess.run([*sys.argv[1:],'overflow'],capture_output=True,text=True)
assert result.returncode==-6 and result.stderr=='birnam-id invariant failed: GC allocation byte counter overflow\n',result
print('Loaded Heap overflow retains its fail-stop diagnostic')
PY
mise exec -- rustc --edition=2024 -Cpanic=abort scripts/fixtures/allocation-budget-host.rs --extern "birnam_host=$budget_target/debug/libbirnam_host.rlib" -L "dependency=$budget_target/debug/deps" -o "$scratch/host-probe"
python3 scripts/fixtures/allocation-budget-host.py "$scratch/host-probe" "$budget_lib"
