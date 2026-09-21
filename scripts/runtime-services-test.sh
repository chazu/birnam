#!/usr/bin/env bash
# Existing independent C service oracles, entered through production Id.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix=${1:-}
group=${2:-all}
CARGO_PROFILE_DEV_PANIC=unwind CARGO_TARGET_DIR=.cargo-target/runtime-services-tests mise exec -- cargo test --offline --locked -p birnam-root-registry -p birnam-gc-settings -p birnam-native-stack --lib
if [[ -n $prefix ]]; then
  mise exec -- cargo build --offline --locked -p birnam-host -p birnam-abi
else
  mise exec -- cargo build --offline --locked --workspace
fi
services_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
lib=${prefix:+$prefix/lib}
lib=${lib:-$services_target/debug}
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-runtime-services.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
mise exec -- rustc --edition=2024 -Cpanic=abort scripts/fixtures/runtime-services-host.rs \
  --extern "birnam_host=$services_target/debug/libbirnam_host.rlib" \
  --extern "birnam_abi=$services_target/debug/libbirnam_abi.rlib" \
  -L "dependency=$services_target/debug/deps" -o "$scratch/host"
for fixture in root-registry gc-settings native-stack native-stack-scan; do
  cc -std=c11 -Wall -Wextra -Werror -dynamiclib -Iabi/fixtures/idapi-v20 -Iscripts/fixtures \
    "scripts/fixtures/$fixture.c" -o "$scratch/$fixture.dylib"
done
python3 - "$scratch/host" "$lib" "$scratch" "$group" <<'PY'
from pathlib import Path
import resource, subprocess, sys
resource.setrlimit(resource.RLIMIT_CORE,(0,0))
host,lib,scratch,group=sys.argv[1:]
owner='Rust floor entered from a non-owner thread'
observation='Id access during storage observation'
cases={
 'root-registry':[('overflow','gc: too many root providers'),('observation',observation),('owner',owner)],
 'gc-settings':[(f'{kind}-{setting}',message) for kind,message in [('owner',owner),('observation',observation)] for setting in ['auto','precise']],
 'native-stack':[(f'{kind}-{value}',message) for kind,message in [('owner',owner),('observation',observation)] for value in ['zero','nonzero']],
 'native-stack-scan':[('reject','GC tracer rejected a candidate')],
}
assert group in ['all','root-registry','gc-settings','native-stack']
passed=0
for kind,rejections in cases.items():
 if group!='all' and kind!=group and not (group=='native-stack' and kind=='native-stack-scan'):continue
 def command(mode):return [host,lib,str(Path(scratch)/f'{kind}.dylib'),mode,kind]
 subprocess.run(command('normal'),check=True);passed+=1
 for mode,message in rejections:
  result=subprocess.run(command(mode),capture_output=True,text=True)
  assert result.returncode==-6,(kind,mode,result)
  assert result.stderr=='birnam-id invariant failed: '+message+'\n',(kind,mode,result.stderr)
  if kind=='native-stack-scan':assert result.stdout=='candidate 0\n',result
  print(f'{kind} {mode}: exact rejection preserved',flush=True);passed+=1
print(f'Runtime services: {passed} production native cases passed',flush=True)
PY
