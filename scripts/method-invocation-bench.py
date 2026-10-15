#!/usr/bin/env python3
"""Compare ordinary source definitions in installed, fully bootstrapped runtimes."""
import argparse
import json
import statistics
import tempfile
from pathlib import Path
from validation_support import ROOT, clean_environment, command, save_json, sha

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--baseline', type=Path, required=True)
p.add_argument('--candidate', type=Path, required=True)
p.add_argument('--samples', type=int, default=7)
a = p.parse_args()
assert a.samples > 0
work = Path(tempfile.mkdtemp(prefix='birnam-method-construction-bench-'))
source = work/'bench.birnam'
source.write_text('''
[Module load: #time];
[[|
[[Namespace includesKey: #MethodInvocation] ifTrue: [| [MethodInvocation clearTemplates]]];
let target := [Object clone];
let body := [:self :argument | argument];
[System gc]; [System autoGc: false];
let begin := [Time monotonicMicros];
let methods := [[1 to: 16] inject: #[] into: [:methods :i |
  [Reflect defineMethod: #constructionBench: on: target as: body];
  [methods add: [target methodFor: #constructionBench:]]
]];
let end := [Time monotonicMicros];
[System println: [end - begin]];
[System println: [methods size]];
[System println: [target constructionBench: 41]];
[1 to: 100 do: [:i | [target constructionBench: 41]]];
let startCalls := [Time monotonicMicros];
[1 to: 10000 do: [:i | [target constructionBench: 41]]];
[System println: [[Time monotonicMicros] - startCalls]];
[System autoGc: true]
] value]
''')
receipt = {'passed': False, 'scope': 'Fresh installed host processes using their compiled startup images and normal source Reflect definitions. Timers exclude startup/module loading; 16 retained definitions include first template compilation. The 10000-call language loop includes iteration costs. GC disabled only inside timed work. No timing threshold.',
           'source': source.read_text(), 'source_sha256': sha(Path(__file__).resolve()), 'runs': []}
for i in range(a.samples):
    for name in (['baseline', 'candidate'] if i % 2 == 0 else ['candidate', 'baseline']):
        prefix = getattr(a,name).resolve()
        result = command([prefix/'bin/birnam',source], cwd=work, env=clean_environment(), timeout=90)
        receipt['runs'].append(dict(sample=i, name=name, **result))
        save_json(work/'results.json', receipt)
        assert result['exit_code'] == 0 and not result['stderr'], result
        lines = result['stdout'].splitlines()
        assert len(lines) == 4 and lines[1:3] == ['16','41'], result
        receipt['runs'][-1].update(construction_us=int(lines[0]), invocation_loop_us=int(lines[3]))
receipt['medians'] = {name: {key: statistics.median(r[key] for r in receipt['runs'] if r['name']==name)
    for key in ['construction_us','invocation_loop_us']} for name in ['baseline','candidate']}
receipt['construction_speedup'] = receipt['medians']['baseline']['construction_us']/receipt['medians']['candidate']['construction_us']
receipt['passed'] = True
save_json(work/'results.json', receipt)
print(json.dumps(receipt['medians']))
print('construction speedup:', receipt['construction_speedup'])
print(work/'results.json')
