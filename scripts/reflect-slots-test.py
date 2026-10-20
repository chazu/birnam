#!/usr/bin/env python3
"""Slot reflection/retention and source-free Module directory compatibility."""
import argparse
import json
from pathlib import Path
import tempfile
from validation_support import ROOT, clean_environment, command, save_json, sha


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('prefix',type=Path)
    p.add_argument('--legacy-prefix',type=Path)
    p.add_argument('--skip-images',action='store_true')
    args=p.parse_args(); prefix=args.prefix.resolve()
    work=Path(tempfile.mkdtemp(prefix='birnam-reflect-slots-'))
    env=clean_environment(); env['BIRNAM_GC_PRECISE']='1'
    receipt={'passed':False,'prefix':str(prefix),'runs':[], 'source_sha256':{str(f.relative_to(ROOT)):sha(f) for f in [
        Path(__file__).resolve(),ROOT/'tests/fixtures/reflect-slots.birnam',ROOT/'modules/stdlib.birnam',ROOT/'modules/modules.birnam']}}
    def run(name,argv,expected=None):
        r=command(argv,cwd=work,env=env,timeout=240)
        receipt['runs'].append(dict(name=name,**r));save_json(work/'results.json',receipt)
        (work/(name+'.log')).write_text(r['output'])
        assert r['exit_code']==0 and not r['stderr'],(name,work,r)
        if expected is not None: assert r['stdout']==expected,(name,work,r)
        return r
    result=run('retention',[prefix/'bin/birnam',ROOT/'tests/fixtures/reflect-slots.birnam'])
    rows=json.loads(result['stdout']);assert rows['passed']
    control=rows['batches'][0]['object_delta']
    assert [r['calls'] for r in rows['batches']]==[0,1,30,100]
    assert all(r['fact_delta']==0 and r['object_delta']<=control+4 for r in rows['batches']),rows
    receipt['retention']=rows
    if not args.skip_images:
        for label,writer in [('new',prefix)]+([('legacy',args.legacy_prefix.resolve())] if args.legacy_prefix else []):
            source=work/(label+'.birnam');image=work/(label+'.image')
            source.write_text('''[Module load: #store];

[[|
let manager := [Module clone]; [manager libDir: "/captured"];
let alias := manager;
[Globals at: #main put: [|
  [System gc];
  [System println: [manager == alias]];
  [System println: [manager libDir]];
  [alias libDir: "/changed"]; [System gc]; [System println: [manager libDir]];
  [System println: [[Module libDir] = LIBRARY]];
  CHECK
]];
[Module libDir: "/old-install/lib"];
[System saveImage: IMAGE]; [System println: [[System imageSaveReport] status]]
] value]
'''.replace('LIBRARY',json.dumps(str(prefix/'lib'))).replace('IMAGE',json.dumps(str(image))).replace('CHECK',
 '''[System println: [[[Reflect slotsOf: manager] at: 0] at: #name]];
 [Reflect slot: #libDir in: manager put: "/reflected"];
 [System println: [manager libDir]];
 [System println: [[db currentValue: manager attr: #libDir] isNil]]'''
 if label=='new' else '''[System println: [[db currentValue: manager attr: #libDir] = "/changed"]]'''))
            receipt[label+'_source']=source.read_text()
            run(label+'-save',[writer/'bin/birnam',source],'#saved\n');source.unlink()
            expected='true\n/captured\n/changed\ntrue\n'+('#libDir\n/reflected\ntrue\n' if label=='new' else 'true\n')
            for mode,flags in [('mapped',[]),('rebuilt',['--rebuild'])]:
                run(label+'-'+mode,[prefix/'bin/birnam',*flags,image],expected)
            receipt[label+'_image_sha256']=sha(image)
    receipt['passed']=True;save_json(work/'results.json',receipt)
    print('reflect-slots: passed; '+str(work/'results.json'))

if __name__=='__main__': main()
