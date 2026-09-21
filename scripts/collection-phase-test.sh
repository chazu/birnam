#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo test --offline --locked -p birnam-collect --lib phase::tests
mise exec -- cargo test --offline --locked -p birnam-abi --lib collection_phase
mise exec -- cargo test --offline --locked -p birnam-runtime --lib runtime::collection_phase_tests
mise exec -- cargo build --offline --locked --workspace
phase_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
phase_lib=${1:+$1/lib}
phase_lib=${phase_lib:-$phase_target/debug}
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-collection-phase.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v20 -Iscripts/fixtures scripts/fixtures/collection-phase.c -o "$scratch/phase"
python3 - "$scratch/phase" "$phase_lib/libbirnam_id.dylib" <<'PY'
import resource,subprocess,sys
resource.setrlimit(resource.RLIMIT_CORE,(0,0))
subprocess.run([*sys.argv[1:],'normal'],check=True)
for mode,message in [('allocate-marking','id allocation attempted during collection'),('allocate-finalizing','id allocation attempted during collection'),('invalid-source-phase','invalid collection phase'),('unmounted','GC coordinator dependency not mounted')]:
 result=subprocess.run([*sys.argv[1:],mode],capture_output=True,text=True)
 assert result.returncode==-6 and result.stderr=='birnam-id invariant failed: '+message+'\n',(mode,result)
 print(mode+': exact rejection diagnostic preserved')
PY
mise exec -- rustc --edition=2024 -Cpanic=abort scripts/fixtures/collection-phase-host.rs --extern "birnam_host=$phase_target/debug/libbirnam_host.rlib" -L "dependency=$phase_target/debug/deps" -o "$scratch/host-probe"
python3 scripts/fixtures/collection-phase-host.py "$scratch/host-probe" "$phase_lib"
