#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo test --offline --locked -p birnam-root-registry -p birnam-gc-settings --lib
mise exec -- cargo build --offline --locked -p birnam-id -p birnam-root-registry -p birnam-gc-settings -p birnam-payload-layout -p birnam-trace-hooks -p birnam-finalizers -p birnam-native-stack -p birnam-native-mark -p birnam-allocation-budget -p birnam-object-model -p birnam-method-observation -p birnam-namespace -p birnam-trace -p birnam-collect
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-root-registry.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v20 -Iscripts/fixtures scripts/fixtures/root-registry.c -o "$scratch/registry"
python3 - "$scratch/registry" "$(pwd)/target/debug/libbirnam_id.dylib" <<'PY'
import resource,subprocess,sys
resource.setrlimit(resource.RLIMIT_CORE,(0,0))
subprocess.run([*sys.argv[1:],'normal'],check=True)
for mode,message in [('overflow','gc: too many root providers'),('observation','Id access during storage observation'),('owner','Rust floor entered from a non-owner thread')]:
 result=subprocess.run([*sys.argv[1:],mode],capture_output=True,text=True)
 assert result.returncode == -6,(mode,result)
 assert result.stderr == 'birnam-id invariant failed: '+message+'\n',(mode,result.stderr)
 print(mode+': expected admission boundary')
PY
