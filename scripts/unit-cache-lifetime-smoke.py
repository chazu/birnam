#!/usr/bin/env python3
"""Source-created Unit cache ownership, closure plans, identity and final release."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
METRICS = ('liveUnitCount', 'relocatedAllocationBytes', 'heapAllocationCount',
           'heapAllocationBytes', 'vtableCacheUnitCount', 'vtableCacheEntryCount')


def snapshot(phase):
    return ['[System gc]', f'[System println: "unit-cache-storage {phase}"]'] + [
        f'[System println: [Tcc {metric}]]' for metric in METRICS]


def lifetime(precise, escaping, *, numeric=False):
    # Returning the captured Cell tests its actual identity while the preceding
    # arithmetic requires the numeric consumer and its privately compiled Unit.
    arithmetic = 'let first := [20 + 15]; let second := [first + first]; ' if numeric else ''
    body = '[:input | let closure := [| ' + arithmetic + 'input]; let answer := [closure value]; let answer := [closure value]; let answer := [closure value]; ' + ('closure' if escaping else 'answer') + ']'
    lines = [f'[System preciseGc: {str(bool(precise)).lower()}]',
             '[Tcc checkUnitCacheContract]',
             f'[Globals at: #LifetimeRecipe put: [[{body} codeObject] withoutSourceProvenance]]',
             '[Globals at: #LifetimeInput put: [73 asCell]]',
             '[Globals at: #LifetimeHeld put: [nil asCell]]',
             '[Globals at: #LifetimeAnswer put: [nil asCell]]',
             '[Codegen evictMaterializations]', '[System gc]'] + snapshot('baseline')
    for cycle in range(3):
        lines += ['[ClosureControlProbe lifetimeBaseline]',
                  '[[Globals at: #LifetimeHeld] set: [ClosureControlProbe lifetimePrepare: [Globals at: #LifetimeRecipe] captures: #[]]]']
        warm = f'[[Globals at: #LifetimeAnswer] set: [ClosureControlProbe lifetimeWarm: [[Globals at: #LifetimeHeld] value] input: [Globals at: #LifetimeInput] escape: {escaping}]]'
        lines += [warm, '[Codegen evictMaterializations]', '[System gc]',
                  '[ClosureControlProbe lifetimeHeld]', warm,
                  '[[Globals at: #LifetimeHeld] set: nil]', '[System gc]']
        if escaping:
            lines += ['[[Globals at: #LifetimeInput] set: 91]',
                      '[ClosureControlProbe lifetimeEscaped: [[Globals at: #LifetimeAnswer] value] input: [Globals at: #LifetimeInput]]']
        lines += ['[[Globals at: #LifetimeAnswer] set: nil]', '[Codegen evictMaterializations]',
                  '[System gc]', '[ClosureControlProbe lifetimeReleased]',
                  f'[System println: "unit-cache-cycle {cycle}"]']
    return '\n'.join(lines + snapshot('released') + [':quit', ''])


def identity(precise, synchronous):
    mode = 'Synchronous' if synchronous else 'Resumable'
    lines = [f'[System preciseGc: {str(bool(precise)).lower()}]',
             '[Globals at: #CacheInput put: [73 asCell]]',
             '[Globals at: #CacheEffects put: [0 asCell]]',
             '[Globals at: #CacheRecipe put: [[[ :input | [| input]] codeObject] withoutSourceProvenance]]',
             f'[Globals at: #CacheFactory put: [Codegen materialize{mode}: [Globals at: #CacheRecipe] captures: #[]]]',
             '[Globals at: #CacheChild put: [[Globals at: #CacheFactory] value: [Globals at: #CacheInput]]]',
             '[Globals at: #CacheVtable put: [Reflect vtableOf: [Globals at: #CacheChild]]]',
             '[Globals at: #CacheCode put: [[Globals at: #CacheChild] codeObject]]',
             '[Reflect defineMethod: #value on: [Globals at: #CacheVtable] as: [:self | [[Globals at: #CacheEffects] inc]; [Globals at: #CacheInput]]]',
             '[Globals at: #CacheChild put: nil]', '[Codegen evictMaterializations]', '[System gc]',
             '[Globals at: #CacheChild put: [[Globals at: #CacheFactory] value: [Globals at: #CacheInput]]]',
             '[System println: [[Reflect vtableOf: [Globals at: #CacheChild]] == [Globals at: #CacheVtable]]]',
             '[System println: [[[Globals at: #CacheChild] codeObject] == [Globals at: #CacheCode]]]',
             '[System println: [[[Globals at: #CacheChild] value] == [Globals at: #CacheInput]]]',
             '[System println: [[[Globals at: #CacheEffects] value] = 1]]',
             '[Globals at: #CacheFactory put: nil]', '[Codegen evictMaterializations]', '[System gc]',
             '[[Globals at: #CacheInput] set: 91]',
             '[System println: [[[[Globals at: #CacheChild] value] value] = 91]]',
             '[System println: [[[Globals at: #CacheEffects] value] = 2]]',
             '[System println: "unit-cache identity and mutation passed"]', ':quit', '']
    if synchronous:
        # This emitter deliberately constructs fresh nested vtables/Code. Its
        # existing behavior must survive the owner fix; edits stay per vtable.
        lines = [line.replace('[[Reflect vtableOf: [Globals at: #CacheChild]] == [Globals at: #CacheVtable]]',
                              '[[[Reflect vtableOf: [Globals at: #CacheChild]] == [Globals at: #CacheVtable]] not]')
                     .replace('[[[Globals at: #CacheChild] codeObject] == [Globals at: #CacheCode]]',
                              '[[[[Globals at: #CacheChild] codeObject] == [Globals at: #CacheCode]] not]')
                     .replace('[[[Globals at: #CacheEffects] value] = 1]', '[[[Globals at: #CacheEffects] value] = 0]')
                     .replace('[[[Globals at: #CacheEffects] value] = 2]', '[[[Globals at: #CacheEffects] value] = 0]')
                 for line in lines]
    return '\n'.join(lines)


def validate(row):
    assert row['exit_code'] == 0 and not row['stderr'], 'process failure'
    assert not re.search(r'^birnam> (false|.*Error.*)$', row['stdout'], re.M), 'language failure'
    if row['case'] == 'identity':
        assert len(re.findall(r'^birnam> true$', row['stdout'], re.M)) == 6
        assert 'unit-cache identity and mutation passed' in row['stdout']
        return
    raw = row['stdout']
    released = re.findall(r'consumer-lifetime phase=released generation=\d+ watched=(\d+) live=(\d+) native_units=(\d+) relocated_bytes=(\d+) heap_blocks=(\d+) heap_bytes=(\d+)', raw)
    assert len(released) == 3 and all(int(n) > 0 and list(map(int, values)) == [0]*5 for n, *values in released), 'retained consumer owners'
    assert re.findall(r'unit-cache-cycle (\d+)', raw) == ['0','1','2']
    activity = [tuple(map(int, fields)) for fields in re.findall(r'consumer-lifetime-activity hits=(\d+) materialized=(\d+) completed=(\d+) escaping=(\d+)', raw)]
    assert len(activity) == 6
    for hits, materialized, completed, escaping in activity[::2]:
        assert escaping == row['escaping']
        if row['enabled']:
            assert hits == 8 and completed == 24 and materialized == 8*escaping
        else:
            assert hits == completed == 0
    numeric_activity = [tuple(map(int, fields)) for fields in re.findall(
        r'consumer-lifetime-numeric kernels=(\d+) counted_indices=(\d+) allocation_bytes=(\d+)', raw)]
    assert len(numeric_activity) == 6
    for kernels, counted, allocated in numeric_activity[::2]:
        # Three numeric calls share one prediction per factory invocation.
        assert kernels == (8 if row['numeric_body'] and row['enabled'] else 0)
        assert allocated > 0
        if not row['counted_frame_indices']:
            assert counted == 0
    if row['numeric_body'] and row['enabled']:
        warm = re.findall(r'consumer-lifetime phase=warm generation=\d+ watched=\d+ live=\d+ native_units=(\d+) relocated_bytes=(\d+) heap_blocks=(\d+) heap_bytes=(\d+)', raw)
        assert len(warm) == 6 and all(all(int(n) > 0 for n in fields) for fields in warm[::2]), 'numeric kernel was not retained while live'
    if row['escaping']:
        assert raw.count('consumer-lifetime phase=escaped') == 3
    assert 'unit-cache contract:' in raw
    storage = []
    for phase in ('baseline', 'released'):
        tail = raw.split('unit-cache-storage '+phase+'\n',1)[1]
        storage.append(list(map(int,re.findall(r'^birnam> (\d+)$', tail, re.M)[:len(METRICS)])))
    assert len(storage[0]) == len(METRICS) and storage[0] == storage[1], ('baseline Unit storage retained', storage)
    row['storage'] = dict(zip(METRICS, storage[1]))
    row['activity'] = activity
    row['numeric_activity'] = numeric_activity


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--prefix', required=True, type=Path)
    p.add_argument('--floor', required=True, type=Path)
    p.add_argument('--tcc', required=True, type=Path)
    p.add_argument('--normal-floor', required=True, type=Path)
    p.add_argument('--normal-tcc', required=True, type=Path)
    p.add_argument('--codegen', required=True, type=Path)
    p.add_argument('--profile', required=True, choices=('debug','release'))
    p.add_argument('--numeric-body', action='store_true')
    p.add_argument('--counted-frame-indices', action='store_true')
    p.add_argument('--output', required=True, type=Path)
    args = p.parse_args()
    assert not args.output.exists()
    sha = lambda path: hashlib.sha256(Path(path).read_bytes()).hexdigest()
    sources = runpy.run_path(str(ROOT/'scripts/recovery-graphs.py'))['sources']()
    sources['scripts/unit-cache-lifetime-smoke.py'] = sha(__file__)
    sources['scripts/closure-cache-lifetime-smoke.py'] = sha(ROOT/'scripts/closure-cache-lifetime-smoke.py')
    env = {k:v for k,v in os.environ.items() if not k.startswith(('BIRNAM_','DYLD_'))}
    env.update(BIRNAM_ID_DYLIB=str(args.floor.resolve()), BIRNAM_TEST_CLOSURE_CONTROL='1',
               BIRNAM_DEV_COUNTED_FRAME_INDICES=str(int(args.counted_frame_indices)))
    manifest = args.prefix/'share/birnam/rust-install.json'
    published = json.loads(manifest.read_text())
    assert published['publication']['state'] == 'published'
    artifacts = {str(args.prefix/name): digest for name,digest in published['staged_manifest'].items()}
    artifacts.update({str(path):sha(path) for path in (args.floor,args.tcc,args.normal_floor,args.normal_tcc,args.codegen,manifest)})
    result = dict(status='running', qualified=False, profile=args.profile, numeric_body=args.numeric_body,
                  counted_frame_indices=args.counted_frame_indices, sources=sources, artifacts=artifacts, rows=[],
                  scope='Copied published host with explicit TCC, Codegen and test-floor overrides. Three source closure lifetimes/process, real optimization activity, weak owner and private/baseline compiler release; synchronous/resumable vtable and Code identity plus method edits after eviction. Not fresh installed-production or timing evidence.')
    def save(): args.output.write_text(json.dumps(result,indent=2)+'\n')
    def current():
        assert all(sha(ROOT/path)==digest for path,digest in sources.items())
        assert all(sha(path)==digest for path,digest in artifacts.items())
    try:
        current();save()
        with tempfile.TemporaryDirectory(prefix='birnam-unit-cache-') as tmp:
            layout=Path(tmp);(layout/'bin').mkdir();(layout/'lib').mkdir()
            shutil.copy2(args.prefix/'bin/birnam',layout/'bin/birnam');(layout/'share').symlink_to(args.prefix.resolve()/'share',target_is_directory=True)
            overrides={'libbirnam_id_tcc.dylib':args.tcc.resolve(),'libbirnam_id_codegen.dylib':args.codegen.resolve()}
            for original in (args.prefix.resolve()/'lib').iterdir():(layout/'lib'/original.name).symlink_to(overrides.get(original.name,original))
            cases = [('lifetime',precise,escaping,enabled) for precise in (0,1) for escaping in (0,1) for enabled in (0,1)]
            cases += [('identity',precise,synchronous,1) for precise in (0,1) for synchronous in (0,1)]
            for kind,precise,variant,enabled in cases:
                current()
                code=lifetime(precise,variant,numeric=args.numeric_body) if kind=='lifetime' else identity(precise,variant)
                row_env=dict(env,BIRNAM_DEV_CLOSURE_ELISION=str(enabled))
                selected_tcc=args.tcc if kind=='lifetime' else args.normal_tcc
                selected_floor=args.floor if kind=='lifetime' else args.normal_floor
                (layout/'lib/libbirnam_id_tcc.dylib').unlink()
                (layout/'lib/libbirnam_id_tcc.dylib').symlink_to(selected_tcc.resolve())
                row_env['BIRNAM_ID_DYLIB']=str(selected_floor.resolve())
                if kind=='identity': row_env.pop('BIRNAM_TEST_CLOSURE_CONTROL')
                process=subprocess.run([str(layout/'bin/birnam'),'--repl'],input=code,cwd=ROOT,
                    env=row_env,text=True,capture_output=True,timeout=240)
                row=dict(floor=str(selected_floor),tcc=str(selected_tcc),case=kind,precise=precise,escaping=variant if kind=='lifetime' else None,synchronous=variant if kind=='identity' else None,enabled=enabled,numeric_body=args.numeric_body,counted_frame_indices=args.counted_frame_indices,source=code,source_sha256=hashlib.sha256(code.encode()).hexdigest(),exit_code=process.returncode,stdout=process.stdout,stderr=process.stderr)
                result['rows'].append(row);save();validate(row);save()
                if kind == 'lifetime' and enabled:
                    off = result['rows'][-2]
                    assert off['case'] == kind and off['precise'] == precise and off['escaping'] == variant and not off['enabled']
                    assert [values[2] for values in off['numeric_activity']] == [values[2] for values in row['numeric_activity']], 'logical allocation accounting changed'
                print('unit-cache',args.profile,kind,precise,variant,enabled,'passed',flush=True)
        if args.numeric_body and args.counted_frame_indices:
            assert any(values[1] > 0 for row in result['rows'] if row['case'] == 'lifetime'
                       for values in row['numeric_activity']), 'counted recovery was not exercised'
        current();result['status']='passed'
    except BaseException as error:result.update(status='failed',error=str(error));raise
    finally:save()


if __name__ == '__main__': main()
