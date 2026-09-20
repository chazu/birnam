#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo test --offline --locked -p birnam-method-observation -p birnam-root-registry -p birnam-gc-settings --lib
mise exec -- cargo build --offline --locked -p birnam-id -p birnam-method-observation -p birnam-root-registry -p birnam-gc-settings -p birnam-payload-layout -p birnam-trace-hooks -p birnam-finalizers -p birnam-native-stack -p birnam-object-model -p birnam-namespace -p birnam-integer -p birnam-float -p birnam-word -p birnam-string -p birnam-trace -p birnam-collect -p birnam-store
observation_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-method-observation.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
for fixture in method-observation method-observation-store; do
  cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v20 -Iscripts/fixtures "scripts/fixtures/$fixture.c" -o "$scratch/$fixture"
done
for mode in normal root-reentry hook-reentry; do
  "$scratch/method-observation-store" "$observation_target/debug/libbirnam_id.dylib" "$observation_target/debug/libbirnam_store.dylib" "$mode"
done
python3 - "$scratch/method-observation" "$observation_target/debug/libbirnam_id.dylib" <<'PY'
import subprocess,sys
cases=['normal','owner','observation']+[f'invalid-{kind}{suffix}' for kind in ['null','instance','stale','garbage'] for suffix in ['', '-hook']]
for case in cases:
    result=subprocess.run([sys.argv[1],sys.argv[2],case],capture_output=True,text=True)
    if case=='normal':
        assert result.returncode==0 and not result.stderr,(case,result)
        print(result.stdout,end='')
    else:
        expected='Rust floor entered from a non-owner thread' if case=='owner' else 'Id access during storage observation' if case=='observation' else 'vtable is not registered'
        assert result.returncode<0 and not result.stdout and result.stderr=='birnam-id invariant failed: '+expected+'\n',(case,result)
print('method observation: owner/observation admission and invalid targets with/without a hook passed')
PY
