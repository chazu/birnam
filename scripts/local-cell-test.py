#!/usr/bin/env python3
"""Check local defaults, explicit facts, reentry, reclamation and source-free images."""
import argparse
import json
from pathlib import Path
import tempfile
from validation_support import ROOT, clean_environment, command, save_json, sha


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix', type=Path)
    parser.add_argument('--skip-images', action='store_true')
    args = parser.parse_args()
    prefix = args.prefix.resolve()
    work = Path(tempfile.mkdtemp(prefix='birnam-local-cells-'))
    env = clean_environment(); env['BIRNAM_GC_PRECISE'] = '1'
    fixture = ROOT/'tests/fixtures/local-cells.birnam'
    receipt = dict(prefix=str(prefix), passed=False, runs=[],
        source_sha256={str(p.relative_to(ROOT)):sha(p) for p in [fixture, Path(__file__).resolve(), ROOT/'modules/stdlib.birnam', ROOT/'modules/modules.birnam']})
    def run(name, argv, expected=None):
        result = command(argv, cwd=work, env=env, timeout=180)
        (work/(name+'.log')).write_text(result['output'])
        receipt['runs'].append(dict(name=name, **result)); save_json(work/'results.json', receipt)
        assert result['exit_code']==0 and not result['stderr'], (name,work)
        if expected is not None: assert result['stdout']==expected, (name,result['stdout'],work)
        return result
    result = run('local-cells', [prefix/'bin/birnam',fixture])
    rows = json.loads(result['stdout'])
    assert rows['passed'] and [b['calls'] for b in rows['batches']]==[0,1,50,200]
    assert all(b['fact_delta']==0 for b in rows['batches'])
    receipt['retention'] = rows
    native_env=dict(env, BIRNAM_TEST_MODULE_DIRECTORY=str(prefix/'lib'), CARGO_TARGET_DIR=str(ROOT/'.cargo-target/native-message-tests'), CARGO_PROFILE_DEV_PANIC='unwind')
    native=command(['mise','exec','--','cargo','test','--offline','--locked','-p','birnam-siphon','--test','local_cell_retention','--','--include-ignored','--nocapture'],env=native_env,timeout=180)
    (work/'native-retention.log').write_text(native['output'])
    receipt['native_retention']=native
    receipt['retention_scope']='Language object deltas are diagnostic; exact native counter after return requires zero added objects and facts.'
    save_json(work/'results.json',receipt)
    assert native['exit_code']==0,(work,native)
    if not args.skip_images:
        image = work/'cells.image'; source = work/'save.birnam'
        # Save both modes and shared aliases. main creates another default Cell
        # after restoration, so the restored asCell definition is also exercised.
        source.write_text('''[Module load: #store];

[[|
let local := [17 asCell]; let alias := local;
let store := db; [store assert: #amount attr: #history value: true];
let entity := [Object clone]; let fact := [store cellFor: entity attr: #amount];
[fact set: 23]; let basis := [store basis]; [fact set: 24];
[Globals at: #main put: [|
  [System gc];
  [System println: [local == alias]];
  [System println: [local value]];
  [alias set: 19]; [System gc]; [System println: [local value]];
  [System println: [local history]];
  [System println: [fact value]];
  [System println: [store valueOf: entity attr: #amount asOf: basis]];
  let before := [db retainedFactCount]; let fresh := [nil asCell];
  [fresh set: local]; [System gc];
  [System println: [[fresh value] == alias]];
  [System println: [[db retainedFactCount] = before]]
]];
[System saveImage: IMAGE]; [System println: [[System imageSaveReport] status]]
] value]
'''.replace('IMAGE',json.dumps(str(image))))
        receipt['image_source_sha256'] = sha(source)
        run('save', [prefix/'bin/birnam',source], '#saved\n')
        assert image.is_file(); source.unlink()
        for name, flags in [('mapped', []), ('rebuilt', ['--rebuild'])]:
            run(name, [prefix/'bin/birnam',*flags,image], 'true\n17\n19\n0\n24\n23\ntrue\ntrue\n')
        receipt['image_sha256'] = sha(image)
    receipt['passed'] = True; receipt['images_checked'] = not args.skip_images
    save_json(work/'results.json',receipt)
    print('local-cells: passed; '+str(work/'results.json'))


if __name__ == '__main__': main()
