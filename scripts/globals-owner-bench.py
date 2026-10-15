#!/usr/bin/env python3
"""Paired Globals operation CPU observations; startup and setup are excluded."""
import argparse
import json
from pathlib import Path
import statistics
import tempfile
from validation_support import ROOT, clean_environment, command, save_json, sha
p=argparse.ArgumentParser(description=__doc__);p.add_argument('baseline',type=Path);p.add_argument('candidate',type=Path);p.add_argument('--samples',type=int,default=3);a=p.parse_args()
prefixes={k:getattr(a,k).resolve() for k in ['baseline','candidate']};work=Path(tempfile.mkdtemp(prefix='birnam-globals-bench-'))
cases={'read':(2000,14000,'[:count | [[1 to: count] inject: 0 into: [:sum :i | [sum + [owner at: #globalsBenchValue]]]]]'),
 'source-read':(2000,14000,'[:count | [[1 to: count] inject: 0 into: [:sum :i | [sum + globalsBenchValue]]]]'),
 'write':(500,500,'[:count | [[1 to: count] do: [:i | [owner at: #globalsBenchWrite put: i]]]; [owner at: #globalsBenchWrite]]')}
receipt={'passed':False,'prefixes':{k:str(v) for k,v in prefixes.items()},'runs':[],'sources':{},'runner_sha256':sha(Path(__file__).resolve()),'scope':'Darwin thread CPU clock 16. Normal GC. Startup, setup and explicit final GC excluded. These are operation microbenchmarks, not whole-language speedups.'}
for name,(count,_,operation) in cases.items():
 source=work/(name+'.birnam');source.write_text('''[Module load: #time]; [Module load: #json];
[[|
let owner := Globals; [owner at: #globalsBenchValue put: 7];
let operation := OPERATION; [operation value: 10]; [System gc];
let facts := [db retainedFactCount]; let cpu := [16 clockNanos];
let answer := [operation value: COUNT]; let elapsed := [[16 clockNanos] - cpu];
[System gc];
[System println: [Json stringify: {"answer" -> answer "cpu_ns" -> elapsed "fact_delta" -> [[db retainedFactCount] - facts]}]]
] value]
'''.replace('OPERATION',operation).replace('COUNT',str(count)));receipt['sources'][name]=source.read_text()
for sample in range(a.samples):
 for label in (['baseline','candidate'] if sample%2==0 else ['candidate','baseline']):
  for name,(_,expected,_) in cases.items():
   run=command([prefixes[label]/'bin/birnam',work/(name+'.birnam')],env=clean_environment(),cwd=work,timeout=180)
   row=dict(label=label,case=name,sample=sample,run=run);receipt['runs'].append(row);save_json(work/'results.json',receipt)
   assert run['exit_code']==0 and not run['stderr'],(work,row)
   value=json.loads(run['stdout']);assert value['answer']==expected and value['cpu_ns']>0 and value['fact_delta']==0,value
   row['observation']=value
receipt['summary']={}
for name in cases:
 med={label:statistics.median(r['observation']['cpu_ns'] for r in receipt['runs'] if r['label']==label and r['case']==name) for label in prefixes}
 receipt['summary'][name]={'median_cpu_ns':med,'candidate_over_baseline':med['candidate']/med['baseline']}
receipt['passed']=True;save_json(work/'results.json',receipt);print('globals-bench: '+str(work/'results.json'))
