#!/usr/bin/env python3
"""Optional facts, explicit opt-out, current errors and mismatched Cell refusal."""
from pathlib import Path
import json
import sys
import tempfile
from validation_support import ROOT, clean_environment, command, save_json, sha
prefix=Path(sys.argv[1]).resolve();work=Path(tempfile.mkdtemp(prefix='birnam-image-store-'));image=work/'core.image'
receipt=dict(passed=False,runs=[],prefix=str(prefix),source_sha256={str(p.relative_to(ROOT)):sha(p) for p in [Path(__file__).resolve(),ROOT/'modules/image-format.birnam',ROOT/'modules/store-image.birnam',ROOT/'rust/birnam-image/src/native.rs']})
def run(name,source,expected):
 p=work/(name+'.birnam');p.write_text(source.replace('IMAGE',json.dumps(str(image))))
 r=command([prefix/'bin/birnam',p],env=clean_environment(),timeout=180);(work/(name+'.log')).write_text(r['output']);receipt['runs'].append(dict(name=name,source=source,**r));save_json(work/'results.json',receipt)
 assert r['exit_code']==0 and not r['stderr'] and r['stdout']==expected,(name,work,r)
run('save-core','''
[Reflect defineMethod: #imageStore on: System as: [:self | nil]];
[Reflect defineMethod: #basis on: Store as: [:self | [Error kind: #unexpectedStoreRead]]];
[Reflect defineMethod: #imageFactsInto: on: Store as: [:self :sink | [Error kind: #unexpectedFacts]]];
[Reflect defineMethod: #new on: Store as: [:self | [Error kind: #unexpectedStoreStage]]];
[Reflect defineMethod: #swapImageStateWith: on: Store as: [:self :other | [Error kind: #unexpectedSwap]]];
[Globals at: #coreSavedValue put: 42];
[System println: [System saveImage: IMAGE]];
[System println: [[System imageSaveReport] status]]
''','true\n#saved\n')
for mode in ['false','true']:
 run('load-core-'+mode,'''
[db assert: #destinationOnly attr: #value value: 73];
[System println: [System loadImage: IMAGE rebuild: MODE]];
[System gc];
[System println: [db currentValue: #destinationOnly attr: #value]];
[System println: coreSavedValue];
[System println: [System imageStore]]
'''.replace('MODE',mode),('false' if mode=='true' else 'true')+'\n73\n42\nnil\n')
original=sha(image)
run('mismatched-cell','''
let other := [Store new]; let handle := [other cellFor: #other attr: #value];
[handle set: 9]; [Globals at: #unselectedCell put: handle];
[System println: [System saveImage: IMAGE]];
[System println: [[System imageSaveReport] status]]
''','nil\n#rejected\n');assert sha(image)==original
run('selection-error','''
let refusal := [Error kind: #imageOwnerRefusal];
[Reflect defineMethod: #refusalIdentity on: refusal as: [:self | self]];
[Reflect defineMethod: #imageStore on: System as: [:self | [System gc]; refusal]];
[System println: [[System saveImage: IMAGE] == refusal]];
[System println: [[System imageSaveReport] status]]
''','true\n#rejected\n');assert sha(image)==original
receipt.update(passed=True,image_sha256=sha(image));save_json(work/'results.json',receipt);print('image-store-participation: passed; '+str(work/'results.json'))
