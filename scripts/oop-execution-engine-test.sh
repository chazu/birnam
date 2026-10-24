#!/usr/bin/env bash
# Exercise the production frame engine while its message-facing module migrates.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
export MISE_TRUSTED_CONFIG_PATHS="$root"
export CARGO_TARGET_DIR="${BIRNAM_OOP_TARGET:-$root/.cargo-target/oop-slice}"
qualification="$CARGO_TARGET_DIR/qualification"
mkdir -p "$qualification"
CARGO_PROFILE_DEV_PANIC=unwind mise exec -- cargo build --offline --locked -p birnam-abi -p birnam-artifact-codec
python3 - "$root" "$CARGO_TARGET_DIR" <<'PY'
from pathlib import Path
import subprocess,sys,os
root=Path(sys.argv[1]);target=Path(sys.argv[2]);q=target/'qualification'
harness=q/'execution-engine.rs'
harness.write_text('#![allow(dead_code,unused_imports)]\n'+''.join(f'#[path="{root}/rust/birnam-execution/src/{name}.rs"] mod {name};\n' for name in ['entry','entry_plan','word_index','objects','execution']))
command=['mise','exec','--','rustc','--edition=2024','--test',str(harness),'-o',str(q/'execution-engine'),'-L',f'dependency={target}/debug/deps']
for crate in ['birnam_abi','birnam_artifact_codec']:
    command+=['--extern',f'{crate}={target}/debug/lib{crate}.rlib']
subprocess.run(command,check=True,env=dict(os.environ,CARGO_MANIFEST_DIR=str(root/"rust/birnam-execution")))
subprocess.run([str(q/'execution-engine')],check=True)
PY
