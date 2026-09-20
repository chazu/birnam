#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo test --offline --locked -p birnam-native-stack --lib
mise exec -- cargo test --offline --locked -p birnam-id --lib runtime::tests::native_stack_anchor
mise exec -- cargo build --offline --locked --workspace
native_stack_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-native-stack.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v20 -Iscripts/fixtures scripts/fixtures/native-stack.c -o "$scratch/native-stack"
python3 - "$scratch/native-stack" "$native_stack_target/debug/libbirnam_id.dylib" <<'PY'
import resource,subprocess,sys
resource.setrlimit(resource.RLIMIT_CORE,(0,0))
subprocess.run([*sys.argv[1:],'normal'],check=True)
for mode,message in [('owner','Rust floor entered from a non-owner thread'),('observation','Id access during storage observation')]:
    for value in ['zero','nonzero']:
        result=subprocess.run([*sys.argv[1:],mode+'-'+value],capture_output=True,text=True)
        assert result.returncode==-6 and result.stderr=='birnam-id invariant failed: '+message+'\n',(mode,value,result)
        print(mode+' '+value+': cached setter admission preserved')
PY
if [[ $# -gt 0 ]]; then
  BIRNAM_NATIVE_STACK_PREFIX="$1" mise exec -- cargo test --offline --locked -p birnam-host --test native_stack -- --ignored
fi
