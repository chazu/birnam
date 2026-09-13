#!/usr/bin/env python3
"""Verify installed loop cost counters and retained cache bounds before timing."""
import argparse
import hashlib
from itertools import product
import json
import os
from pathlib import Path
import subprocess
import tempfile
from loop_timing_common import source, parse_output, check_pair
from unboxed_numeric_timing_common import peak_rss

ROOT = Path(__file__).resolve().parent.parent


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', required=True, type=Path)
    parser.add_argument('--floor', required=True, type=Path)
    parser.add_argument('--profile', required=True, choices=('debug', 'release'))
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--batches', type=int, default=2)
    args = parser.parse_args()
    if not 1 <= args.batches <= 500:
        parser.error('batches must be between 1 and 500')
    floor, prefix = args.floor.resolve(), args.prefix.resolve()
    paths = [Path(__file__).resolve(), ROOT / 'scripts/loop_timing_common.py',
             ROOT / 'scripts/unboxed_numeric_timing_common.py',
             ROOT / 'rust/birnam-id/src/runtime/numeric/chain.rs',
             ROOT / 'rust/birnam-id/src/runtime/numeric/chain/loop_reuse.rs',
             ROOT / 'rust/birnam-id/src/runtime/native_tier.rs',
             ROOT / 'rust/birnam-id/src/runtime/native_tier/fused_numeric.rs',
             ROOT / 'rust/birnam-id/src/runtime/unboxed_numeric_probe.rs',
             ROOT / 'rust/birnam-id/src/runtime/unboxed_numeric_probe/loop_measure.rs']
    installed = [prefix / 'bin/birnam', prefix / 'share/birnam/rust-install.json'] + sorted((prefix / 'lib').glob('*.dylib'))
    receipt = dict(status='running', profile=args.profile, batches=args.batches, floor_sha256=sha(floor),
                   source_sha256={str(p.relative_to(ROOT)): sha(p) for p in paths},
                   installed_sha256={str(p): sha(p) for p in installed}, rows=[],
                   scope='Measurement correctness only. Real Id loops, both GC modes, boxed/unboxed arithmetic; cache capacity is stable across complete loops and collections. No timing qualification.')
    env = {k: v for k, v in os.environ.items() if not k.startswith(('BIRNAM_', 'DYLD_'))}
    env.update(BIRNAM_ID_DYLIB=str(floor), BIRNAM_TEST_UNBOXED_NUMERIC='1', BIRNAM_LOOP_BATCHES=str(args.batches))
    try:
        with tempfile.TemporaryDirectory(prefix='birnam-loop-measure-') as work:
            fixture = Path(work) / 'measure.birnam'
            for operations, shape, precise, unboxed in product((3, 16, 32), range(4), (0, 1), (0, 1)):
                code = source(operations, shape, precise)
                fixture.write_text(code)
                pair = []
                for enabled in (0, 1):
                    env.update(BIRNAM_DEV_LOOP_INVARIANTS=str(enabled), BIRNAM_DEV_UNBOXED_NUMERIC=str(unboxed))
                    process = subprocess.run(['/usr/bin/time', '-l', str(prefix / 'bin/birnam'), str(fixture)],
                                             cwd=ROOT, env=env, text=True, capture_output=True)
                    assert process.returncode == 0, (shape, precise, unboxed, enabled, process.stdout, process.stderr)
                    values = parse_output(process.stdout)
                    pair.append(values)
                    receipt['rows'].append(dict(operations=operations, shape=shape, precise=precise, unboxed=unboxed, enabled=enabled,
                                                source_sha256=hashlib.sha256(code.encode()).hexdigest(),
                                                peak_rss_bytes=peak_rss(process.stderr), **values))
                check_pair(*pair, batches=args.batches, unboxed=unboxed)
                print(f'loop-measurement-smoke: profile={args.profile} operations={operations} shape={shape} precise={precise} unboxed={unboxed} passed', flush=True)
        assert sha(floor) == receipt['floor_sha256']
        assert all(sha(ROOT / p) == h for p, h in receipt['source_sha256'].items())
        assert all(sha(p) == h for p, h in receipt['installed_sha256'].items())
        receipt['status'] = 'passed'
    except BaseException as error:
        receipt.update(status='failed', error=str(error))
        raise
    finally:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(receipt, indent=2) + '\n')


if __name__ == '__main__':
    main()
