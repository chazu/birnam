#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo test --offline --locked -p birnam-payload-layout --lib
mise exec -- cargo build --offline --locked -p birnam-id -p birnam-gc-settings -p birnam-payload-layout -p birnam-trace-hooks -p birnam-root-registry -p birnam-object-model -p birnam-method-observation -p birnam-namespace -p birnam-trace -p birnam-collect -p birnam-tcc
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-payload-layout.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v18 -Iscripts/fixtures scripts/fixtures/payload-layout.c -o "$scratch/layouts"
python3 - "$scratch/layouts" "$(pwd)/target/debug/libbirnam_id.dylib" <<'PY'
import resource,subprocess,sys
resource.setrlimit(resource.RLIMIT_CORE,(0,0))
subprocess.run([*sys.argv[1:],'normal'],check=True)
for mode in ['owner-set','owner-slot','observation-set','observation-slot','invalid-set','invalid-slot','null-prefix','null-prefix-invalid']:
 result=subprocess.run([*sys.argv[1:],mode],capture_output=True,text=True)
 message=('Rust floor entered from a non-owner thread' if mode.startswith('owner')
          else 'Id access during storage observation' if mode.startswith('observation')
          else 'setLayout kinds is null with nonzero count' if mode.startswith('null-prefix')
          else 'vtable is not registered')
 assert result.returncode == -6,(mode,result)
 assert result.stderr == 'birnam-id invariant failed: '+message+'\n',(mode,result.stderr)
 print(mode+': expected admission boundary')
PY
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v18 scripts/fixtures/payload-layout-tcc.c -o "$scratch/tcc-layouts"
"$scratch/tcc-layouts" "$(pwd)/target/debug/libbirnam_id.dylib" "$(pwd)/target/debug/libbirnam_tcc.dylib"
