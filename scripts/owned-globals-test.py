#!/usr/bin/env python3
"""Owned binding retention and source-free new/legacy image compatibility."""
import argparse
import json
from pathlib import Path
import tempfile
from validation_support import ROOT, clean_environment, command, save_json, sha
p=argparse.ArgumentParser(description=__doc__);p.add_argument('prefix',type=Path);p.add_argument('--legacy-prefix',type=Path);p.add_argument('--skip-images',action='store_true');a=p.parse_args()
prefix=a.prefix.resolve();work=Path(tempfile.mkdtemp(prefix='birnam-owned-globals-'));env=clean_environment();env['BIRNAM_GC_PRECISE']='1'
receipt={'passed':False,'prefix':str(prefix),'runs':[],'sources':{str(f.relative_to(ROOT)):sha(f) for f in [ROOT/'modules/globals.birnam',ROOT/'modules/globals-owner.birnam',ROOT/'tests/fixtures/owned-globals.birnam',Path(__file__).resolve()]}}
def run(name,args,expected=None):
 r=command(args,cwd=work,env=env,timeout=300);(work/(name+'.log')).write_text(r['output']);receipt['runs'].append(dict(name=name,**r));save_json(work/'results.json',receipt)
 assert r['exit_code']==0 and not r['stderr'],(work,name,r)
 if expected is not None:assert r['stdout']==expected,(work,name,r)
 return r
rows=json.loads(run('retention',[prefix/'bin/birnam',ROOT/'tests/fixtures/owned-globals.birnam'])['stdout']);control=rows['batches'][0]['object_delta'];assert rows['passed'] and all(r['fact_delta']==0 and r['object_delta']<=control+32 for r in rows['batches']),rows;receipt['retention']=rows;receipt['retention_scope']='Language telemetry itself allocates and may take different admitted paths after GC; allow bounded fixed telemetry overhead. Native owner and Store-absent tests independently assert that removed key/value objects are actually reclaimed.'
if not a.skip_images:
 for name,writer in [('new',prefix)]+([('legacy',a.legacy_prefix.resolve())] if a.legacy_prefix else []):
  src=work/(name+'.birnam');image=work/(name+'.image')
  src.write_text('''[[|
let left := ["same" , " key"]; let right := ["same " , "key"];
let shared := [Object clone];
[Globals at: left put: #left]; [Globals at: right put: #right];
[Globals at: #savedAliasA put: shared]; [Globals at: #savedAliasB put: shared];
[Globals at: #savedNil put: nil]; [Globals at: #savedValue put: 41];
[db assert: #__globals__ attr: #savedValue value: 99];
[Globals at: #main put: [|
 [System gc];
 [System println: [Globals at: #savedValue]];
 [System println: [[Globals at: #savedAliasA] == [Globals at: #savedAliasB]]];
 [System println: [Globals includes: #savedNil]];
 [System println: [[Globals at: #savedNil ifAbsent: [| #bad]] isNil]];
 [System println: [Globals at: left]]; [System println: [Globals at: right]];
 [System println: [db currentValue: #__globals__ attr: #savedValue]];
 [Globals at: #savedValue put: 42]; [System gc];
 [System println: [Globals at: #savedValue]];
 [System println: [db currentValue: #__globals__ attr: #savedValue]]
]];
[System saveImage: IMAGE]; [System println: [[System imageSaveReport] status]]
] value]
'''.replace('IMAGE',json.dumps(str(image))))
  receipt[name+'_source']=src.read_text();run(name+'-save',[writer/'bin/birnam',src],'#saved\n');src.unlink()
  expected=('41' if name=='new' else '99')+'\ntrue\ntrue\ntrue\n#left\n#right\n99\n'+('42\n99\n' if name=='new' else '42\n42\n')
  for label,flags in [('mapped',[]),('rebuilt',['--rebuild'])]:run(name+'-'+label,[prefix/'bin/birnam',*flags,image],expected)
  receipt[name+'_image_sha256']=sha(image)
receipt['passed']=True;save_json(work/'results.json',receipt);print('owned-globals: passed; '+str(work/'results.json'))
