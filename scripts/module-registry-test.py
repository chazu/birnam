#!/usr/bin/env python3
"""Owned registry retention, new/legacy source-free images and live edits."""
import argparse
import json
from pathlib import Path
import tempfile
from validation_support import ROOT, clean_environment, command, save_json, sha


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('prefix',type=Path);p.add_argument('--legacy-prefix',type=Path)
    p.add_argument('--skip-images',action='store_true');args=p.parse_args()
    prefix=args.prefix.resolve();work=Path(tempfile.mkdtemp(prefix='birnam-module-registry-'))
    env=clean_environment();env['BIRNAM_GC_PRECISE']='1'
    receipt={'passed':False,'prefix':str(prefix),'runs':[], 'source_sha256':{str(f.relative_to(ROOT)):sha(f) for f in [Path(__file__).resolve(),ROOT/'modules/modules.birnam',ROOT/'tests/fixtures/module-registry.birnam',ROOT/'tests/fixtures/module-registry-reentry.birnam',ROOT/'rust/birnam-siphon/tests/module_registry_retention.rs']}}
    def run(name,argv,expected=None):
        r=command(argv,cwd=work,env=env,timeout=240)
        receipt['runs'].append(dict(name=name,**r));save_json(work/'results.json',receipt)
        (work/(name+'.log')).write_text(r['output'])
        assert r['exit_code']==0 and not r['stderr'],(name,work,r)
        if expected is not None:assert r['stdout']==expected,(name,work,r)
        return r
    run('reentry',[prefix/'bin/birnam',ROOT/'tests/fixtures/module-registry-reentry.birnam'],'module-registry-reentry: passed\n')
    r=run('retention',[prefix/'bin/birnam',ROOT/'tests/fixtures/module-registry.birnam'])
    rows=json.loads(r['stdout']);assert rows['passed']
    assert [r['calls'] for r in rows['batches']]==[0,1,30,100]
    assert all(r['fact_delta']==0 for r in rows['batches']),rows
    receipt['retention']=rows
    # System liveObjects itself performs allocating language sends after GC.
    # Check exact reclamation outside that path, after the same authored batch
    # returns. Do not turn a variable measurement overhead into a leak budget.
    native_env=dict(env,CARGO_TARGET_DIR=str(ROOT/'.cargo-target/native-message-tests'),
        BIRNAM_TEST_MODULE_DIRECTORY=str(prefix/'lib'),CARGO_PROFILE_DEV_PANIC='unwind')
    native=command(['cargo','test','--offline','--locked','-p','birnam-siphon',
        '--test','module_registry_retention','--','--include-ignored','--nocapture'],
        cwd=ROOT,env=native_env,timeout=180)
    receipt['runs'].append(dict(name='native-retention',**native));save_json(work/'results.json',receipt)
    (work/'native-retention.log').write_text(native['output'])
    assert native['exit_code']==0 and '1 passed; 0 failed' in native['output'],(work,native)
    receipt['retention_scope']='Language object deltas are diagnostic: the queries allocate. Native post-return collection requires exactly zero retained objects for 0, 1, 30 and repeated 100-call batches.'
    if not args.skip_images:
        for label,writer in [('new',prefix)]+([('legacy',args.legacy_prefix.resolve())] if args.legacy_prefix else []):
            image=work/(label+'.image');source=work/(label+'.birnam')
            source.write_text('''
[Module named: #registryImageLeaf requires: #[]];
[Module named: #registryImageRoot requires: #[#registryImageLeaf]];
[Module markLoaded: #registryImageRoot];
[[|
let clone := [Module clone];
SNAPSHOT
[Globals at: #main put: [|
  [System gc];
  [System println: [Module loadOrder: #[#registryImageRoot]]];
  [System println: [Module isLoaded: #registryImageRoot]];
  [System println: [Module isLoaded: #registryImageLeaf]];
  [System println: [[Module libDir] = DIRECTORY]];
  EDIT
  [System gc];
  [System println: [Module loadOrder: #[#registryImageRoot]]];
  [System println: [clone requiresOf: #registryImageRoot]];
  VERIFY
]];
[Module libDir: "/old-install/lib"];
[System saveImage: IMAGE]; [System println: [[System imageSaveReport] status]]
] value]
'''.replace('DIRECTORY',json.dumps(str(prefix/'lib'))).replace('IMAGE',json.dumps(str(image)))
.replace('SNAPSHOT','let snapshot := [Module registry];' if label=='new' else '')
.replace('EDIT', '''let records := [Reflect slot: #registry in: Module];
  let edited := [[records at: #registryImageRoot] at: #requires put: #[#changed]];
  [Module record: #registryImageRoot put: edited];'''
 if label=='new' else '[db assert: #registryImageRoot attr: #requires value: #[#changed]];')
.replace('VERIFY', '''[System println: [[snapshot at: #registryImageRoot] at: #requires]];
  [System println: [[db currentValue: #registryImageRoot attr: #requires] isNil]]'''
 if label=='new' else '[System println: [[db currentValue: #registryImageRoot attr: #requires] = #[#changed]]]'))
            receipt[label+'_source']=source.read_text();run(label+'-save',[writer/'bin/birnam',source],'#saved\n');source.unlink()
            expected='#[#registryImageLeaf #registryImageRoot]\ntrue\nfalse\ntrue\n#[#changed #registryImageRoot]\n'
            expected += '#[#registryImageLeaf]\n#[#registryImageLeaf]\ntrue\n' if label=='new' else '#[#changed]\ntrue\n'
            for mode,flags in [('mapped',[]),('rebuilt',['--rebuild'])]:
                run(label+'-'+mode,[prefix/'bin/birnam',*flags,image],expected)
            receipt[label+'_image_sha256']=sha(image)
    receipt['passed']=True;save_json(work/'results.json',receipt)
    print('module-registry: passed; '+str(work/'results.json'))

if __name__=='__main__':main()
