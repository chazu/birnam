#!/usr/bin/env python3
"""Owner payloads preserve identity, failures and source-free image round trips."""
import json
from pathlib import Path
import sys
import tempfile
from validation_support import ROOT, clean_environment, command, save_json, sha

prefix=Path(sys.argv[1]).resolve()
work=Path(tempfile.mkdtemp(prefix='birnam-image-owners-'))
env=clean_environment();env['BIRNAM_GC_PRECISE']='1'
image=work/'owners.image';resave=work/'resaved.image';refused=work/'refused.image'
refused.write_bytes(b'keep the existing destination')
source=work/'owners.birnam'
source.write_text('''[[|
let owner := [ObjectModel newSlots: #[#live nil] parent: ObjectVt];
[ObjectModel slot: 1 in: owner put: owner];
[Reflect defineMethod: #imagePayload on: owner as: [:self |
  [ObjectModel newSlots: #[#saved self] parent: ObjectVt]
]];
[Globals at: #PayloadLeft put: owner]; [Globals at: #PayloadRight put: owner];
[Globals at: #PayloadError put: [Error kind: #ordinarySavedError]];
[Globals at: #PayloadOmitted put: #live];
[Globals transientBinding: #PayloadOmitted put: true];
[Globals at: #PayloadBindingCopy put: [[Globals bindingOwner] clone]];
[Globals at: #main put: [|
  [System gc];
  [System println: [PayloadLeft == PayloadRight]];
  [System println: [[ObjectModel slot: 0 in: PayloadLeft] == #saved]];
  [System println: [[ObjectModel slot: 1 in: PayloadLeft] == PayloadLeft]];
  [System println: [[PayloadError kind] == #ordinarySavedError]];
  [System println: [[Globals at: #PayloadOmitted] isNil]];
  let snapshot := [ObjectModel slot: 0 in: PayloadBindingCopy];
  [System println: [[[snapshot at: 0] at: #PayloadOmitted] == #live]];
  [System saveImage: RESAVE];
  [System println: [[System imageSaveReport] status]]
]];
[System saveImage: IMAGE]; [System println: [[System imageSaveReport] status]];
[System println: [[ObjectModel slot: 0 in: owner] == #live]];
[System println: [[Globals at: #PayloadOmitted] == #live]];
[Reflect defineMethod: #imagePayload on: owner as: [:self | false]];
[System saveImage: REFUSED]; [System println: [[System imageSaveReport] status]];
[Reflect defineMethod: #imagePayload on: owner as: [:self | [Error kind: #projectionFailure]]];
[System println: [[System saveImage: REFUSED] kind]];
[System println: [[System imageSaveReport] status]];
[Reflect defineMethod: #imagePayload on: owner as: [:self | self]];
let native := ["native" , " representation"];
[Reflect defineMethod: #imagePayload on: native as: [:self | [Object clone]]];
[Globals at: #InvalidNativePayload put: native];
[System saveImage: REFUSED]; [System println: [[System imageSaveReport] status]]
] value]
'''.replace('RESAVE',json.dumps(str(resave))).replace('IMAGE',json.dumps(str(image))).replace('REFUSED',json.dumps(str(refused))))
receipt={'passed':False,'prefix':str(prefix),'source':source.read_text(),'runner_sha256':sha(Path(__file__).resolve()),'runs':[]}
def run(name,args,expected):
    r=command([prefix/'bin/birnam',*args],cwd=work,env=env,timeout=900)
    receipt['runs'].append(dict(name=name,**r));save_json(work/'results.json',receipt)
    (work/(name+'.log')).write_text(r['output'])
    assert r['exit_code']==0 and not r['stderr'] and r['stdout']==expected,(work,name,r)
run('save',[source],'#saved\ntrue\ntrue\n#rejected\n#projectionFailure\n#rejected\n#rejected\n')
assert refused.read_bytes()==b'keep the existing destination'
source.unlink()
expected='true\n'*6+'#saved\n'
run('mapped',[image],expected)
run('resaved',[resave],expected)
run('rebuilt',['--rebuild',image],expected)
receipt['passed']=True;receipt['image_sha256']=sha(image);receipt['resaved_sha256']=sha(resave)
image.unlink();resave.unlink();receipt['image_artifacts_cleaned']=True
save_json(work/'results.json',receipt)
print('image owners: passed; '+str(work/'results.json'))
