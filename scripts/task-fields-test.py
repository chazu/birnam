#!/usr/bin/env python3
"""Owned task fields: aliases, collection, live accessors and image reset policy."""
import json
from pathlib import Path
import sys
import tempfile
from validation_support import clean_environment, command, save_json, sha

prefix = Path(sys.argv[1]).resolve()
work = Path(tempfile.mkdtemp(prefix='birnam-task-fields-'))
env = clean_environment()
env['BIRNAM_GC_PRECISE'] = '1'
image = work / 'fields.image'
source = work / 'fields.birnam'
source.write_text('''[[|
let factory := [TaskControl clone];
let left := [factory field: #cancelReason];
let right := [factory field: #cancelReason];
[System println: [[left == right] not]];
let value := [Object clone];
[System println: [[left set: value] == left]];
[System gc];
[System println: [[right value] == value]];
[System println: [[factory cancelReason] == value]];
[factory cancelReason: #changed];
[System println: [[left value] == #changed]];
[System println: [[left replace: value with: #wrong] == false]];
[System println: [[left replace: #changed with: #replaced] == true]];
[System println: [[right value] == #replaced]];
let copy := [factory clone];
[copy cancelReason: #copy];
[System println: [[factory cancelReason] == #replaced]];
[System println: [[[factory field: #unknown] kind] == #unknownSlot]];
let retained := [[| let owner := [TaskControl clone]; [owner field: #cancelReason]] value];
[System gc]; [retained set: #retained]; [System gc];
[System println: [[retained value] == #retained]];
let worker := [Worker fromHandle: #old];
[worker workerHandle: #new];
[System println: [[worker replaceWorkerHandle: #old with: nil] == false]];
[System println: [[worker workerHandle] == #new]];
// An ordinary accessor replacement affects the actual task algorithm.
let dirtyFactory := [TaskControl clone];
[dirtyFactory requestWorker: #oldWorker]; [dirtyFactory requestId: #oldId];
[dirtyFactory externalResult: #oldResult];
let task := [dirtyFactory prepare: [:self | 42] parent: nil];
[System println: [[task requestWorker] isNil]];
[System println: [[task requestId] isNil]];
[System println: [[task externalResult] isNil]];
[Reflect defineMethod: #shieldDepth on: task as: [:self | 40]];
[task shield];
[System println: [[[task field: #shieldDepth] value] = 41]];
[task unshield];
[System println: [[[task field: #shieldDepth] value] = 39]];
let executor := [TaskExecutor clone];
[executor ingress: #[]];
let watched := [executor watch: factory];
[System println: [[watched value] = #[factory]]];
[System println: [[[executor field: #ingress] value] == [watched value]]];
let failure := [RemoteFailure fromOutcome: #[#failure #[#test] nil #[]]];
[System gc];
[System println: [[failure outcome] = #[#failure #[#test] nil #[]]]];
[System println: [[failure kind] == #remoteFailure]];
[Globals at: #SavedTaskField put: left];
[Globals at: #SavedTaskFieldAlias put: right];
[Globals at: #SavedTaskFactory put: factory];
[Globals at: #SavedExecutorField put: [executor field: #quantum]];
[executor quantum: 999];
[Globals at: #SavedExecutor put: executor];
[Globals at: #SavedWorker put: worker];
[worker activeRequest: factory]; [worker closing: true];
[Globals at: #SavedWorkerField put: [worker field: #activeRequest]];
[Globals at: #main put: [|
  [System gc];
  [System println: [[SavedTaskField value] == #replaced]];
  [SavedTaskField set: #restored];
  [System println: [[SavedTaskFieldAlias value] == #restored]];
  [System println: [[SavedTaskFactory cancelReason] == #restored]];
  [System println: [[SavedExecutorField value] isNil]];
  [System println: [[SavedExecutor quantum] = 128]];
  [System println: [[SavedExecutorField value] = 128]];
  [System println: [[SavedWorker workerHandle] == #new]];
  [System println: [[SavedWorkerField value] isNil]];
  [System println: [[SavedWorker closing] isNil]]
]]
] value];
// The prepared task above is no longer rooted when the image is collected.
[System gc];
[System saveImage: IMAGE];
[System println: [[System imageSaveReport] status]]
'''.replace('IMAGE', json.dumps(str(image))))
receipt = {'passed': False, 'prefix': str(prefix), 'source': source.read_text(),
           'runner_sha256': sha(Path(__file__).resolve()), 'runs': []}
for name, args, expected in [('source', [source], 'true\n' * 22 + '#saved\n'),
                             ('mapped', [image], 'true\n' * 9),
                             ('rebuilt', ['--rebuild', image], 'true\n' * 9)]:
    run = command([prefix / 'bin/birnam', *args], cwd=work, env=env, timeout=600)
    receipt['runs'].append(dict(name=name, **run))
    (work / (name + '.log')).write_text(run['output'])
    save_json(work / 'results.json', receipt)
    assert run['exit_code'] == 0 and not run['stderr'] and run['stdout'] == expected, (work, name, run)
    if name == 'source':
        source.unlink()
# A view must not turn a completed process-local task into an imageable factory.
refused = work / 'refused.image'
refused.write_bytes(b'preserve existing destination')
reject = work / 'retained-task-field.birnam'
reject.write_text('''[Globals at: #RetainedTaskField put: [[TaskControl start: [:self | 42]] field: #outcome]];
[System gc];
[System saveImage: DESTINATION];
[System println: [[System imageSaveReport] status]]
'''.replace('DESTINATION', json.dumps(str(refused))))
run = command([prefix / 'bin/birnam', reject], cwd=work, env=env, timeout=120)
receipt['runs'].append(dict(name='retained-task-field-rejected', **run))
save_json(work / 'results.json', receipt)
assert run['exit_code'] == 0 and not run['stderr'] and run['stdout'] == '#rejected\n', (work, run)
assert refused.read_bytes() == b'preserve existing destination'
receipt['passed'] = True
receipt['image_sha256'] = sha(image)
image.unlink()
save_json(work / 'results.json', receipt)
print('task fields: passed; ' + str(work / 'results.json'))
