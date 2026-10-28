#!/usr/bin/env python3
"""Exercise existing Method/vtable operations with Store and Generation absent."""
import argparse,json,shutil,tempfile
from pathlib import Path
from validation_support import ROOT,clean_environment,command,save_json,sha
p=argparse.ArgumentParser(description=__doc__);p.add_argument('prefix',type=Path);a=p.parse_args()
prefix=a.prefix.resolve();work=Path(tempfile.mkdtemp(prefix='birnam-minimal-mutation-'));programs=work/'programs';programs.mkdir();floor=work/'floor';floor.mkdir()
modules='id root_registry namespace boolean nil collect object_model word bytes execution portable_code string integer vector map tcc native_backend codegen_runtime'.split()
source=work/'export.birnam'
source.write_text('''
[[|
let bodies := #[[:self | 11] [:self | 22] [:self | [self replaceNow]; 11] [:self :argument | argument]];
[0 to: [[bodies size] - 1] do: [:i |
  let body := [bodies at: i];
  [System println: [body captureCount]];
  [System writeFile: [[DIRECTORY , [i printString]] , ".bin"] contents: [[CodeObject canonicalBytesOf: [body codeObject]] asString]]
]]
] value]
'''.replace('DIRECTORY',json.dumps(str(programs)+'/')))
env=clean_environment();export=command([prefix/'bin/birnam',source],env=env,timeout=120)
assert export['exit_code']==0 and export['stdout']=='0\n'*4 and not export['stderr'],export
for module in modules:shutil.copy2(prefix/'lib'/f'libbirnam_{module}.dylib',floor)
for name in ['store','relational','generation','bootstrap']:assert not (floor/f'libbirnam_{name}.dylib').exists()
env.update(BIRNAM_MUTATION_PROGRAMS=str(programs),BIRNAM_TEST_MODULE_DIRECTORY=str(floor),BIRNAM_GC_PRECISE='1',CARGO_TARGET_DIR=str(ROOT/'.cargo-target/native-message-tests'),CARGO_PROFILE_DEV_PANIC='unwind')
run=command(['mise','exec','--','cargo','test','--offline','--locked','-p','birnam-siphon','--test','minimal_mutation','--','--include-ignored','--nocapture'],env=env,timeout=180)
receipt=dict(passed=run['exit_code']==0,prefix=str(prefix),export=export,run=run,
 scope='Existing primitives on a physically Store/Relational/Generation/Bootstrap-free floor. Portable bodies exported beforehand; no parser, source metadata, Globals alias or whole application bootstrap. The production Reflect define/remove facade is also exercised on that floor.',
 source_sha256={str(f.relative_to(ROOT)):sha(f) for f in [Path(__file__).resolve(),ROOT/'rust/birnam-siphon/tests/minimal_mutation.rs',ROOT/'rust/birnam-siphon/tests/support/minimal.rs',ROOT/'tests/fixtures/oop-host/minimal-mutation.c']},
 floor_sha256={f.name:sha(f) for f in floor.iterdir()},program_sha256={f.name:sha(f) for f in programs.iterdir()})
save_json(work/'results.json',receipt)
print(run['output']);print(work/'results.json');assert receipt['passed'],work
