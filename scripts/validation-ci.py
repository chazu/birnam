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
from validate import HOST_GROUPS, plan
from validation_support import ROOT, audit_plan, save_json

PREFIX = Path('/private/tmp/birnam-ci/prefix')
OUTPUT = ROOT / '.cargo-target/validation'


def select(paths):
    bootstrap=re.compile(r'^(\.github/|mise\.toml$|scripts/(rust-(install|driver-cold)|dist\.sh|release[._-]|install[-_])|rust/birnam-(host|cli|siphon|build)/|.*/build\.rs$)')
    protected=re.compile(r'^(rust/|modules/|vendor/|tests/|Cargo\.|scripts/(fixtures/|closure|unboxed|loop|recovery|validation|validate))')
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
    integrations = {'host-' + group for group in HOST_GROUPS}
    core = [s['name'] for s in steps if s['name'] not in integrations]
    return steps, core, [s for s in steps if s['name'] in integrations]


def shards(steps, count=4):
    # Each integration group has its own prefix and fixture lifetime. Time limits
    # only balance work; required receipts determine coverage.
    bins = [[] for _ in range(count)]
    totals = [0] * count
    for step in sorted(steps, key=lambda s: (-s['timeout'], s['name'])):
        index = min(range(count), key=lambda i: totals[i])
        bins[index].append(step['name'])
        totals[index] += step['timeout']
    return bins


def execute(suite, names=None):
    argv=[sys.executable,str(ROOT/'scripts/validate.py'),'--suite',suite,'--prefix',str(PREFIX),'--deadline','4800']
    if names:argv+=['--only',*names]
    subprocess.run(argv,cwd=ROOT,check=True)


def pack_seed(destination):
    # Preserve executable permissions in tar; upload-artifact alone drops them.
    with tarfile.open(destination,'w:gz') as archive:
        archive.add(PREFIX,arcname='prefix')
        archive.add(OUTPUT,arcname='validation')


def unpack_seed(source):
    seed=ROOT/'.cargo-target/ci-seed'
    seed.mkdir(parents=True,exist_ok=True)
    with tarfile.open(source) as archive:
        # Data filter rejects absolute paths, traversal and links escaping seed.
        archive.extractall(seed,filter='data')
    PREFIX.parent.mkdir(parents=True,exist_ok=True)
    shutil.copytree(seed/'prefix',PREFIX,dirs_exist_ok=True)
    shutil.copytree(seed/'validation',OUTPUT,dirs_exist_ok=True)


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
        execute(args.suite,core)
        shutil.copy2(OUTPUT/'run.json',OUTPUT/'stage-core.json')
        pack_seed(args.seed)
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
        audit_plan(rows, expected)
        save_json(OUTPUT/'run.json', dict(schema=1, status='passed', suite=args.suite,
                  selected_steps=[s['name'] for s in expected], steps=rows))
        print('CI: all native and installed Host acceptance groups passed')



if __name__=='__main__':main()
