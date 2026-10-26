#!/usr/bin/env python3
"""Search drivers retain identity, replacement and suspension in saved images."""
import json
import sys
from pathlib import Path
from validation_support import clean_environment, command

prefix = Path(sys.argv[1]).resolve()
work = Path(sys.argv[2]).resolve()
work.mkdir(parents=True, exist_ok=True)
source = work/'search.birnam'
image = work/'search.image'
source.write_text('''
[[|
[Globals at: #SavedSearchItem put: [Object clone]];
[Globals at: #SavedSearchReplacement put: [Object clone]];
[Reflect defineMethod: #replacementSearch: on: SavedSearchReplacement as: [:self :body | SavedSearchReplacement]];
[Globals at: #SavedSearchVector put: #[nil SavedSearchItem #suffix]];
[Globals at: #main put: [|
  [System gc]; let calls := [0 asCell];
  let found := [SavedSearchVector detect: [:each | [calls inc]; [System gc]; true]];
  [System println: [found == SavedSearchItem]];
  [System println: [calls value]];
  [[Reflect vtableOf: SavedSearchVector] addMethod: [SavedSearchReplacement methodFor: #replacementSearch:] as: #detect:];
  [System println: [[SavedSearchVector detect: [:each | true]] == SavedSearchReplacement]];
  let task := [TaskControl start: [:task | [[1 to: 3] detect: [:each | [task wait]]]]];
  [System gc]; [task resolve: [task waitToken] with: false];
  [System gc]; [task resolve: [task waitToken] with: true];
  [System println: [task pollOutcome]]]];
[System saveImage: IMAGE];
[System println: [[System imageSaveReport] status]]
] value]
'''.replace('IMAGE', json.dumps(str(image))))
env = clean_environment()
env['BIRNAM_GC_PRECISE'] = '1'
run = command([prefix/'bin/birnam', source], cwd=work, env=env, timeout=600)
assert run['exit_code'] == 0 and not run['stderr'] and run['stdout'] == '#saved\n', run
source.unlink()
for flags in ([], ['--rebuild']):
    run = command([prefix/'bin/birnam', *flags, image], cwd=work, env=env, timeout=600)
    assert run['exit_code'] == 0 and not run['stderr'], run
    assert run['stdout'] == 'true\n2\ntrue\n#[#success 2]\n', run
print('search images: source-free mapped/rebuilt identity, GC, replacement and suspension passed')
