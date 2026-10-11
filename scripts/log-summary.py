#!/usr/bin/env python3
"""Unattended application acceptance and bounded, reproducible measurements."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import resource
import shutil
import subprocess
import time
from log_summary_data import generate
from validation_support import ROOT, clean_environment, command, save_json, sha


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix',type=Path,required=True)
    parser.add_argument('--work',type=Path,default=ROOT/'.cargo-target/log-summary')
    parser.add_argument('--timeout',type=float,default=30)
    parser.add_argument('--sizes',default='small,medium,large')
    parser.add_argument('--skip-edit',action='store_true')
    args=parser.parse_args()
    if not 1 <= args.timeout <= 120: parser.error('timeout must be 1..120 seconds')
    sizes=args.sizes.split(',')
    if any(s not in ('small','medium','large') for s in sizes): parser.error('unknown size')
    prefix=args.prefix.resolve();binary=prefix/'bin/birnam';work=args.work.resolve()
    if not binary.is_file(): parser.error('prefix has no bin/birnam')
    if work==ROOT or ROOT.is_relative_to(work): parser.error('work must be a dedicated directory')
    if work.exists() and not (work/'log-summary-work.json').exists(): parser.error('work directory is not owned by this runner')
    work.mkdir(parents=True,exist_ok=True)
    save_json(work/'log-summary-work.json',{'purpose':'disposable log-summary acceptance'})
    project=work/'examples/log-summary';library=work/'modules'
    shutil.copytree(ROOT/'examples/log-summary',project,dirs_exist_ok=True,ignore=shutil.ignore_patterns('.birnam'))
    shutil.copytree(ROOT/'modules',library,dirs_exist_ok=True,ignore=shutil.ignore_patterns('.birnam'))
    manifest=generate(work/'data');env=clean_environment()
    evidence={'schema':1,'source_revision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
              'source_sha256':{str(p.relative_to(ROOT)):sha(p) for p in sorted([*ROOT.glob('examples/log-summary/**/*.birnam'),*ROOT.glob('examples/log-summary/**/birnam.toml'),ROOT/'modules/json.birnam',ROOT/'modules/birnam.toml',ROOT/'scripts/log_summary_data.py',Path(__file__)]) if '.birnam' not in p.parts},
              'runtime_sha256':{p.name:sha(p) for p in sorted((prefix/'lib').glob('*.dylib'))},
              'host_sha256':sha(binary),'prefix':str(prefix),'fixtures':manifest,'runs':[],
              'qualification':'Separate process diagnostics; warm driver times parse/aggregate/render only. No release claim or idle-host requirement.'}
    def run(label,argv,timeout=None):
        before=resource.getrusage(resource.RUSAGE_CHILDREN)
        metrics=work/(label+'.time')
        timer=['/usr/bin/time', '-l' if platform.system()=='Darwin' else '-v','-o',str(metrics)]
        started=time.monotonic_ns()
        try:
            result=command(timer+[str(binary),*map(str,argv)],cwd=work,env=env,timeout=timeout or args.timeout)
            result['status']='completed'
        except subprocess.TimeoutExpired:
            result={'status':'timeout','exit_code':None,'stdout':'','stderr':'','argv':list(map(str,argv)), 'wall_ns':time.monotonic_ns()-started, 'deadline_seconds':timeout or args.timeout}
        after=resource.getrusage(resource.RUSAGE_CHILDREN)
        result.update(label=label,cpu_seconds=(after.ru_utime+after.ru_stime)-(before.ru_utime+before.ru_stime))
        raw=metrics.read_text() if metrics.exists() else ''
        match=re.search(r'(\d+)\s+maximum resident set size',raw)
        if match: result['peak_rss_bytes']=int(match[1])
        else:
            match=re.search(r'Maximum resident set size \(kbytes\):\s*(\d+)',raw)
            if match: result['peak_rss_bytes']=int(match[1])*1024
        evidence['runs'].append(result);save_json(work/'results.json',evidence)
        print(label,result['status'],result['exit_code'],round(result.get('wall_ns',0)/1e9,3),flush=True)
        return result
    def passed(row):
        if row['exit_code']!=0: raise RuntimeError(f"{row['label']}: {row['stderr']}")
    def expected(name): return json.loads((work/'data'/manifest['valid'][name]['expected']).read_text())
    def check(row,name):
        passed(row)
        if row['stderr']: raise RuntimeError(f"{row['label']}: unexpected stderr {row['stderr']}")
        if json.loads(row['stdout'])!=expected(name): raise RuntimeError(f"{row['label']}: incorrect report")
    passed(run('check',['check',project,'--format=json'],120))
    passed(run('build',['build',project],120))
    passed(run('tests',['test',project],120))
    for name in ('empty','tiny','edges','escaping'):
        check(run(name,['run',project,'--',work/'data'/(name+'.jsonl')]),name)
    for name in manifest['invalid']:
        row=run('invalid-'+name,['run',project,'--',work/'data'/(name+'.jsonl')])
        line=3 if name=='line-number' else 1
        if row['exit_code']!=2 or row['stdout'] or f'line {line}:' not in row['stderr'] or name+'.jsonl' not in row['stderr']:
            raise RuntimeError(f'{name}: wrong error contract: {row}')
    row=run('missing-file',['run',project,'--',work/'data/no-such-file.jsonl'])
    if row['exit_code']!=2 or 'could not read file' not in row['stderr']: raise RuntimeError('missing-file diagnostic failed')
    row=run('usage',['run',project])
    if row['exit_code']!=2 or 'usage:' not in row['stderr']: raise RuntimeError('usage diagnostic failed')
    outputs=[]
    for i in range(3):
        row=run('startup-'+str(i),['run',project,'--',work/'data/tiny.jsonl']);check(row,'tiny');outputs.append(row['stdout'])
    if len(set(outputs))!=1: raise RuntimeError('output bytes are nondeterministic')
    for name in sizes:
        row=run('scale-'+name,['run',project,'--',work/'data'/(name+'.jsonl')])
        if row['status']!='timeout': check(row,name)
    passed(run('build-warm',['build',project/'bench'],120))
    for name in ['tiny','small']:
        row=run('warm-'+name,['run',project/'bench','--',work/'data'/(name+'.jsonl'),'5'])
        if row['status']=='timeout': continue
        passed(row);decoded=json.loads(row['stdout'])
        if decoded['report']!=expected(name) or len(decoded['samples'])!=5: raise RuntimeError('warm result mismatch')
        row['warm']=decoded;save_json(work/'results.json',evidence)
    if not args.skip_edit:
        source=project/'src/log-summary.birnam';original=source.read_text()
        source.write_text(original.replace('log-summary-v1','log-summary-edit'))
        try:
            row=run('edit-rebuild-run',['run',project,'--build','--',work/'data/tiny.jsonl'],120)
            passed(row);want=expected('tiny');want['format']='log-summary-edit'
            if json.loads(row['stdout'])!=want: raise RuntimeError('published program did not reflect edit')
        finally: source.write_text(original)
    evidence['correctness_passed']=True;save_json(work/'results.json',evidence)
    print(work/'results.json')

if __name__=='__main__':
    try: main()
    except (RuntimeError, ValueError, OSError) as error: raise SystemExit(str(error))
