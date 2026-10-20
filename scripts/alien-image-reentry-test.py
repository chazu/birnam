#!/usr/bin/env python3
"""A later wrapper installed by an image callback remains authoritative on resave."""
import json
from pathlib import Path
import sys,tempfile
from validation_support import clean_environment,command,save_json,sha
prefix=Path(sys.argv[1]).resolve();work=Path(tempfile.mkdtemp(prefix='birnam-alien-image-reentry-'));source=work/'save.birnam';first=work/'first.image';next_image=work/'next.image'
source.write_text('''[Alien wrap: "toupper" signature: "i i" as: #imageNestedProbe on: Integer];
[Alien wrap: "tolower" signature: "i i" as: #lowerCacheOnly on: Integer];
[Reflect removeMethod: #lowerCacheOnly on: Integer];
[Reflect defineMethod: #installRecipe: on: Alien as: [:self :recipe |
  let selector := [recipe at: 2]; let target := [recipe at: 3];
  let installed := [self wrap: [recipe at: 0] signature: [recipe at: 1] as: selector on: target];
  [[installed isError] ifTrue: [| installed] ifFalse: [|
    let method := [Execution directMethodFor: selector in: [Reflect vtableOf: target]];
    [[selector == #imageNestedProbe] ifTrue: [|
      [self wrap: "tolower" signature: "i i" as: selector on: target]; [System gc]
    ]]; method
  ]]
]];
[Globals at: #ImageNestedCount put: [0 asCell]];
[Globals at: #main put: [|
  [System println: [[65 imageNestedProbe] = 97]];
  let rows := [[AlienRecipes snapshotRegistrations] select: [:row | [[row at: 2] == #imageNestedProbe]]];
  [System println: [[rows size] = 1]];
  [System println: [[[rows at: 0] at: 0] = "tolower"]];
  [ImageNestedCount inc];
  [[[ImageNestedCount value] = 1] ifTrue: [| [System saveImage: NEXT]]]
]];
[System gc]; [System saveImage: FIRST]; [System println: [[System imageSaveReport] status]]
'''.replace('FIRST',json.dumps(str(first))).replace('NEXT',json.dumps(str(next_image))))
env=clean_environment();env['BIRNAM_GC_PRECISE']='1';receipt={'passed':False,'prefix':str(prefix),'source':source.read_text(),'runner_sha256':sha(Path(__file__).resolve()),'runs':[]}
for name,args,expected in [('source',[source],'#saved\n'),('mapped',[first],'true\n'*3),('rebuilt-resave',['--rebuild',next_image],'true\n'*3)]:
 r=command([prefix/'bin/birnam',*args],cwd=work,env=env,timeout=600);receipt['runs'].append(dict(name=name,**r));save_json(work/'results.json',receipt)
 assert r['exit_code']==0 and not r['stderr'] and r['stdout']==expected,(work,r)
 if name=='source':source.unlink()
receipt['passed']=True;save_json(work/'results.json',receipt);first.unlink();next_image.unlink()
print('Alien image reentry: passed; '+str(work/'results.json'))
