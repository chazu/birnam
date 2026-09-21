#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mise exec -- cargo test --offline --locked -p birnam-native-mark --lib
mise exec -- cargo test --offline --locked -p birnam-abi --lib native_mark
mise exec -- cargo test --offline --locked -p birnam-runtime --lib gc::native_mark_tests
mise exec -- cargo build --offline --locked --workspace
native_mark_target=$(mise exec -- cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
scratch=$(mktemp -d "${TMPDIR:-/tmp}/birnam-native-mark.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iabi/fixtures/idapi-v20 scripts/fixtures/native-mark.c -o "$scratch/native-mark"
python3 - "$scratch/native-mark" "$native_mark_target/debug" "$scratch" <<'PY'
from pathlib import Path
import resource,shutil,subprocess,sys
resource.setrlimit(resource.RLIMIT_CORE,(0,0))
probe,libdir,scratch=Path(sys.argv[1]),Path(sys.argv[2]),Path(sys.argv[3])
second=scratch/'second';second.mkdir()
for library in libdir.glob('*.dylib'):
    target=second/library.name
    if library.name=='libbirnam_id.dylib':shutil.copy2(library,target)
    else:target.symlink_to(library)
args=[str(probe),str(libdir/'libbirnam_id.dylib'),str(libdir/'libbirnam_trace.dylib')]
subprocess.run([*args,'normal'],check=True)
for mode,message in [('escaped-null','GC mark callback escaped its invocation'),('escaped-value','GC mark callback escaped its invocation'),
                     ('thread','GC mark callback escaped its invocation'),('nested','GC mark callback reentered'),
                     ('reject','GC tracer rejected a candidate'),('other-floor','GC mark callback escaped its invocation')]:
    result=subprocess.run([*args,mode,str(second/'libbirnam_id.dylib')],capture_output=True,text=True)
    assert result.returncode==-6 and result.stdout=='' and result.stderr=='birnam-id invariant failed: '+message+'\n',(mode,result)
    print(mode+': exact callback scope boundary preserved')
PY
