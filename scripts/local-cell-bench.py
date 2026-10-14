#!/usr/bin/env python3
"""Paired, unattended CPU/retention observations for the default Cell transition."""
import argparse
import json
from pathlib import Path
import statistics
import tempfile
from validation_support import ROOT, clean_environment, command, save_json, sha

CASES = {
    'cells': (1000, 501500, '''[:count |
      [[1 to: count] inject: 0 into: [:sum :i |
        let cell := [i asCell]; [cell set: [i + 1]]; [sum + [cell value]]]]
    ]'''),
    'string-order': (500, 500, '''[:count |
      [[1 to: count] inject: 0 into: [:sum :i |
        [sum + [["ab" < "ac"] ifTrue: [| 1] ifFalse: [| 0]]]]]
    ]'''),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('baseline', type=Path)
    parser.add_argument('candidate', type=Path)
    parser.add_argument('--samples', type=int, default=3)
    args = parser.parse_args()
    if args.samples < 1: parser.error('samples must be positive')
    prefixes = {k: getattr(args,k).resolve() for k in ('baseline','candidate')}
    work = Path(tempfile.mkdtemp(prefix='birnam-local-cell-bench-'))
    receipt = {'runs':[], 'passed':False, 'prefixes':{k:str(p) for k,p in prefixes.items()},
        'runner_sha256':sha(Path(__file__).resolve()), 'runtime_sha256':{}, 'sources':{}}
    for label,prefix in prefixes.items():
        paths = [prefix/'bin/birnam', *sorted((prefix/'lib').glob('*.dylib')), *sorted((prefix/'lib').glob('*.image'))]
        receipt['runtime_sha256'][label] = {str(p.relative_to(prefix)):sha(p) for p in paths}
    for case,(count,expected,operation) in CASES.items():
        source = work/(case+'.birnam')
        source.write_text('''[Module load: #json]; [Module load: #time];
[[|
let operation := OPERATION;
[operation value: 10]; [System gc];
let facts := [db retainedFactCount]; let objects := [System liveObjects];
let wall := [6 clockNanos]; let cpu := [16 clockNanos];
let result := [operation value: COUNT];
let used := [[16 clockNanos] - cpu]; let elapsed := [[6 clockNanos] - wall];
let gcStart := [16 clockNanos]; [System gc]; let gcCpu := [[16 clockNanos] - gcStart];
let retained := [[System liveObjects] - objects];
[System println: [Json stringify: {"answer" -> result "cpu_ns" -> used "wall_ns" -> elapsed
 "gc_cpu_ns" -> gcCpu "object_delta" -> retained "fact_delta" -> [[db retainedFactCount] - facts]}]]
] value]
'''.replace('OPERATION',operation).replace('COUNT',str(count)))
        receipt['sources'][case] = source.read_text()
    for sample in range(args.samples):
        for label in (('baseline','candidate') if sample % 2 == 0 else ('candidate','baseline')):
            for case,(count,expected,_) in CASES.items():
                run = command([prefixes[label]/'bin/birnam',work/(case+'.birnam')],
                    cwd=work, env=clean_environment(), timeout=180)
                (work/f'{label}-{case}-{sample}.log').write_text(run['output'])
                receipt['runs'].append(dict(label=label,case=case,sample=sample,run=run))
                save_json(work/'results.json',receipt)
                assert run['exit_code']==0 and not run['stderr'], (label,case,work)
                row=json.loads(run['stdout']); assert row['answer']==expected
                assert row['cpu_ns']>0 and row['wall_ns']>0
                expected_facts = 0 if label=='candidate' else count*(2 if case=='string-order' else 1)
                assert row['fact_delta']==expected_facts, (label,case,row)
                receipt['runs'][-1]['observation']=row
    summary={}
    for case in CASES:
        values={label:[r['observation']['cpu_ns'] for r in receipt['runs'] if r['case']==case and r['label']==label] for label in prefixes}
        medians={label:statistics.median(v) for label,v in values.items()}
        summary[case]={'cpu_ns':values,'median_cpu_ns':medians,
                       'cpu_reduction_percent':100*(1-medians['candidate']/medians['baseline'])}
    for label,prefix in prefixes.items():
        for name,digest in receipt['runtime_sha256'][label].items(): assert sha(prefix/name)==digest
    receipt.update(summary=summary,passed=True); save_json(work/'results.json',receipt)
    print('local-cell-bench: passed; '+str(work/'results.json'))


if __name__=='__main__': main()
