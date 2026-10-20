#!/usr/bin/env python3
"""Restore foreign bindings before hash repair and index construction."""
import json
from pathlib import Path
import sys,tempfile
from validation_support import clean_environment,command,save_json,sha
prefix=Path(sys.argv[1]).resolve();work=Path(tempfile.mkdtemp(prefix='birnam-alien-hash-'))
env=clean_environment();env['BIRNAM_GC_PRECISE']='1'
receipt={'passed':False,'prefix':str(prefix),'runner_sha256':sha(Path(__file__).resolve()),'runs':[],'sources':{}}
for mode in ['identity','foreign']:
    source=work/(mode+'.birnam');image=work/(mode+'.image')
    hash_body='[ex optionalWordResultOf: id selector: #identityOf: argument: salt resultFactory: word]'
    prepare=''
    if mode=='foreign':
        prepare='[Alien wrap: "labs" signature: "l l" as: #imageHashAbsolute on: Integer];'
        hash_body='''let bits := [ex optionalWordResultOf: id selector: #identityOf: argument: salt resultFactory: word];
let number := [ex objectResultOf: integers selector: #fromWord: wordArgument: bits];
[ex wordResultOf: [number imageHashAbsolute] selector: #word]'''
    source.write_text('''[[|
let collected := [#[] asCell]; let reflection := Reflect; let mapBehavior := MapVt;
let maps := Map; let globals := Globals; let output := System; let ex := Execution;
let id := Vtable; let word := Word; let integers := Integer;
PREPARE
let visitor := [Object clone];
[Reflect defineMethod: #allocation:bytes:identity: on: visitor as: [ex block: [:self :value :size :identity |
  [[[reflection vtableOf: value] == mapBehavior] ifTrue: [| [collected set: [[collected value] add: value]]]]; nil
] withNativeSignature: "xOww"]];
[Reflect defineMethod: #root: on: visitor as: [ex block: [:self :value | nil] withNativeSignature: "xO"]];
[Reflect defineMethod: #edgeFrom:to: on: visitor as: [ex block: [:self :from :to | nil] withNativeSignature: "xOO"]];
[Reflect defineMethod: #complete: on: visitor as: [ex block: [:self :gaps | nil] withNativeSignature: "vw"]];
[System gc]; [Storage observe: visitor];
let salt := [Object clone];
let hash := [ex block: [:self | HASH] withNativeSignature: "w"];
[reflection defineMethod: #collectionHash on: SymbolVt as: hash];
[[collected value] do: [:map | [maps rehashImage: map]]];
[output println: [[Alien wrap: "labs" signature: "l l" as: #hashOwnerProbe on: Integer] == Integer]];
[globals at: #main put: [| [System println: [-7 hashOwnerProbe]]]]
] value];
// Release the observation visitor and its otherwise unrelated Map references.
[System gc]; [System saveImage: IMAGE];
[System println: [[System imageSaveReport] status]]
'''.replace('PREPARE',prepare).replace('HASH',hash_body).replace('IMAGE',json.dumps(str(image))))
    receipt['sources'][mode]=source.read_text()
    for name,args,expected in [('source',[source],'true\n#saved\n'),('mapped',[image],'7\n'),('rebuilt',['--rebuild',image],'7\n')]:
        r=command([prefix/'bin/birnam',*args],cwd=work,env=env,timeout=600)
        receipt['runs'].append(dict(mode=mode,name=name,**r));save_json(work/'results.json',receipt)
        assert r['exit_code']==0 and not r['stderr'] and r['stdout']==expected,(work,mode,name,r)
        if name=='source':source.unlink()
    image.unlink()
receipt['passed']=True;save_json(work/'results.json',receipt)
print('Alien image hashing: passed; '+str(work/'results.json'))
