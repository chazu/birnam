#!/usr/bin/env python3
"""Bounded, headless Logue acceptance and generation/turn measurements (macOS)."""
import argparse
import json
from pathlib import Path
import platform
import re
import shutil
import statistics
import subprocess
from logue_oracle import expected
from validation_support import ROOT, clean_environment, command, save_json, sha

CASES = {
    'quiet': dict(width=16, monsters=0, turns=16, batches=4),
    'sparse': dict(width=16, monsters=2, turns=16, batches=4),
    'crowded': dict(width=16, monsters=8, turns=16, batches=4),
    'long': dict(width=16, monsters=2, turns=64, batches=4),
    'wide': dict(width=32, monsters=8, turns=16, batches=4),
    'generation': dict(phase='generation', batches=1),
    'generation-alt': dict(phase='generation', seed=777, batches=1),
    'replacement': dict(width=8, monsters=2, turns=8, batches=2, samples=1, replace=True),
}

def literal(config):
    def value(x):
        if isinstance(x,bool): return 'true' if x else 'false'
        return json.dumps(x)
    return '{'+ ' '.join('#'+key+' -> '+value(v) for key,v in config.items())+'}'

def check_report(report, config):
    if report['config'] != {'#'+k:v for k,v in config.items()}: raise ValueError('configuration mismatch')
    states,replacement=expected(config)
    if len(report['results']) != len(states): raise ValueError('missing samples')
    for sample,(actual,want) in enumerate(zip(report['results'],states),1):
        if len(actual)!=len(want): raise ValueError('missing checkpoints')
        for batch,(row,state) in enumerate(zip(actual,want),1):
            if row['sample']!=sample or row['batch']!=batch: raise ValueError('checkpoint order mismatch')
            if json.loads(row['state_json']) != state:
                actual_state=json.loads(row['state_json'])
                keys=[k for k in set(actual_state)|set(state) if actual_state.get(k)!=state.get(k)]
                raise ValueError(f'sample {sample}, batch {batch}: state mismatch in {keys}')
            if row['cpu_ns']<=0 or row['wall_ns']<=0: raise ValueError('invalid timing')
    if report['replacement']!=replacement: raise ValueError('replacement was not observed')

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--prefix',type=Path,required=True)
    p.add_argument('--work',type=Path,default=ROOT/'.cargo-target/logue-bench')
    p.add_argument('--cases',default=','.join(CASES))
    p.add_argument('--samples',type=int,default=3)
    p.add_argument('--timeout',type=float,default=180)
    p.add_argument('--allocation',action='store_true',help='also run one no-auto-GC batch per selected performance case')
    p.add_argument('--skip-regression',action='store_true',help='development runs only; receipt records missing qualification')
    args=p.parse_args(); names=args.cases.split(',')
    if platform.system()!='Darwin': p.error('Time clock IDs currently require macOS')
    if not 1<=args.samples<=10 or not 1<=args.timeout<=900: p.error('samples must be 1..10 and timeout 1..900')
    if not names or any(n not in CASES for n in names) or len(names)!=len(set(names)): p.error('unknown or duplicate case')
    prefix=args.prefix.resolve(); binary=prefix/'bin/birnam'; work=args.work.resolve()
    if not binary.is_file(): p.error('prefix has no bin/birnam')
    if work==ROOT or ROOT.is_relative_to(work) or work==prefix or prefix.is_relative_to(work): p.error('work must be a dedicated directory')
    marker=work/'logue-bench-work.json'
    if work.exists() and not marker.is_file(): p.error('work directory is not owned by this runner')
    work.mkdir(parents=True,exist_ok=True); save_json(marker,{'purpose':'disposable Logue benchmark'})
    project=work/'logue'; project.mkdir(exist_ok=True)
    sources=[ROOT/'examples/logue'/n for n in ('logue-core.birnam','logue-test.birnam','logue-bench.birnam')]
    for source in sources: shutil.copy2(source,project/source.name)
    bound=sources+[Path(__file__).resolve(),ROOT/'scripts/logue_oracle.py',ROOT/'scripts/logue-bench-test.py',ROOT/'scripts/validation_support.py']
    evidence={'schema':1,'revision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        'platform':platform.platform(),'prefix':str(prefix),'source_sha256':{str(f.relative_to(ROOT)):sha(f) for f in bound},
        'runtime_sha256':{str(f.relative_to(prefix)):sha(f) for f in [binary,*sorted((prefix/'lib').glob('*.dylib')),*sorted((prefix/'lib').glob('*.image'))]},
        'runs':[], 'regression_requested':not args.skip_regression,
        'qualification':'Script loader path; startup/source loading, fixture setup, GC between batches, snapshots and JSON are outside phase timers. Timing keeps normal GC and Store history. Allocation mode disables automatic GC for one batch and is not a timing baseline. Live counters are process-wide and include retained harness output; charge is GC accounting, not RSS.'}
    save_json(work/'results.json',evidence)
    def run(label,source):
        metrics=work/(label+'.time'); log=work/(label+'.log')
        try:
            row=command(['/usr/bin/time','-l','-o',str(metrics),str(binary),source],cwd=project,env=clean_environment(),timeout=args.timeout)
            row['status']='completed'; log.write_text(row['output'])
        except subprocess.TimeoutExpired as e:
            partial=e.stdout or b''
            if isinstance(partial,bytes): partial=partial.decode(errors='replace')
            row=dict(exit_code=None,status='timeout',deadline_seconds=args.timeout,stdout=partial,stderr='')
            log.write_text(partial)
        row.update(label=label,log=str(log),log_sha256=sha(log))
        raw=metrics.read_text() if metrics.exists() else ''
        match=re.search(r'(\d+)\s+maximum resident set size',raw)
        if match: row['peak_rss_bytes']=int(match[1])
        evidence['runs'].append(row); save_json(work/'results.json',evidence)
        print(label,row['status'],row['exit_code'],flush=True)
        return row
    if not args.skip_regression:
        row=run('regression','logue-test.birnam')
        if row['exit_code']!=0 or row['stderr'] or '  FAIL ' in row['stdout'] or 'RESULT: 34 checks, 0 failures' not in row['stdout']:
            raise RuntimeError('existing headless regression failed or timed out; see '+str(work/'results.json'))
        row['correctness_passed']=True
    for name in names:
        for allocation in ([False,True] if args.allocation and name!='replacement' else [False]):
            config=dict(phase='arena',seed=12345,width=16,monsters=2,turns=16,batches=4,samples=args.samples,allocation=allocation,replace=False)
            config.update(CASES[name])
            if allocation: config.update(samples=1,batches=1)
            label=name+('-allocation' if allocation else '')
            source=project/(label+'.birnam')
            source.write_text('[Globals at: #LogueBenchConfig put: '+literal(config)+'];\n[System load: "logue-bench.birnam"]\n')
            row=run(label,source.name); row['config']=config; row['fixture_sha256']=sha(source)
            if row['status']=='timeout':
                save_json(work/'results.json',evidence); continue
            if row['exit_code']!=0 or row['stderr']: raise RuntimeError(label+': process failed; see '+str(row['log']))
            lines=row['stdout'].splitlines()
            if len(lines)!=2 or lines[0]!='[logue-core] loaded': raise ValueError(label+': unexpected output')
            report=json.loads(lines[1])
            try: check_report(report,config)
            except ValueError as e:
                row['correctness_passed']=False; row['error']=str(e); save_json(work/'results.json',evidence); raise
            row['correctness_passed']=True; row['report']=report
            rows=[b for s in report['results'] for b in s]
            row['median_cpu_ns']=statistics.median(b['cpu_ns'] for b in rows)
            row['batch_median_cpu_ns']={str(i):statistics.median(b['cpu_ns'] for b in rows if b['batch']==i) for i in range(1,config['batches']+1)}
            if config['phase']=='arena': row['median_cpu_ns_per_turn']=row['median_cpu_ns']/config['turns']
            print('  checked',len(rows),'checkpoints; median CPU ms',round(row['median_cpu_ns']/1e6,3),flush=True)
            save_json(work/'results.json',evidence)
    # Recheck originals and installed artifacts: measurements must bind stable inputs.
    for name,digest in evidence['source_sha256'].items():
        if sha(ROOT/name)!=digest: raise RuntimeError('source changed during benchmark: '+name)
    for name,digest in evidence['runtime_sha256'].items():
        if sha(prefix/name)!=digest: raise RuntimeError('runtime changed during benchmark: '+name)
    evidence['completed_correctness_passed']=all(r.get('correctness_passed') for r in evidence['runs'] if r['status']=='completed')
    evidence['all_requested_passed']=all(r.get('correctness_passed',False) for r in evidence['runs'])
    save_json(work/'results.json',evidence); print(work/'results.json')
    return 0 if evidence['all_requested_passed'] else 2

if __name__=='__main__':
    try: raise SystemExit(main())
    except (RuntimeError, ValueError, OSError) as e: raise SystemExit(str(e))
