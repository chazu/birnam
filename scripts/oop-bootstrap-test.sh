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
packages=(birnam-id birnam-bootstrap birnam-generation birnam-image birnam-alien birnam-isolate birnam-portable-code birnam-root-registry birnam-namespace birnam-boolean birnam-nil birnam-collect birnam-object-model birnam-word birnam-bytes birnam-execution birnam-string birnam-integer birnam-float birnam-prelude birnam-system-output birnam-system-input birnam-environment birnam-file-io birnam-storage birnam-vector birnam-map birnam-peg birnam-artifact birnam-tcc birnam-native-backend birnam-codegen-runtime birnam-lang-seed)
build_args=()
for package in "${packages[@]}"; do build_args+=(-p "$package"); done
mise exec -- cargo build --offline --locked --release --lib "${build_args[@]}" --features birnam-bootstrap/lang-seed > "$logs/build.log" 2>&1
mise exec -- cargo build --offline --locked --release -p birnam-siphon --example oop_bootstrap > "$logs/caller-build.log" 2>&1
python3 - "$CARGO_TARGET_DIR" "$logs" <<'PY'
from pathlib import Path
import os,shutil,subprocess,sys,tempfile
build=Path(sys.argv[1]);logs=Path(sys.argv[2]);root=Path.cwd()
stage=Path(tempfile.mkdtemp(prefix='cold-',dir=logs))
names='id bootstrap generation image alien isolate portable_code root_registry namespace boolean nil collect object_model word bytes execution string integer float prelude system_output system_input environment file_io storage vector map peg artifact tcc native_backend codegen_runtime lang_seed'.split()
for name in names:shutil.copy2(build/'release'/f'libbirnam_{name}.dylib',stage)
subprocess.run(['clang','-dynamiclib','-O2','tests/fixtures/oop-bootstrap/returns.c','-o',str(stage/'returns.dylib')],check=True)
subprocess.run(['clang','-dynamiclib','-O2','tests/fixtures/oop-bootstrap/foreign.c','-o',str(stage/'foreign.dylib')],check=True)
subprocess.run(['clang','-dynamiclib','-O2','tests/fixtures/oop-continuation/native.c','-o',str(stage/'continuation-native.dylib')],check=True)
subprocess.run(['clang','-dynamiclib','-x','c','-','-o',str(stage/'not-module.dylib')],input=b'int not_a_module=17;',check=True)
env=dict(os.environ,BIRNAM_MODULES_DIR='/nonexistent',BIRNAM_TEST_MODULE_DIRECTORY='/nonexistent')
with open(logs/'cold.log','w') as output:
 subprocess.run([str(build/'release/examples/oop_bootstrap'),str(stage),str(root)],env=env,stdout=output,stderr=subprocess.STDOUT,check=True,timeout=1200)
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
(stage/'libbirnam_lang_seed.dylib').unlink()
env=dict(os.environ,BIRNAM_MODULES_DIR='/nonexistent',BIRNAM_TEST_MODULE_DIRECTORY='/nonexistent')
with open(logs/'published.log','w') as output:
 subprocess.run([str(build/'release/examples/oop_bootstrap'),str(stage),str(Path.cwd()),'published'],cwd=stage,env=env,stdout=output,stderr=subprocess.STDOUT,check=True,timeout=300)
assert 'OOP_FULL_PUBLISHED_STARTUP_OK' in (logs/'published.log').read_text()
print((logs/'published.log').read_text(),end='')
required=stage/'libbirnam_cell.dylib';absent=stage/'libbirnam_cell.unavailable'
required.rename(absent)
try:
 with open(logs/'published-missing.log','w') as output:
  subprocess.run([str(build/'release/examples/oop_bootstrap'),str(stage),str(Path.cwd()),'published-missing'],cwd=stage,env=env,stdout=output,stderr=subprocess.STDOUT,check=True,timeout=120)
 assert 'OOP_PUBLISHED_MISSING_ARTIFACT_REJECTED' in (logs/'published-missing.log').read_text()
 print((logs/'published-missing.log').read_text(),end='')
finally: absent.rename(required)
PY_PUBLISHED
