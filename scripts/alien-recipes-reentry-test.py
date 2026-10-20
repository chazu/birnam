#!/usr/bin/env python3
"""Reentrant registration and errors through current authored recipe methods."""
from pathlib import Path
import sys,tempfile
from validation_support import clean_environment,command,save_json,sha
prefix=Path(sys.argv[1]).resolve();work=Path(tempfile.mkdtemp(prefix='birnam-alien-reentry-'));source=work/'reentry.birnam'
source.write_text('''[[|
let behavior := [Reflect vtableOf: Integer]; let meta := [Reflect vtableOf: behavior];
let original := [Execution directMethodFor: #addMethod:as: in: meta];
let entered := [false asCell];
[Reflect defineMethod: #addMethod:as: on: meta as: [:self :method :selector |
  let installed := [original invokeOn: self arguments: #[method selector]];
  [[[selector == #nestedCase] & [[entered value] not]] ifTrue: [|
    [entered set: true]; [System gc];
    [Alien wrap: "tolower" signature: "i i" as: #nestedCase on: Integer]
  ]]; installed
]];
[System println: [[Alien wrap: "toupper" signature: "i i" as: #nestedCase on: Integer] == Integer]];
[System println: [[65 nestedCase] = 97]];
[original invokeOn: meta arguments: #[original #addMethod:as:]];
let selected := [[AlienRecipes snapshotRegistrations] select: [:row | [[row at: 2] == #nestedCase]]];
[System println: [[selected size] = 1]];
[System println: [[[selected at: 0] at: 0] = "tolower"]];
// Owned policy errors propagate; already published effects remain published.
let failure := [Error kind: #forcedRecipeFailure];
[Reflect defineMethod: #privateMarker on: failure as: [:self | true]];
let recipe := #["toupper" "i i" #privateInstallFailure Integer [AlienRecipes objectFor: "toupper" signature: "i i"]];
[Reflect defineMethod: #addMethod:as: on: meta as: [:self :method :selector |
  [[selector == #privateInstallFailure] ifTrue: [| failure] ifFalse: [| [original invokeOn: self arguments: #[method selector]]]]
]];
[System println: [[Alien installRecipe: recipe] == failure]];
[System println: [[Execution directMethodFor: #privateInstallFailure in: behavior] isNil]];
[original invokeOn: meta arguments: #[original #addMethod:as:]];
[Reflect defineMethod: #record:method: on: AlienRecipes as: [:self :recipe :method | failure]];
[System println: [[Alien wrap: "labs" signature: "l l" as: #recipeErrorProbe on: Integer] == failure]];
[System println: [[-7 recipeErrorProbe] = 7]];
[Reflect defineMethod: #objectFor:signature: on: AlienRecipes as: [:self :function :signature | failure]];
[System println: [[Alien wrap: "labs" signature: "l l" as: #neverPublished on: Integer] == failure]];
[System println: [[Execution directMethodFor: #neverPublished in: [Reflect vtableOf: Integer]] isNil]]
] value]
''')
env=clean_environment();env['BIRNAM_GC_PRECISE']='1'
r=command([prefix/'bin/birnam',source],cwd=work,env=env,timeout=300)
receipt={'passed':r['exit_code']==0 and not r['stderr'] and r['stdout']=='true\n'*10,'prefix':str(prefix),'source':source.read_text(),'runner_sha256':sha(Path(__file__).resolve()),'run':r}
save_json(work/'results.json',receipt);assert receipt['passed'],(work,r)
print('Alien recipe reentry: passed; '+str(work/'results.json'))
