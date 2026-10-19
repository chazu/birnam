#!/usr/bin/env python3
"""Alien publishes through current Method dictionaries, independently of Store policy."""
from pathlib import Path
import sys
import tempfile
from validation_support import clean_environment, command, save_json, sha

prefix = Path(sys.argv[1]).resolve()
work = Path(tempfile.mkdtemp(prefix='birnam-alien-publication-'))
source = work / 'publication.birnam'
source.write_text('''[[|
let denied := [Error kind: #optionalStoreDenied]; let permit := [false asCell];
[Store setMethodMutationAuthorizer: [| [[permit value] ifTrue: [| true] ifFalse: [| denied]]]];
[System println: [[Alien wrap: "labs" signature: "l l" as: #publicationAbsolute on: Integer] == Integer]];
[System println: [[-7 publicationAbsolute] = 7]];
let behavior := [Reflect vtableOf: Integer];
[System println: [[Store currentValue: behavior attr: #publicationAbsolute] isNil]];
let method := [Execution directMethodFor: #publicationAbsolute in: behavior];
// Explicit Store publications still honor their own authorizer.
[System println: [[Store addMethod: #publicationAbsolute on: behavior with: method] == denied]];
// Explicit descriptions are snapshots until a consumer subscribes to Id changes.
[permit set: true];
[System println: [[Store addMethod: #publicationAbsolute on: behavior with: method] == Store]];
[System println: [[Store currentValue: behavior attr: #publicationAbsolute] == method]];
[System println: [[Alien wrap: "labs" signature: "l l" as: #publicationAbsolute on: Integer] == Integer]];
[System println: [[Store currentValue: behavior attr: #publicationAbsolute] == method]];
[Vtable methodChangesTo: Store];
[System println: [[Alien wrap: "labs" signature: "l l" as: #publicationAbsolute on: Integer] == Integer]];
[System println: [[Store currentValue: behavior attr: #publicationAbsolute] isNil]];
[permit set: false];
// Replace the dictionary publication operation itself, preserving its current sends.
let meta := [Reflect vtableOf: behavior];
let original := [Execution directMethodFor: #addMethod:as: in: meta];
let calls := [0 asCell]; let mode := [#forward asCell];
let failed := [Error kind: #publisherFailure];
let holder := [Object clone];
[Reflect defineMethod: #replacement on: holder as: [:self | 123]];
let replacement := [Execution directMethodFor: #replacement in: [Reflect vtableOf: holder]];
[Reflect defineMethod: #addMethod:as: on: meta as: [:self :value :selector |
  [[selector == #publicationAbsolute] ifTrue: [|
    [calls inc]; [System gc];
    [[[mode value] == #fail] ifTrue: [| failed] ifFalse: [|
      let installed := [original invokeOn: self arguments: #[value selector]];
      [[[mode value] == #replace] ifTrue: [|
        [original invokeOn: self arguments: #[replacement selector]]; [System gc]
      ]];
      installed
    ]]
  ] ifFalse: [| [original invokeOn: self arguments: #[value selector]]]]
]];
[System println: [[Alien wrap: "labs" signature: "l l" as: #publicationAbsolute on: Integer] == Integer]];
[System println: [[calls value] = 1]];
[System println: [[-8 publicationAbsolute] = 8]];
[mode set: #fail];
[System println: [[Alien wrap: "labs" signature: "l l" as: #publicationAbsolute on: Integer] == failed]];
[System println: [[-9 publicationAbsolute] = 9]];
[mode set: #replace];
[System println: [[Alien wrap: "labs" signature: "l l" as: #publicationAbsolute on: Integer] == Integer]];
[System println: [[-10 publicationAbsolute] = 123]];
[System println: [[calls value] = 3]];
[original invokeOn: meta arguments: #[original #addMethod:as:]];
[System gc]
] value]
''')
env = clean_environment(); env['BIRNAM_GC_PRECISE'] = '1'
run = command([prefix/'bin/birnam', source], cwd=work, env=env, timeout=300)
receipt = {'passed': run['exit_code'] == 0 and not run['stderr'] and run['stdout'] == 'true\n'*18,
           'prefix': str(prefix), 'source': source.read_text(),
           'runner_sha256': sha(Path(__file__).resolve()), 'run': run}
save_json(work/'results.json', receipt)
assert receipt['passed'], (work, run)
print('Alien direct publication: passed; ' + str(work/'results.json'))
