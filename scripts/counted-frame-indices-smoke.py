#!/usr/bin/env python3
"""Compare counted Frame indices against physical allocation on installed source."""
from validation_support import save_json
from measurement_archive import read_bytes
import argparse
import copy
import hashlib
import itertools
import json
import os
from pathlib import Path
import subprocess
import tempfile
from unboxed_numeric_timing_common import source, parse_output, check_pair

ROOT = Path(__file__).resolve().parent.parent


def sha(path):
    return hashlib.sha256(read_bytes(Path(path))).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path, required=True)
    parser.add_argument('--floor', type=Path, required=True)
    parser.add_argument('--profile', choices=('debug', 'release'), required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    assert not args.output.exists(), 'preserve existing evidence'
    prefix, floor = args.prefix.resolve(), args.floor.resolve()
    paths = [Path(__file__).resolve(), ROOT / 'scripts/unboxed_numeric_timing_common.py']
    paths += sorted((ROOT / 'rust/birnam-id').rglob('*.rs'))
    paths += [ROOT / 'rust/birnam-id/Cargo.toml']
    installed = [prefix / 'bin/birnam', prefix / 'share/birnam/rust-install.json']
    installed += sorted((prefix / 'lib').glob('*.dylib'))
    receipt = dict(status='running', qualified=False, profile=args.profile,
                   scope='60 installed-source processes: three chain lengths, five numeric/control shapes, unboxing off/on and counted body indices off/on. Two 128-call batches with precise GC. Exact values and identities checked inside the existing probe; original numeric checks retained after separately accounting for counted indices. No production or timing qualification claim.',
                   floor=str(floor), floor_sha256=sha(floor),
                   sources={str(p.relative_to(ROOT)): sha(p) for p in paths},
                   installed={str(p): sha(p) for p in installed}, rows=[])
    env = {k: v for k, v in os.environ.items() if not k.startswith(('BIRNAM_', 'DYLD_'))}
    env.update(BIRNAM_TEST_UNBOXED_NUMERIC='1', BIRNAM_ID_DYLIB=str(floor), BIRNAM_NUMERIC_BATCHES='2')

    def save():
        save_json(args.output, receipt)

    def current():
        assert sha(floor) == receipt['floor_sha256']
        assert all(sha(ROOT / p) == h for p, h in receipt['sources'].items())
        assert all(sha(p) == h for p, h in receipt['installed'].items())

    try:
        save()
        with tempfile.TemporaryDirectory(prefix='birnam-counted-indices-') as directory:
            fixture = Path(directory) / 'indices.birnam'
            for operations, shape in itertools.product((3, 16, 32), range(5)):
                code = source(operations)
                fixture.write_text(code)
                rows = {}
                for counted, unboxed in itertools.product((0, 1), repeat=2):
                    current()
                    env.update(BIRNAM_DEV_COUNTED_FRAME_INDICES=str(counted),
                               BIRNAM_DEV_UNBOXED_NUMERIC=str(unboxed), BIRNAM_NUMERIC_SHAPE=str(shape))
                    argv = [str(prefix / 'bin/birnam'), str(fixture)]
                    process = subprocess.run(argv, cwd=ROOT, env=env, text=True, capture_output=True)
                    row = dict(operations=operations, shape=shape, counted=counted, unboxed=unboxed,
                               argv=argv, source_sha256=hashlib.sha256(code.encode()).hexdigest(),
                               exit_code=process.returncode, stdout=process.stdout, stderr=process.stderr)
                    receipt['rows'].append(row)
                    save()
                    assert process.returncode == 0, row
                    row.update(parse_output(process.stdout))
                    rows[counted, unboxed] = row
                    save()
                for unboxed in (0, 1):
                    off, on = (rows[c, unboxed]['measure'] for c in (0, 1))
                    assert off['counted_indices'] == 0
                    assert on['counted_indices'] > 0, (operations, shape, unboxed, on)
                    assert on['physical_saved'] - off['physical_saved'] == on['counted_indices']
                    for name in ('calls', 'allocation_bytes', 'logical_allocations', 'reclaimed_objects',
                                 'live_after_gc', 'live_bytes_after_gc', 'completed', 'materialized',
                                 'restored_completed', 'maps', 'metadata_bytes', 'native_units',
                                 'numeric_units', 'scratch_cached', 'scratch_bytes',
                                 'private_native_units', 'private_relocated_bytes'):
                        assert off[name] == on[name], (operations, shape, unboxed, name, off[name], on[name])
                    assert on['journal_retained_bytes'] <= on['journal_peak_bytes']
                for counted in (0, 1):
                    normalized = [copy.deepcopy(rows[counted, u]) for u in (0, 1)]
                    for row in normalized:
                        # Preserve every original numeric assertion; only the
                        # independently checked dead index boxes are subtracted.
                        row['measure']['physical_saved'] -= row['measure']['counted_indices']
                    check_pair(*normalized, batches=2)
                print(f'counted-frame-indices-smoke: profile={args.profile} operations={operations} shape={shape} passed', flush=True)
        current()
        assert len(receipt['rows']) == 60
        receipt['status'] = 'passed'
    except BaseException as error:
        receipt.update(status='failed', error=str(error))
        raise
    finally:
        save()


if __name__ == '__main__':
    main()
