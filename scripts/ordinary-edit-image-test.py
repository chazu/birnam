#!/usr/bin/env python3
"""Ordinary edits while running, no method facts, and edited source-free images."""
import argparse,json,tempfile
from pathlib import Path
from validation_support import clean_environment,command,save_json,sha
p=argparse.ArgumentParser(description=__doc__);p.add_argument('prefix',type=Path);p.add_argument('--mode',choices=['both','managed','unmanaged'],default='both');a=p.parse_args()
prefix=a.prefix.resolve();work=Path(tempfile.mkdtemp(prefix='birnam-ordinary-edit-'));env=clean_environment();env['BIRNAM_GC_PRECISE']='1'
receipt=dict(passed=False,prefix=str(prefix),runs=[],source_sha256=sha(Path(__file__).resolve()))
def run(name,argv,expected):
 r=command(argv,cwd=work,env=env,timeout=300);receipt['runs'].append(dict(name=name,**r));save_json(work/'results.json',receipt)
 assert r['exit_code']==0 and not r['stderr'] and r['stdout']==expected,(work,name,r)
 return r
for managed in ([False,True] if a.mode=='both' else [a.mode=='managed']):
 tag='managed' if managed else 'unmanaged';module=work/(tag+'-module.birnam');writer=work/(tag+'-writer.birnam');initial=work/(tag+'-initial.image');edited=work/(tag+'-edited.image')
 module.write_text('[Module named: #ordinaryEditFixture requires: #[]]; [Reflect defineMethod: #editableValue on: Integer as: [:self | 41]]\n')
 writer.write_text('''
[System load: MODULE];
ADOPT
[Globals at: #main put: [|
  [System println: [[DispatchGeneration current] isNil]];
  let old := [3 methodFor: #editableValue];
  let basis := [db basis];
  let result := [Reflect defineMethod: #editableValue on: Integer as: [:self | 99]];
  [[result isError] ifTrue: [| [System println: [result kind]]]];
  [System println: [result == Integer]];
  [System println: [3 editableValue]];
  [System println: [old invokeOn: 3 arguments: #[]]];
  [Reflect defineMethod: #temporaryEditProbe on: Integer as: [:self | 47]];
  [System println: [[Reflect removeMethod: #temporaryEditProbe on: Integer] == Integer]];
  [System println: [[db basis] = basis]];
  [Globals at: #main put: [|
    [System println: [3 editableValue]];
    [System println: [[DispatchGeneration current] isNil]];
    RESTORED_STATUS
    let fresh := [Reflect defineMethod: #editableValue on: Integer as: [:self | 111]];
    [System println: [fresh == Integer]];
    [System println: [3 editableValue]]
  ]];
  [System saveImage: EDITED];
  [System println: [[System imageSaveReport] status]];
  SAVED_STATUS
]];
[System saveImage: INITIAL]; [System println: [[System imageSaveReport] status]]
'''.replace('MODULE',json.dumps(str(module))).replace('ADOPT','[DispatchGeneration adoptCurrent];' if managed else '').replace('RESTORED_STATUS','[System println: [[DispatchGeneration current] state]]; [System println: [[Generation validateImageGeneration] isError]];' if managed else '').replace('SAVED_STATUS','[System println: [[DispatchGeneration current] state]]' if managed else 'nil').replace('EDITED',json.dumps(str(edited))).replace('INITIAL',json.dumps(str(initial))))
 run(tag+'-save',[prefix/'bin/birnam',writer],'#saved\n');writer.unlink();module.unlink()
 run(tag+'-edit',[prefix/'bin/birnam',initial],('false' if managed else 'true')+'\ntrue\n99\n41\ntrue\ntrue\n#saved\n'+('nil\n' if managed else ''))
 assert edited.exists()
 expected='99\n'+('false\nnil\ntrue\n' if managed else 'true\n')+'true\n111\n'
 for mode,flags in [('mapped',[]),('rebuilt',['--rebuild'])]:run(tag+'-'+mode,[prefix/'bin/birnam',*flags,edited],expected)
 if managed:
  resaver=work/'resaver.birnam';resaved=work/'resaved.image'
  resaver.write_text('[System loadImage: '+json.dumps(str(edited))+']; [System saveImage: '+json.dumps(str(resaved))+']; [System println: [[System imageSaveReport] status]]\n')
  run('managed-resave',[prefix/'bin/birnam',resaver],'#saved\n');resaver.unlink()
  run('managed-resaved-load',[prefix/'bin/birnam',resaved],expected)

receipt['passed']=True;save_json(work/'results.json',receipt);print('ordinary-edit images passed:',work/'results.json')
