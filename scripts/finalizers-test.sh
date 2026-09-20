#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo test --offline --locked -p birnam-finalizers --lib
mise exec -- cargo test --offline --locked -p birnam-id --lib runtime::tests::finalizers
mise exec -- cargo test --offline --locked -p birnam-codegen-runtime --lib finalizer_tests
mise exec -- cargo build --offline --locked --workspace
finalizers_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-finalizers.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v19 -Iscripts/fixtures scripts/fixtures/finalizers.c -o "$scratch/finalizers"
python3 - "$scratch/finalizers" "$finalizers_target/debug/libbirnam_id.dylib" <<'PY'
import resource,subprocess,sys
resource.setrlimit(resource.RLIMIT_CORE,(0,0))
subprocess.run([*sys.argv[1:],'normal'],check=True)
for mode,message in [('owner','Rust floor entered from a non-owner thread'),('observation','Id access during storage observation')]:
    result=subprocess.run([*sys.argv[1:],mode],capture_output=True,text=True)
    assert result.returncode==-6 and result.stderr=='birnam-id invariant failed: '+message+'\n',(mode,result)
    print(mode+': admission precedes ignored null target')
PY
cc -std=c11 -Wall -Wextra -Werror scripts/fixtures/collection.c -o "$scratch/collection"
"$scratch/collection" "$finalizers_target/debug/libbirnam_id.dylib" "$finalizers_target/debug/libbirnam_trace.dylib" "$finalizers_target/debug/libbirnam_collect.dylib"
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v19 -Iscripts/fixtures scripts/fixtures/finalizers-consumers.c -o "$scratch/consumers"
cc -std=c11 -Wall -Wextra -Werror scripts/fixtures/finalizers-worker.c -o "$scratch/worker"
BIRNAM_WORKER_EXECUTABLE="$scratch/worker" FINALIZER_WORKER_PID_FILE="$scratch/worker.pid" \
python3 - "$scratch/consumers" "$finalizers_target/debug" <<'PY'
from pathlib import Path
import resource,subprocess,sys
resource.setrlimit(resource.RLIMIT_CORE,(0,0))
probe,libdir=sys.argv[1],Path(sys.argv[2])
for mode,library in [('peg','peg'),('builder','string_builder'),('tcc','tcc'),
                     ('portable','portable_code'),('alien','alien'),('isolate','isolate')]:
    subprocess.run([probe,str(libdir/'libbirnam_id.dylib'),str(libdir/f'libbirnam_{library}.dylib'),mode],check=True)
result=subprocess.run([probe,str(libdir/'libbirnam_id.dylib'),str(libdir/'libbirnam_alien.dylib'),'alien-replace'],capture_output=True,text=True)
assert result.returncode==-6 and result.stdout=='' and result.stderr=='birnam Rust Alien: IdApi was replaced\n',result
print('Alien rejects API replacement; all seven callers preserve saved finalizer selection and registration timing')
PY
