#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo test --offline --locked -p birnam-trace-hooks --lib
mise exec -- cargo build --offline --locked --workspace
trace_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-trace-hooks.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v18 -Iscripts/fixtures scripts/fixtures/trace-hooks.c -o "$scratch/hooks"
python3 - "$scratch/hooks" "$trace_target/debug/libbirnam_id.dylib" <<'PY'
import resource,subprocess,sys
resource.setrlimit(resource.RLIMIT_CORE,(0,0))
subprocess.run([*sys.argv[1:],'normal'],check=True)
for mode,message in [('owner','Rust floor entered from a non-owner thread'),('observation','Id access during storage observation'),('invalid','vtable is not registered')]:
    result=subprocess.run([*sys.argv[1:],mode],capture_output=True,text=True)
    assert result.returncode==-6 and result.stderr=='birnam-id invariant failed: '+message+'\n',(mode,result)
    print(mode+': expected admission boundary')
PY
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v18 -Iscripts/fixtures scripts/fixtures/trace-hooks-consumers.c -o "$scratch/consumers"
for module in peg store portable tcc; do
  library=$module
  if [[ $module == portable ]]; then library=portable_code; fi
  "$scratch/consumers" "$trace_target/debug/libbirnam_id.dylib" "$trace_target/debug/libbirnam_$library.dylib" "$module"
done
