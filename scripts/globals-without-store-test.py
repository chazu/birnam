#!/usr/bin/env python3
"""Export authored Globals and their actual captures; run with no Store/Relational."""
import json
from pathlib import Path
import shutil
import sys
import tempfile
from validation_support import ROOT, clean_environment, command, save_json, sha

prefix=Path(sys.argv[1]).resolve();work=Path(tempfile.mkdtemp(prefix='birnam-globals-floor-'))
programs=work/'programs';programs.mkdir();floor=work/'floor';floor.mkdir()
selectors=[('bindingOwner',0),('bindingState',0),('publishBindingState:from:',2),('at:',1),('includes:',1),('at:ifAbsent:',2),('bindings',0),('bindingNames',0),('bindingBasis',0),('bindingAt:',1),('bindingMutation:',1),('at:put:',2),('map:without:',2),('order:without:',2),('remove:',1),('liveBindingsDo:',1),('nameOf:',1),('reservedBindingCompensationFor:',1),('reserveBindingCompensationFor:count:',2),('releaseBindingCompensationFor:',1),('restoreName:in:from:',3),('restoreBinding:expected:previous:reservation:',4),('imageBindingCheckpoint',0),('restoreImageBindings:',1),('releaseImageReservations',0),('transientBinding:put:',2),('bindingIsTransient:',1),('imageBindingProjection',0)]
requests=[('Globals','globals',s,n) for s,n in selectors]+[
 ('Object','object','isError',0),('Object','object','bindingIdentity',0),('Object','object','bindingKey',0),
 ('SymbolVt','symbolBehavior','bindingKey',0),('keyPrototype','keyPrototype','bindingIdentity',0),
 ('keyPrototype','keyPrototype','collectionHash',0),('keyPrototype','keyPrototype','collectionEquals:',1),
 ('keyPrototype','keyPrototype','slotNames',0),('storage','storage','slotNames',0),
 ('Error','error','kind:',1),('Error','error','kind:message:',2),
 ('ErrorPrototype','errorPrototype','isError',0),('ErrorPrototype','errorPrototype','kind',0)]
source=work/'export.birnam';source.write_text('''[Module load: #json];
[[|
let storage := [Globals bindingOwner];
let probe := [[Object clone] bindingKey];
let keyPrototype := [Vtable ownerOf: [Vtable parentOf: [Reflect vtableOf: probe]]];
let labels := #[#[ObjectModel "model"] #[Reflect "reflect"] #[Map "map"] #[nil "nil"]
  #[Error "error"] #[ErrorPrototype "errorPrototype"] #[storage "storage"] #[keyPrototype "keyPrototype"]];
let requests := REQUESTS;
let rows := [[0 to: [[requests size] - 1]] collect: [:index |
  let request := [requests at: index];
  let row := [[Reflect directMethodsOf: [request at: 0]] detect: [:row | [[row at: #selector] == [request at: 2]]]];
  let block := [row at: #method];
  let captures := [[0 to: [[block captureCount] - 1]] collect: [:i |
    let value := [block captureAt: i];
    let label := [labels detect: [:pair | [[pair at: 0] == value]]];
    [[label isNil] ifTrue: [|
      [[[Reflect vtableOf: value] == WordVt] ifTrue: [| "word0"] ifFalse: [|
        [[[Reflect vtableOf: value] == IntegerVt] ifTrue: [| ["index" , [value printString]]] ifFalse: [| "UNKNOWN"]]
      ]]
    ] ifFalse: [| [label at: 1]]]
  ]];
  let signature := [[block codeObject] nativeSignature];
  let spelling := [[signature isNil] ifTrue: [| ""] ifFalse: [| [signature asString]]];
  let bytes := [CodeObject canonicalBytesOf: [block codeObject]];
  [System writeFile: [[DIRECTORY , [index printString]] , ".bin"] contents: [bytes asString]];
  {"target" -> [request at: 1] "selector" -> [Reflect nameOf: [request at: 2]] "arity" -> [request at: 3] "captures" -> captures "signature" -> spelling}
]];
[System println: [Json stringify: rows]]
] value]
'''.replace('DIRECTORY',json.dumps(str(programs)+'/')).replace('REQUESTS','#[ '+' '.join(f'#[{obj} "{label}" #{sel} {arity}]' for obj,label,sel,arity in requests)+' ]'))
env=clean_environment();result=command([prefix/'bin/birnam',source],env=env,timeout=120)
(work/'export.log').write_text(result['output']);assert result['exit_code']==0 and not result['stderr'],(work,result)
plan=json.loads(result['stdout']);assert all('UNKNOWN' not in row['captures'] for row in plan),(work,plan)
save_json(programs/'plan.json',plan)
for name in 'id root_registry namespace boolean nil collect object_model word bytes execution portable_code string integer vector map tcc native_backend codegen_runtime'.split():
 shutil.copy2(prefix/'lib'/f'libbirnam_{name}.dylib',floor)
env.update(BIRNAM_GLOBAL_PROGRAMS=str(programs),BIRNAM_TEST_MODULE_DIRECTORY=str(floor),CARGO_TARGET_DIR=str(ROOT/'.cargo-target/native-message-tests'),CARGO_PROFILE_DEV_PANIC='unwind')
run=command(['mise','exec','--','cargo','test','--offline','--locked','-p','birnam-siphon','--test','globals_without_store','--','--include-ignored'],env=env,timeout=240)
(work/'run.log').write_text(run['output']);receipt={'passed':run['exit_code']==0,'export':result,'run':run,'plan':plan,'source_sha256':{str(p.relative_to(ROOT)):sha(p) for p in [ROOT/'modules/globals.birnam',ROOT/'modules/error.birnam',ROOT/'rust/birnam-siphon/tests/globals_without_store.rs',Path(__file__).resolve()]},'program_sha256':{p.name:sha(p) for p in programs.iterdir()},'floor_sha256':{p.name:sha(p) for p in floor.iterdir()},'scope':'Actual authored bodies with equivalent owner captures; raw bootstrap method installation, no Store, Relational, Lang, Bootstrap or Generation. Not full Store-free application startup.'}
save_json(work/'results.json',receipt);assert receipt['passed'],work
print('globals-without-store: passed; '+str(work/'results.json'))
