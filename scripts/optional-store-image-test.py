#!/usr/bin/env python3
"""Fresh CLI and worker images activate only their declared local providers."""
from pathlib import Path
import json
import sys
import tempfile
from validation_support import ROOT, clean_environment, command, save_json, sha
prefix=Path(sys.argv[1]).resolve();work=Path(tempfile.mkdtemp(prefix='birnam-optional-image-'));image=work/'optional.image';resaved=work/'resaved.image';runs=[]
def run(name,argv,expected):
 r=command(argv,env=clean_environment(),cwd=work,timeout=900);(work/(name+'.log')).write_text(r['output']);runs.append(dict(name=name,**r));save_json(work/'results.json',dict(passed=False,runs=runs));assert r['exit_code']==0 and not r['stderr'] and r['stdout']==expected,(name,work,r['output'][-2000:])
source=work/'save.birnam';source.write_text('''[Module load: #relational];
[Store assert: #imageValue attr: #history value: true];
[Store assert: #item attr: #imageValue value: 17]; let earlier := [Store basis];
[Store assert: #item attr: #imageValue value: 18];
let StoreHandler := [Object clone];
[Reflect defineMethod: #query: on: StoreHandler as: [:self :value |
 [Store findAll: ?v where: #[#[#item #imageValue ?v]]]
]];
[Globals at: #main put: [|
 [System gc];
 [System println: [Store valueOf: #item attr: #imageValue asOf: earlier]];
 [System println: [StoreHandler query: nil]];
 [System println: [Module imageProviders]];
 [System saveImage: RESAVED]
]];
[System saveImage: IMAGE]; [System println: [[System imageSaveReport] status]]
'''.replace('RESAVED',json.dumps(str(resaved))).replace('IMAGE',json.dumps(str(image))))
run('save',[prefix/'bin/birnam',source],'#saved\n');source.unlink()
for name,flags,path in [('mapped',[],image),('rebuilt',['--rebuild'],image),('resaved',[],resaved)]:
 run(name,[prefix/'bin/birnam',*flags,path],'17\n#[18]\n#[#store #relational]\n')
worker=work/'worker.birnam';worker.write_text('''
[System println: [Namespace includesKey: #Store]];
let worker := [Isolate start: #StoreHandler selector: #query: image: IMAGE];
[System println: [worker state]]; [System println: [worker request: nil]];
[worker stop]; [System println: [worker state]];
[System println: [Namespace includesKey: #Store]]
'''.replace('IMAGE',json.dumps(str(image))))
run('worker',[prefix/'bin/birnam',worker],'false\n#ready\n#[18]\n#exited\nfalse\n')
result=dict(passed=True,prefix=str(prefix),runs=runs,image_sha256=sha(image),resaved_sha256=sha(resaved),runner_sha256=sha(Path(__file__).resolve()));save_json(work/'results.json',result);print('optional-store-image: passed; '+str(work/'results.json'))
