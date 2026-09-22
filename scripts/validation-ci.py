#!/usr/bin/env python3
"""CI stage selection and disjoint matrix shards; validate.py owns execution."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
from closure_plan import closure_plan, TARGET
from validate import plan
from validation_support import ROOT, PlanRunner, audit_steps, save_json, sha

PREFIX = Path('/private/tmp/birnam-ci/prefix')
OUTPUT = ROOT / '.cargo-target/validation'
LIBRARIES = ('libbirnam_id.dylib', 'libbirnam_codegen_runtime.dylib', 'libbirnam_tcc.dylib')


def select(paths):
    bootstrap=re.compile(r'^(\.github/|mise\.toml$|scripts/rust-(install|driver-cold)|rust/birnam-(host|cli|siphon|build)/|.*/build\.rs$)')
    protected=re.compile(r'^(rust/|src/id/|modules/|vendor/|abi/|Cargo\.|scripts/(fixtures/|closure|unboxed|loop|recovery|validation|validate))')
    if any(bootstrap.search(path) for path in paths): return 'release'
    return 'exhaustive' if any(protected.search(path) for path in paths) else 'routine'


def scope():
    if os.environ.get('GITHUB_EVENT_NAME') in ('schedule', 'workflow_dispatch'): return 'release'
    base=os.environ.get('BIRNAM_DIFF_BASE','')
    if not base or set(base)=={'0'}: return 'exhaustive'
    diff=subprocess.run(['git','diff','--name-only',base,'HEAD'],cwd=ROOT,text=True,capture_output=True,timeout=30)
    return select(diff.stdout.splitlines()) if diff.returncode==0 else 'exhaustive'


def groups(suite):
    steps=plan(suite,PREFIX,OUTPUT)
    matrices={s['name'] for s in closure_plan(PREFIX,OUTPUT/'matrices','exhaustive')
              if not s['name'].startswith('diagnostics-') and s['name'] not in ('normal-probe-absence','complete-matrix-oracle')}
    core=[s['name'] for s in steps if s['name'] not in matrices and s['name']!='complete-matrix-oracle']
    return steps,core,[s for s in steps if s['name'] in matrices]


def shards(steps, count=4):
    # Approximate process counts only balance scheduling; they never determine
    # coverage or acceptance. Debug cases cost more; all cases remain assigned.
    def weight(step):
        argv=' '.join(step['argv'])
        cases=next((n for text,n in (('closure-sequence-smoke',320),('closure-budget-smoke',208),
                   ('closure-string-cleanup',240),('unboxed-numeric-control',448),
                   ('unboxed-numeric-smoke',146),('loop-measurement',96)) if text in argv),32)
        return cases*(5 if step['name'].endswith('-debug') else 1)
    bins=[[] for _ in range(count)]; totals=[0]*count
    for step in sorted(steps,key=lambda s:(-weight(s),s['name'])):
        index=min(range(count),key=lambda i:totals[i])
        bins[index].append(step['name']);totals[index]+=weight(step)
    return bins


def execute(suite, names=None):
    argv=[sys.executable,str(ROOT/'scripts/validate.py'),'--suite',suite,'--prefix',str(PREFIX),'--deadline','1800']
    if names:argv+=['--only',*names]
    subprocess.run(argv,cwd=ROOT,check=True)


def pack_seed(destination):
    # Preserve executable permissions in tar; upload-artifact alone drops them.
    with tarfile.open(destination,'w:gz') as archive:
        archive.add(PREFIX,arcname='prefix')
        archive.add(OUTPUT,arcname='validation')
        for profile in ('debug','release'):
            for name in LIBRARIES:
                archive.add(TARGET/profile/name,arcname=f'diagnostics/{profile}/{name}')


def unpack_seed(source):
    seed=ROOT/'.cargo-target/ci-seed'
    seed.mkdir(parents=True,exist_ok=True)
    with tarfile.open(source) as archive:
        # Data filter rejects absolute paths, traversal and links escaping seed.
        archive.extractall(seed,filter='data')
    PREFIX.parent.mkdir(parents=True,exist_ok=True)
    shutil.copytree(seed/'prefix',PREFIX,dirs_exist_ok=True)
    shutil.copytree(seed/'validation',OUTPUT,dirs_exist_ok=True)
    shutil.copytree(seed/'diagnostics',TARGET,dirs_exist_ok=True)


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('stage',choices=('select','core','matrix','collect'))
    p.add_argument('--suite',choices=('routine','exhaustive','release'))
    p.add_argument('--shard',type=int,choices=range(4))
    p.add_argument('--seed',type=Path,default=ROOT/'.cargo-target/ci-seed.tgz')
    args=p.parse_args()
    if args.stage=='select':
        chosen=scope();print(chosen)
        if os.environ.get('GITHUB_OUTPUT'):
            with open(os.environ['GITHUB_OUTPUT'],'a') as output: output.write('suite='+chosen+'\n')
        return
    if not args.suite:p.error('--suite is required')
    if args.stage=='core':
        _,core,_=groups(args.suite)
        execute(args.suite,None if args.suite=='routine' else core)
        shutil.copy2(OUTPUT/'run.json',OUTPUT/'stage-core.json')
        if args.suite!='routine':pack_seed(args.seed)
    elif args.stage=='matrix':
        if args.shard is None:p.error('--shard is required')
        unpack_seed(args.seed)
        _,_,matrices=groups(args.suite)
        execute(args.suite,shards(matrices)[args.shard])
        shutil.copy2(OUTPUT/'run.json',OUTPUT/f'stage-{args.shard}.json')
    else:
        unpack_seed(args.seed)
        rows={}
        for name in ('core','0','1','2','3'):
            data=json.loads((OUTPUT/f'stage-{name}.json').read_text())
            if data['status']!='passed':raise ValueError('CI stage did not pass: '+name)
            for label,row in data['steps'].items():
                if label in rows and rows[label]['evidence_sha256']!=row['evidence_sha256']:
                    raise ValueError('workers used different prerequisites: '+label)
                rows[label]=row
        expected=plan(args.suite,PREFIX,OUTPUT)
        audit_steps(rows,[s['name'] for s in expected if s['name']!='complete-matrix-oracle'])
        runner=PlanRunner(OUTPUT,deadline=180)
        runner.rows.update(rows)
        runner.run(**next(s for s in expected if s['name']=='complete-matrix-oracle'))
        audit_steps(runner.rows,[s['name'] for s in expected])
        runner.receipt.update(status='passed',suite=args.suite,selected_steps=[s['name'] for s in expected])
        runner.save()
        print('CI: all disjoint matrices and complete coverage oracle passed')


if __name__=='__main__':main()
