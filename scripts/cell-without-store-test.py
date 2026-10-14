#!/usr/bin/env python3
"""Export actual Cell CodeObjects; exercise them in a floor where Store cannot load."""
import argparse
import json
from pathlib import Path
import shutil
import tempfile
from validation_support import ROOT, clean_environment, command, save_json, sha

MODULES = 'id root_registry namespace boolean nil collect object_model word bytes execution portable_code string integer vector map tcc native_backend codegen_runtime'.split()

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('prefix',type=Path);args=p.parse_args()
    prefix=args.prefix.resolve();work=Path(tempfile.mkdtemp(prefix='birnam-cell-without-store-'))
    programs=work/'programs';programs.mkdir();floor=work/'floor';floor.mkdir()
    source=work/'export.birnam'
    source.write_text('''
[[|
let rows := [Reflect directMethodsOf: Cell];
let names := #[#with: #value #set: #history #inc #dec];
[0 to: 5 do: [:index |
  let row := [rows detect: [:row | [[row at: #selector] == [names at: index]]]];
  let block := [row at: #method];
  [System println: [block captureCount]];
  let bytes := [CodeObject canonicalBytesOf: [block codeObject]];
  [System writeFile: [[DIRECTORY , [index printString]] , ".bin"] contents: [bytes asString]]
]]
] value]
'''.replace('DIRECTORY',json.dumps(str(programs)+'/')))
    env=clean_environment()
    export=command([prefix/'bin/birnam',source],env=env,timeout=120)
    (work/'export.log').write_text(export['output'])
    assert export['exit_code']==0 and not export['stderr'] and export['stdout']=='0\n'*6,(work,export)
    for module in MODULES: shutil.copy2(prefix/'lib'/f'libbirnam_{module}.dylib',floor)
    assert not (floor/'libbirnam_store.dylib').exists()
    env.update(BIRNAM_CELL_PROGRAMS=str(programs), BIRNAM_TEST_MODULE_DIRECTORY=str(floor),
               CARGO_TARGET_DIR=str(ROOT/'.cargo-target/native-message-tests'),CARGO_PROFILE_DEV_PANIC='unwind')
    run=command(['mise','exec','--','cargo','test','--offline','--locked','-p','birnam-siphon','--test','cell_without_store','--','--include-ignored'],env=env,timeout=180)
    (work/'run.log').write_text(run['output'])
    receipt={'passed':run['exit_code']==0,'prefix':str(prefix),'export':export,'run':run,
      'source_sha256':{str(f.relative_to(ROOT)):sha(f) for f in [ROOT/'modules/cell.birnam',ROOT/'rust/birnam-siphon/tests/cell_without_store.rs',Path(__file__).resolve()]},
      'program_sha256':{f.name:sha(f) for f in programs.glob('*.bin')},
      'floor_sha256':{f.name:sha(f) for f in floor.iterdir()},
      'scope':'Production method bodies, with a Namespace binding owner and raw bootstrap publication; not a Store-free full application bootstrap.'}
    save_json(work/'results.json',receipt)
    assert receipt['passed'],work
    print('cell-without-store: passed; '+str(work/'results.json'))

if __name__=='__main__':main()
