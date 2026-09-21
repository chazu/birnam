#!/usr/bin/env python3
"""Verify raw arithmetic reuse in source-created Id tail loops and call controls."""
from measurement_archive import read_bytes
import argparse
import hashlib
from itertools import product
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def sha(path):
    return hashlib.sha256(read_bytes(Path(path))).hexdigest()


def source(shape, precise):
    setup = f'''[System preciseGc: {'true' if precise else 'false'}];
let numericSource := [:a :b | let first := [a + a]; let second := [first + b]; [second + a]];
let numeric := [Codegen materializeResumable: [[numericSource codeObject] withoutSourceProvenance] captures: #[]];
[128 timesRepeat: [| [numeric value: 11 value: 3]]];
let before := [UnboxedNumericProbe loopReused];
'''
    if shape == 3:
        body = '''let result := [nil asCell];
[128 timesRepeat: [| [result set: [numeric value: 11 value: 3]]]];
[System println: [result value]];
'''
    else:
        a, b = [("11", "3"), ("11", "index"), ("index", "index")][shape]
        body = f'''let stop := [:again :calc :index :previous | previous];
let loopSource := [:again :calc :index :previous |
  let result := [calc value: {a} value: {b}];
  [[result == previous] ifTrue: [| [OS exit: 1]] ifFalse: [| nil]];
  let next := [[index < 127] ifTrue: [| again] ifFalse: [| stop]];
  [next value: again value: calc value: [index + 1] value: result]
];
let loop := [Codegen materializeResumable: [[loopSource codeObject] withoutSourceProvenance] captures: #[stop]];
[System println: [loop value: loop value: numeric value: 0 value: nil]];
'''
    return setup + body + '[System println: [[UnboxedNumericProbe loopReused] - before]];\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', required=True, type=Path)
    parser.add_argument('--floor', required=True, type=Path)
    parser.add_argument('--profile', required=True, choices=('debug', 'release'))
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--counted-frame-indices', action='store_true',
                        help='enable counted body index allocations during the unchanged control checks')
    args = parser.parse_args()
    floor, prefix = args.floor.resolve(), args.prefix.resolve()
    paths = [Path(__file__).resolve(), ROOT / 'rust/birnam-codegen/src/numeric_chain.rs',
             ROOT / 'rust/birnam-runtime/src/execution.rs', ROOT / 'rust/birnam-runtime/src/execution/tail_entry.rs',
             ROOT / 'rust/birnam-runtime/src/execution/loop_iteration.rs',
             ROOT / 'rust/birnam-runtime/src/runtime/numeric/chain.rs',
             ROOT / 'rust/birnam-runtime/src/runtime/numeric/chain/loop_reuse.rs',
             ROOT / 'rust/birnam-runtime/src/runtime/unboxed_numeric_probe.rs']
    paths += [ROOT / 'rust/birnam-runtime/src/runtime/native_tier/regions.rs',
              ROOT / 'rust/birnam-runtime/src/heap/dead_allocations/frame_indices.rs']
    installed = [prefix / 'bin/birnam', prefix / 'share/birnam/rust-install.json'] + sorted((prefix / 'lib').glob('*.dylib'))
    receipt = dict(status='running', profile=args.profile, floor_sha256=sha(floor),
                   source_sha256={str(p.relative_to(ROOT)): sha(p) for p in paths},
                   installed_sha256={str(p): sha(p) for p in installed}, rows=[],
                   scope='Source loops through installed Codegen; raw kernel prediction work reused, not result Oop identity. No timing qualification.')
    env = {k: v for k, v in os.environ.items() if not k.startswith(('BIRNAM_', 'DYLD_'))}
    env.update(BIRNAM_ID_DYLIB=str(floor), BIRNAM_TEST_UNBOXED_NUMERIC='1')
    receipt['counted_frame_indices'] = args.counted_frame_indices
    if args.counted_frame_indices:
        env['BIRNAM_DEV_COUNTED_FRAME_INDICES'] = '1'
    try:
        with tempfile.TemporaryDirectory(prefix='birnam-loop-invariant-') as work:
            fixture = Path(work) / 'loop.birnam'
            for shape, precise, unboxed, enabled in product(range(4), (0, 1), (0, 1), (0, 1)):
                code = source(shape, precise)
                fixture.write_text(code)
                env.update(BIRNAM_DEV_LOOP_INVARIANTS=str(enabled), BIRNAM_DEV_UNBOXED_NUMERIC=str(unboxed))
                process = subprocess.run([str(prefix / 'bin/birnam'), str(fixture)], cwd=ROOT, env=env, text=True, capture_output=True)
                assert process.returncode == 0, (shape, precise, unboxed, enabled, process.stdout, process.stderr)
                values = [int(line) for line in process.stdout.splitlines()]
                assert values == [[36, 160, 508, 36][shape], ([381, 127, 0, 0][shape] if enabled else 0)], (shape, precise, unboxed, enabled, values)
                receipt['rows'].append(dict(shape=shape, precise=precise, unboxed=unboxed, enabled=enabled,
                                            source_sha256=hashlib.sha256(code.encode()).hexdigest(), answer=values[0], predicted_operations_reused=values[1]))
                print(f'loop-invariant-numeric-smoke: profile={args.profile} shape={shape} precise={precise} unboxed={unboxed} enabled={enabled} passed', flush=True)
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
