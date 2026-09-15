#!/usr/bin/env python3
"""Check counted indices composed with aggregate allocation and field consumers."""
from validation_support import save_json
from measurement_archive import read_bytes
import argparse
import hashlib
import importlib.util
import itertools
import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location('aggregate_source', ROOT / 'scripts/aggregate-source-smoke.py')
aggregate = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(aggregate)
MODES = ((), ('discarded-clones',), ('virtual-fields',), ('discarded-clones', 'virtual-fields'))


def sha(path):
    return hashlib.sha256(read_bytes(Path(path))).hexdigest()


def compare_pair(off, on):
    """Admit using only pre-work counts; never retry a comparable outcome failure."""
    phases = off['measurements'].keys()
    if any(off['measurements'][phase][key] != on['measurements'][phase][key]
           for phase in phases for key in ('live_before', 'live_bytes_before')):
        return False
    for phase in phases:
        a, b = off['measurements'][phase], on['measurements'][phase]
        assert a['counted_indices'] == 0
        assert b['physical_saved'] - a['physical_saved'] == b['counted_indices'], (phase, a, b)
        for key in ('invocations', 'bytes', 'logical_allocations', 'completed', 'materialized', 'answer'):
            assert a[key] == b[key], (phase, key, a, b)
        assert off['collection'][phase] == on['collection'][phase], (off, on)
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path, required=True)
    parser.add_argument('--floor', type=Path, required=True)
    parser.add_argument('--profile', choices=('debug', 'release'), required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    assert not args.output.exists(), 'preserve existing evidence'
    prefix, floor = args.prefix.resolve(), args.floor.resolve()
    paths = [Path(__file__).resolve(), ROOT / 'scripts/aggregate-source-smoke.py']
    for directory in ('rust', 'modules'):
        paths += sorted(p for p in (ROOT / directory).rglob('*') if p.is_file())
    installed = [prefix / 'bin/birnam', prefix / 'share/birnam/rust-install.json']
    installed += sorted((prefix / 'lib').glob('*.dylib'))
    result = dict(status='running', qualified=False, profile=args.profile,
                  floor=str(floor), floor_sha256=sha(floor),
                  sources={str(p.relative_to(ROOT)): sha(p) for p in paths},
                  installed={str(p): sha(p) for p in installed}, rows=[], attempts=[],
                  scope='Focused composition verification, not the prerequisite production gate.240 accepted installed REPL process rows per profile:3projection counts x5shapes x2GC modes x4aggregate consumer settings x counted indices off/on. Reuses the canonical aggregate fixture and every original shape, identity, effect, byte and aggregate-savings assertion. Also compares original logical allocation counts, collection results and post-GC live counts/bytes between index modes. Pairs must start with equal per-phase live counts/bytes. Retain all attempts; at most3attempts per coordinate. Never retry an outcome failure from equal starting counts. No timing claim.')
    env = {k: v for k, v in os.environ.items() if not k.startswith(('BIRNAM_', 'DYLD_'))}
    env.update(BIRNAM_ID_DYLIB=str(floor), BIRNAM_TEST_UNBOXED_NUMERIC='1')

    def save():
        save_json(args.output, result)

    def current():
        assert sha(floor) == result['floor_sha256']
        assert all(sha(ROOT / n) == h for n, h in result['sources'].items())
        assert all(sha(n) == h for n, h in result['installed'].items())

    try:
        save()
        for projections, shape, precise, disabled in itertools.product((1, 3, 16), range(5), (0, 1), MODES):
            code = aggregate.source(projections, shape, precise, quiet_repl=True)
            for attempt_number in range(3):
                pair = []
                attempt = dict(projections=projections, shape=shape, precise=precise,
                               disabled_consumers=disabled, attempt=attempt_number, status='running', rows=[])
                result['attempts'].append(attempt)
                for counted in (0, 1):
                    current()
                    extra = dict(BIRNAM_DEV_COUNTED_FRAME_INDICES=str(counted),
                                 BIRNAM_DEV_DISCARDED_CLONES=str(int('discarded-clones' not in disabled)),
                                 BIRNAM_DEV_VIRTUAL_FIELDS=str(int('virtual-fields' not in disabled)))
                    argv = [str(prefix / 'bin/birnam'), '--repl']
                    process = subprocess.run(argv, input=code, cwd=ROOT, env=dict(env, **extra),
                                             text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
                    row = dict(projections=projections, shape=shape, precise=precise,
                               disabled_consumers=disabled, counted=counted, argv=argv,
                               source_sha256=hashlib.sha256(code.encode()).hexdigest(), input=code,
                               exit_code=process.returncode, output=process.stdout)
                    attempt['rows'].append(row)
                    save()
                    assert process.returncode == 0, row
                    aggregate.check_output(row, disabled, bool(counted))
                    gc = [dict((k, int(v)) for k, v in re.findall(r'(\w+)=(-?\d+)', line))
                          for line in process.stdout.splitlines() if line.startswith('unboxed-numeric-gc ')]
                    assert len(gc) == 4, row
                    row['collection'] = dict(zip(row['measurements'], gc))
                    for phase, m in row['measurements'].items():
                        g = row['collection'][phase]
                        assert g['live_objects'] == m['live_before'] + m['logical_allocations'] - g['reclaimed_objects'], (phase, m, g)
                    pair.append(row)
                    save()
                # Source values, identities, effects and every original aggregate
                # assertion above are mandatory for each attempted process too.
                # Admission uses only the state recorded before measured work.
                if not compare_pair(*pair):
                    attempt['status'] = 'incomparable_starting_heaps'
                    save()
                    continue
                attempt['status'] = 'passed'
                result['rows'].extend(pair)
                save()
                break
            else:
                raise AssertionError(('no comparable starting heaps after3attempts', projections, shape, precise, disabled))
            print(f'counted-frame-aggregate: profile={args.profile} projections={projections} shape={shape} precise={precise} disabled={disabled} passed', flush=True)
        current()
        assert len(result['rows']) == 240
        assert any(m['counted_indices'] > 0 for r in result['rows'] for m in r['measurements'].values())
        result['status'] = 'passed'
    except BaseException as error:
        result.update(status='failed', error=str(error))
        raise
    finally:
        save()


if __name__ == '__main__':
    main()
