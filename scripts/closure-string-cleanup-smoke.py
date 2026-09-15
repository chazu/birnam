#!/usr/bin/env python3
"""Collect real numeric-closure deferred allocations beside String finalizers."""
import argparse
import hashlib
import itertools
import json
import os
from pathlib import Path
import re
import runpy
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('prefix', 'floor', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--quick', action='store_true')
    args = parser.parse_args()
    assert not args.output.exists(), 'preserve existing evidence'
    source = runpy.run_path(str(ROOT / 'scripts/closure-sequence-smoke.py'))['source']
    sha = lambda p: hashlib.sha256(Path(p).read_bytes()).hexdigest()
    paths = [Path(__file__).resolve(), ROOT / 'scripts/closure-sequence-smoke.py']
    paths += [p for p in (ROOT / 'rust').rglob('*') if p.is_file()]
    sources = {str(p.relative_to(ROOT)): sha(p) for p in paths}
    prefix = args.prefix.resolve()
    manifest = prefix / 'share/birnam/rust-install.json'
    publication = json.loads(manifest.read_text())
    assert publication['publication']['state'] == 'published'
    installed = dict(publication['staged_manifest'])
    installed['share/birnam/rust-install.json'] = sha(manifest)
    floor = args.floor.resolve()
    result = dict(status='running', qualified=False, sources=sources, installed=installed,
                  floor_sha256=sha(floor), quick=args.quick, rows=[],
                  scope='Actual compiled numeric closure calls, counted indices and post-work GC beside exact, observing or late-replaced owned String cleanup. Existing sequence result/recovery/accounting checks remain; additionally verify settlement, callback observations, storage release and retained escaped closures. No timing claim.')
    env = {k: v for k, v in os.environ.items() if not k.startswith(('BIRNAM_', 'DYLD_'))}
    env.update(BIRNAM_ID_DYLIB=str(floor), BIRNAM_TEST_CLOSURE_CONTROL='1',
               BIRNAM_DEV_CLOSURE_FORCE_RECOVERY='0', BIRNAM_DEV_CLOSURE_REPORT='0')

    def current():
        assert sha(floor) == result['floor_sha256']
        assert all(sha(ROOT / n) == h for n, h in sources.items())
        assert all(sha(prefix / n) == h for n, h in installed.items())

    def save():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2) + '\n')

    def values(lines, prefix):
        found = [s for s in lines if s.startswith(prefix + ' ')]
        assert len(found) == 1, (prefix, lines)
        return {k: int(v) for k, v in re.findall(r'(\w+)=(\d+)', found[0])}

    try:
        with tempfile.TemporaryDirectory(prefix='closure-string-cleanup-') as work:
            fixture = Path(work) / 'numeric.birnam'
            modes = ((False, -1), (True, -1), (True, 0), (True, 1), (True, "last"))
            for precise, calls, escaping, cleanup, (enabled, force) in itertools.product(
                    (0, 1), (3,) if args.quick else (3, 33), (False, True), (1, 2, 3), modes):
                force = min(calls, 32) - 1 if force == 'last' else force
                code = source(calls, escaping, precise, numeric=True, numeric_copy_region=True)
                fixture.write_text(code)
                pair = []
                for counted in (False, True):
                    current()
                    extra = dict(BIRNAM_TEST_CLOSURE_STRING_CLEANUP=str(cleanup),
                                 BIRNAM_DEV_CLOSURE_ELISION=str(int(enabled)), BIRNAM_DEV_CLOSURE_FORCE_AT=str(force),
                                 BIRNAM_DEV_COUNTED_FRAME_INDICES=str(int(counted)))
                    run = subprocess.run([str(prefix / 'bin/birnam'), str(fixture)], cwd=ROOT,
                                         env=dict(env, **extra), text=True, capture_output=True, timeout=180)
                    row = dict(precise=precise, calls=calls, escaping=escaping, cleanup=cleanup,
                               enabled=enabled, force=force, counted=counted, source_sha256=hashlib.sha256(code.encode()).hexdigest(),
                               exit_code=run.returncode, stdout=run.stdout, stderr=run.stderr)
                    result['rows'].append(row)
                    save()
                    assert run.returncode == 0, row
                    lines = run.stdout.splitlines()
                    v = row['sequence'] = values(lines, 'closure-sequence')
                    c = row['collection'] = values(lines, 'closure-cleanup')
                    forced = enabled and force >= 0
                    assert v['calls'] == calls and v['escaping'] == int(escaping) and v['invocations'] == 8
                    assert v['bytes'] > 0 and v['attempts'] == (8 if enabled else 0)
                    assert v['hits'] == (8 if enabled and not forced else 0)
                    assert v['completed_calls'] == (8 * (force if forced else min(calls, 32)) if enabled else 0)
                    assert v['materialized'] == (8 if enabled and not forced and (escaping or calls > 32) else 0)
                    assert (v['counted_indices'] > 0) if counted else v['counted_indices'] == 0
                    assert v['physical_saved'] - v['counted_indices'] == (8 if enabled and not forced and not escaping and calls <= 32 else 0)
                    assert c['mode'] == cleanup and c['pending'] == v['physical_saved']
                    assert c['settled'] == (0 if cleanup == 1 else c['pending'])
                    assert c['callbacks'] == int(cleanup != 1) and c['string_released'] == 1
                    assert c['reclaimed'] > 0 and c['after_live'] == c['before_live'] - c['reclaimed']
                    assert 0 < c['after_bytes'] < c['before_bytes']
                    if cleanup == 1:
                        assert c['callback_live'] == c['callback_bytes'] == 0
                    else:
                        # Sweep addresses/order are intentionally unspecified.
                        # Compare the callback's own remaining sweep, rather than
                        # imposing equal positions across independent processes.
                        assert c['callback_live'] == c['after_live'] + c['remaining_dead']
                        assert c['callback_bytes'] == c['after_bytes'] + c['remaining_dead_bytes']
                    pair.append(row)
                    save()
                a, b = pair
                av, bv = dict(a['sequence']), dict(b['sequence'])
                for v in (av, bv):
                    v['physical_saved'] -= v.pop('counted_indices')
                assert av == bv, pair
                observable = lambda row: {k: v for k, v in row['collection'].items() if k not in ('pending', 'settled', 'callback_live', 'callback_bytes', 'remaining_dead', 'remaining_dead_bytes')}
                assert observable(a) == observable(b), pair
                print(f'closure cleanup: precise={precise} calls={calls} escaping={escaping} cleanup={cleanup} enabled={enabled} force={force} passed', flush=True)
        current()
        result['status'] = 'passed'
    except BaseException as error:
        result.update(status='failed', error=str(error))
        raise
    finally:
        save()


if __name__ == '__main__':
    main()
