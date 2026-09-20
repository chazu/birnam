#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo test --offline --locked -p birnam-storage --lib observation::tests
mise exec -- cargo test --offline --locked -p birnam-abi --lib storage_observation
mise exec -- cargo test --offline --locked -p birnam-id --lib runtime::storage::scope_tests
mise exec -- cargo build --offline --locked --workspace
scope_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scope_lib=${1:+$1/lib}
scope_lib=${scope_lib:-$scope_target/debug}
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-storage-observation-scope.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v20 -Iscripts/fixtures scripts/fixtures/storage-observation-scope.c -o "$scratch/scope"
python3 - "$scratch/scope" "$scope_lib/libbirnam_id.dylib" <<'PY'
import resource,subprocess,sys
resource.setrlimit(resource.RLIMIT_CORE,(0,0))
subprocess.run([*sys.argv[1:],'normal'],check=True)
subprocess.run([*sys.argv[1:],'advance-refusal'],check=True)
for mode,message in [('active-read','Id access during storage observation'),('owner-before-active','Rust floor entered from a non-owner thread')]:
 result=subprocess.run([*sys.argv[1:],mode],capture_output=True,text=True)
 assert result.returncode==-6 and result.stderr=='birnam-id invariant failed: '+message+'\n',(mode,result)
 print(mode+': exact rejection diagnostic preserved')
PY
mise exec -- rustc --edition=2024 -Cpanic=abort scripts/fixtures/storage-observation-scope-host.rs --extern "birnam_host=$scope_target/debug/libbirnam_host.rlib" -L "dependency=$scope_target/debug/deps" -o "$scratch/host-probe"
python3 scripts/fixtures/storage-observation-scope-host.py "$scratch/host-probe" "$scope_lib"
