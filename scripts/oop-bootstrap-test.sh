#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
export MISE_TRUSTED_CONFIG_PATHS="$root"
export CARGO_TARGET_DIR="${BIRNAM_OOP_TARGET:-$root/.cargo-target/oop-slice}"
logs="$CARGO_TARGET_DIR/bootstrap-qualification"
mkdir -p "$logs"
# The seed is a build tool. Published Bootstrap is separately checked below;
# it still requires the complete owner closure and has no fallback to this seed.
mise exec -- cargo build --offline --locked --release --lib -p birnam-bootstrap --features lang-seed > "$logs/build.log" 2>&1
mise exec -- cargo build --offline --locked --release -p birnam-siphon --example oop_bootstrap > "$logs/caller-build.log" 2>&1
python3 - "$CARGO_TARGET_DIR" "$logs" <<'PY'
from pathlib import Path
import os,shutil,subprocess,sys,tempfile,json
build=Path(sys.argv[1]);logs=Path(sys.argv[2]);root=Path.cwd()
stage=Path(tempfile.mkdtemp(prefix='cold-',dir=logs))
names='id bootstrap root_registry namespace boolean nil collect object_model word bytes execution string integer float prelude system_output system_input environment file_io storage vector map peg artifact tcc native_backend codegen_runtime lang_seed'.split()
for name in names:shutil.copy2(build/'release'/f'libbirnam_{name}.dylib',stage)
subprocess.run(['clang','-dynamiclib','-O2','tests/fixtures/oop-bootstrap/returns.c','-o',str(stage/'returns.dylib')],check=True)
subprocess.run(['clang','-dynamiclib','-x','c','-','-o',str(stage/'not-module.dylib')],input=b'int not_a_module=17;',check=True)
env=dict(os.environ,BIRNAM_MODULES_DIR='/nonexistent',BIRNAM_TEST_MODULE_DIRECTORY='/nonexistent')
with open(logs/'cold.log','w') as output:
 subprocess.run([str(build/'release/examples/oop_bootstrap'),str(stage),str(root)],env=env,stdout=output,stderr=subprocess.STDOUT,check=True,timeout=300)
output=(logs/'cold.log').read_text()
for marker in ['OOP_COLD_COMPILER_BOOTSTRAP_OK','OOP_COMPILED_LANG_HANDOFF_OK','OOP_NATIVE_LOADER_STATUS_OK']:assert marker in output,output
print(output,end='')
(logs/'stage.txt').write_text(str(stage)+'\n')
PY
mise exec -- cargo build --offline --locked --release --lib -p birnam-bootstrap > "$logs/published-build-check.log" 2>&1
python3 - "$CARGO_TARGET_DIR" "$logs" <<'PY_PUBLISHED'
from pathlib import Path
import os,shutil,subprocess,sys
build,logs=map(Path,sys.argv[1:]);stage=Path((logs/'stage.txt').read_text().strip())
shutil.copy2(build/'release/libbirnam_bootstrap.dylib',stage/'libbirnam_bootstrap.dylib')
env=dict(os.environ,BIRNAM_MODULES_DIR='/nonexistent',BIRNAM_TEST_MODULE_DIRECTORY='/nonexistent')
with open(logs/'published-rejection.log','w') as output:
 subprocess.run([str(build/'release/examples/oop_bootstrap'),str(stage),str(Path.cwd()),'published-rejection'],env=env,stdout=output,stderr=subprocess.STDOUT,check=True,timeout=120)
assert 'OOP_PUBLISHED_BOOTSTRAP_REQUIRES_PROVIDERS_OK' in (logs/'published-rejection.log').read_text()
print((logs/'published-rejection.log').read_text(),end='')
PY_PUBLISHED
