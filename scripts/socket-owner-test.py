#!/usr/bin/env python3
"""Real descriptor reuse, clone aliases, collecting callbacks and socket images."""
import json
from pathlib import Path
import sys
import tempfile
from validation_support import clean_environment, command, save_json, sha

prefix = Path(sys.argv[1]).resolve()
work = Path(tempfile.mkdtemp(prefix='birnam-socket-owner-'))
env = clean_environment(); env['BIRNAM_GC_PRECISE'] = '1'
source = work / 'owners.birnam'; image = work / 'closed.image'; refused = work / 'live.image'
refused.write_bytes(b'keep this destination')
source.write_text('''[Module load: #socket];
[Alien wrap: "dup" signature: "i i" as: #ownerProbeDuplicate on: Integer];
[[|
let first := [1 ownerProbeDuplicate];
let old := [Socket wrapFd: first];
let alias := [old clone];
[System println: [[old connectionOwner] == [alias connectionOwner]]];
[System println: [[old close] = 0]];
let replacement := [1 ownerProbeDuplicate];
[System println: [replacement = first]];
[System println: [[alias close] = 0]];
let alive := [replacement ownerProbeDuplicate];
[System println: [alive >= 0]];
[System println: [[old fd] isNil]];
[System println: [[alias buffer] isNil]];
[[alive >= 0] ifTrue: [| [alive closeFd]]]; [replacement closeFd];
// A collecting free callback reenters close on an alias of the same owner.
let nested := [Socket wrapFd: [1 ownerProbeDuplicate]];
let nestedAlias := [nested clone]; let realBuffer := [nested buffer];
let calls := [0 asCell]; let cleanup := [Object clone];
[Reflect defineMethod: #free on: cleanup as: [:self |
  [calls inc]; [System gc]; [nestedAlias close]; [realBuffer free]
]];
[[nested connectionOwner] set: #[[nested fd] cleanup]];
[System println: [[nested close] = 0]];
[System println: [[calls value] = 1]];
[System println: [[nestedAlias fd] isNil]];
// Cleanup can install a fresh owner without a trailing clear erasing it.
let changing := [Socket wrapFd: [1 ownerProbeDuplicate]];
let oldBuffer := [changing buffer]; let nextDescriptor := [1 ownerProbeDuplicate];
let nextBuffer := [Alien alloc: 65536]; let nextOwner := [Cell with: #[nextDescriptor nextBuffer]];
let replacing := [Object clone];
[Reflect defineMethod: #free on: replacing as: [:self |
  [changing connectionOwner: nextOwner]; [System gc]; [oldBuffer free]
]];
[[changing connectionOwner] set: #[[changing fd] replacing]];
[System println: [[changing close] = 0]];
[System println: [[changing connectionOwner] == nextOwner]];
let duplicated := [nextDescriptor ownerProbeDuplicate];
[System println: [duplicated >= 0]];
[[duplicated >= 0] ifTrue: [| [duplicated closeFd]]];
[System println: [[changing close] = 0]];
// Preserve the first close error, consume ownership, and do not retry it.
let invalid := [Socket wrapFd: -1];
[System println: [[invalid close] = -1]];
[System println: [[invalid close] = 0]];
// Current owner dispatch stays malleable; a getter can replace state before CAS.
let stale := [Socket wrapFd: [1 ownerProbeDuplicate]];
let staleOwner := [stale connectionOwner]; let observed := [staleOwner value];
let freshDescriptor := [1 ownerProbeDuplicate]; let freshBuffer := [Alien alloc: 65536];
[Reflect defineMethod: #value on: staleOwner as: [:self |
  [self set: #[freshDescriptor freshBuffer]]; observed
]];
[System println: [[stale close] = 0]];
let stillOpen := [freshDescriptor ownerProbeDuplicate];
[System println: [stillOpen >= 0]];
[[stillOpen >= 0] ifTrue: [| [stillOpen closeFd]]];
[freshDescriptor closeFd]; [freshBuffer free];
[[observed at: 0] closeFd]; [[observed at: 1] free];
[Globals at: #ClosedSocket put: old]; [Globals at: #ClosedSocketAlias put: alias];
[Globals at: #SocketFactoryCopy put: [SockConn clone]];
[Globals at: #main put: [|
  [System gc];
  [System println: [[ClosedSocket fd] isNil]];
  [System println: [[ClosedSocket buffer] isNil]];
  [System println: [[ClosedSocket connectionOwner] == [ClosedSocketAlias connectionOwner]]];
  [System println: [[ClosedSocket close] = 0]];
  [System println: [[SocketFactoryCopy close] = 0]]
]]
] value];
[System gc]; [System saveImage: IMAGE];
[System println: [[System imageSaveReport] status]];
[Globals at: #LiveSocket put: [Socket wrapFd: [1 ownerProbeDuplicate]]];
[System saveImage: REFUSED]; [System println: [[System imageSaveReport] status]];
[LiveSocket close];
'''.replace('IMAGE', json.dumps(str(image))).replace('REFUSED', json.dumps(str(refused))))
receipt = {'passed': False, 'prefix': str(prefix), 'source': source.read_text(),
           'runner_sha256': sha(Path(__file__).resolve()), 'runs': []}
for name, args, expected in [('source', [source], 'true\n' * 18 + '#saved\n#rejected\n'),
                             ('mapped', [image], 'true\n' * 5),
                             ('rebuilt', ['--rebuild', image], 'true\n' * 5)]:
    run = command([prefix / 'bin/birnam', *args], cwd=work, env=env, timeout=600)
    receipt['runs'].append(dict(name=name, **run)); save_json(work / 'results.json', receipt)
    (work / (name + '.log')).write_text(run['output'])
    assert run['exit_code'] == 0 and not run['stderr'] and run['stdout'] == expected, (work, name, run)
    if name == 'source': source.unlink()
assert refused.read_bytes() == b'keep this destination'
receipt['image_sha256'] = sha(image); image.unlink()
receipt['passed'] = True; save_json(work / 'results.json', receipt)
print('socket owner: passed; ' + str(work / 'results.json'))
