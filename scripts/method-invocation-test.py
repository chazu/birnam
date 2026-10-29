#!/usr/bin/env python3
"""Direct method construction and live policy across fresh image processes."""
import argparse
import json
from pathlib import Path
import tempfile
from validation_support import ROOT, clean_environment, command, save_json, sha

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('prefix', type=Path)
a = p.parse_args()
prefix = a.prefix.resolve()
work = Path(tempfile.mkdtemp(prefix='birnam-method-invocation-'))
image = work / 'factory.image'
source = work / 'writer.birnam'
source.write_text('''
[[|
let target := [Object clone];
[Reflect defineMethod: #answer: on: target as: [:self :argument | argument]];
[Globals at: #FactoryImageTarget put: target];
let error := [Error kind: #imageFactoryRejected];
[Globals at: #main put: [|
  [System println: [[Namespace includesKey: #MethodInvocation] not]];
  let target := FactoryImageTarget;
  let old := [target methodFor: #answer:];
  let basis := [Globals bindingBasis];
  let bound := [Reflect methodForArity: 1 implementation: [:self :argument | [argument + 1]]];
  [System println: [bound invokeOn: target arguments: #[41]]];
  [System println: [old invokeOn: target arguments: #[41]]];
  [System println: [[Globals bindingBasis] = basis]];
  // Ordinary edits remain available in running main; construction stays live.
  let constructor := [Reflect methodFor: #methodForArity:implementation:];
  [Reflect defineMethod: #methodForArity:implementation: on: Reflect as: [:self :arity :implementation | error]];
  let result := [Reflect methodForArity: 1 implementation: [:self :argument | argument]];
  [System println: [result == error]];
  [[Vtable behaviorOf: Reflect] addMethod: constructor as: #methodForArity:implementation:];
  [System println: [[Reflect defineMethod: #runningEdit on: target as: [:self | nil]] == target]]
]];
[System saveImage: IMAGE];
[System println: [[System imageSaveReport] status]];
[System println: [[Namespace includesKey: #MethodInvocation] not]]
] value]
'''.replace('IMAGE', json.dumps(str(image))))
receipt = {'passed': False, 'prefix': str(prefix), 'source': source.read_text(), 'runs': [],
           'source_sha256': {str(f.relative_to(ROOT)): sha(f) for f in [
               Path(__file__).resolve(),
               ROOT/'rust/birnam-host-entry/src/startup.rs', ROOT/'rust/birnam-codegen-runtime/src/lib.rs']}}

def run(name, argv, expected, env):
    result = command(argv, cwd=work, env=env, timeout=240)
    receipt['runs'].append(dict(name=name, **result))
    save_json(work/'results.json', receipt)
    assert result['exit_code'] == 0 and not result['stderr'] and result['stdout'] == expected, (name, work, result)

env = clean_environment()
run('save', [prefix/'bin/birnam', source], '#saved\ntrue\n', env)
source.unlink()
for name, flags in [('mapped', []), ('rebuilt', ['--rebuild'])]:
    run(name, [prefix/'bin/birnam', *flags, image], 'true\n42\n41\ntrue\ntrue\ntrue\n', env)
receipt['image_sha256'] = sha(image)
receipt['passed'] = True
save_json(work/'results.json', receipt)
print('method-invocation: source-free images and live factory policy passed; '+str(work/'results.json'))
