#!/usr/bin/env python3
"""Ordinary Id factory reuse after eviction, including escaped shared Cells."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import subprocess

ROOT = Path(__file__).resolve().parent.parent


def source(precise, escaping):
    tail = 'closure' if escaping else '[closure value]'
    factory = f'[:input | let closure := [| input]; {tail}]'
    check = '[[Globals at: #LifetimeResult] value]' if escaping else '[Globals at: #LifetimeResult]'
    lines = [f'[System preciseGc: {str(bool(precise)).lower()}]',
             f'[Globals at: #LifetimeRecipe put: [[{factory} codeObject] withoutSourceProvenance]]',
             '[Globals at: #LifetimeInput put: [73 asCell]]']
    for cycle in range(3):
        lines += ['[Globals at: #LifetimeFactory put: [Codegen materializeResumable: [Globals at: #LifetimeRecipe] captures: #[]]]']
        for phase in ('before', 'after'):
            if phase == 'after':
                lines += ['[Codegen evictMaterializations]', '[System gc]']
            lines += ['[Globals at: #LifetimeResult put: [[Globals at: #LifetimeFactory] value: [Globals at: #LifetimeInput]]]',
                      f'[System println: "cache-lifetime cycle={cycle} phase={phase}"]',
                      f'[System println: [{check} == [Globals at: #LifetimeInput]]]']
        lines += ['[Globals at: #LifetimeFactory put: nil]', '[Codegen evictMaterializations]', '[System gc]']
        if escaping:
            lines += ['[[Globals at: #LifetimeInput] set: 91]',
                      f'[System println: "cache-lifetime cycle={cycle} phase=escaped"]',
                      f'[System println: [{check} == [Globals at: #LifetimeInput]]]',
                      f'[System println: [[{check} value] = 91]]']
        lines += ['[Globals at: #LifetimeResult put: nil]', '[Codegen evictMaterializations]', '[System gc]']
    return '\n'.join(lines + [':quit', ''])


def validate(row):
    assert row['exit_code'] == 0 and not row['stderr'], 'process failed'
    assert 'Error' not in row['stdout'] and 'error' not in row['stdout']
    phases = re.findall(r'cache-lifetime cycle=(\d+) phase=(\w+)', row['stdout'])
    expected = [(str(c), p) for c in range(3) for p in (('before', 'after', 'escaped') if row['escaping'] else ('before', 'after'))]
    assert phases == expected, 'missing or reordered invocation'
    assert len(re.findall(r'^birnam> true$', row['stdout'], re.M)) == (12 if row['escaping'] else 6), 'identity or shared Cell changed'
    assert not re.search(r'^birnam> false$', row['stdout'], re.M)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--prefix', required=True, type=Path)
    p.add_argument('--floor', required=True, type=Path)
    p.add_argument('--profile', required=True, choices=('debug', 'release'))
    p.add_argument('--output', required=True, type=Path)
    args = p.parse_args()
    assert not args.output.exists()
    sha = lambda path: hashlib.sha256(Path(path).read_bytes()).hexdigest()
    sources = runpy.run_path(str(ROOT/'scripts/recovery-graphs.py'))['sources']()
    sources['scripts/closure-cache-lifetime-smoke.py'] = sha(__file__)
    manifest_path = args.prefix/'share/birnam/rust-install.json'
    installed = json.loads(manifest_path.read_text())
    assert installed['publication']['state'] == 'published'
    artifacts = {str(args.prefix/name): digest for name, digest in installed['staged_manifest'].items()}
    artifacts.update({str(args.floor): sha(args.floor), str(manifest_path): sha(manifest_path)})
    result = dict(status='running', qualified=False, profile=args.profile, sources=sources, artifacts=artifacts, rows=[], scope='Normal feature floor override on a published host. Ordinary Id calls verify values, shared Cells and escaped closure use after cache eviction; this does not assert complete cache or generation release.')
    env = {k:v for k,v in os.environ.items() if not k.startswith(('BIRNAM_', 'DYLD_'))}
    env['BIRNAM_ID_DYLIB'] = str(args.floor.resolve())
    args.output.parent.mkdir(parents=True, exist_ok=True)
    def save(): args.output.write_text(json.dumps(result, indent=2)+'\n')
    try:
        for precise in (0, 1):
            for escaping in (0, 1):
                for enabled in (0, 1):
                    assert all(sha(path)==digest for path,digest in artifacts.items())
                    code = source(precise, escaping)
                    process = subprocess.run([str(args.prefix.resolve()/'bin/birnam'), '--repl'], input=code, cwd=ROOT,
                        env=dict(env, BIRNAM_DEV_CLOSURE_ELISION=str(enabled)), text=True, capture_output=True, timeout=180)
                    row = dict(precise=precise, escaping=escaping, enabled=enabled, source=code, source_sha256=hashlib.sha256(code.encode()).hexdigest(), exit_code=process.returncode, stdout=process.stdout, stderr=process.stderr)
                    result['rows'].append(row); save(); validate(row)
                    print('cache-lifetime', args.profile, precise, escaping, enabled, 'passed', flush=True)
        assert all(sha(ROOT/path)==digest for path,digest in sources.items())
        assert all(sha(path)==digest for path,digest in artifacts.items())
        result['status']='passed'
    except BaseException as error:
        result.update(status='failed',error=str(error));raise
    finally:save()


if __name__ == '__main__': main()
