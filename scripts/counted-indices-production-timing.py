#!/usr/bin/env python3
"""Verify and compare counted Frame indices through ordinary Id execution."""
import argparse
import hashlib
import itertools
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
import tempfile
import time

from unboxed_numeric_timing_common import source as numeric_source, peak_rss

ROOT = Path(__file__).resolve().parent.parent
CASES = tuple(itertools.product((3, 16, 32), range(5), (0, 1), (0, 1)))


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def source(operations, shape, precise):
    # Reuse the complete five numeric bodies, including the earlier-result and
    # escaping Cell cases. Preparation and invocation use public Id methods.
    setup = numeric_source(operations).split('let block :=', 1)[0]
    setup = setup.replace('[System preciseGc: true]',
                          '[System preciseGc: ' + ('true' if precise else 'false') + ']')
    expected = 14 if shape == 4 else (11.5 if shape == 2 else 11) + 3 * operations
    changed = 11 + 3 * (operations - 2)
    return setup + f'''[System load: "modules/time.birnam"];
let portable := [[source codeObject] withoutSourceProvenance];
let materializeStart := [Time monotonicMicros];
let block := [Codegen materializeResumable: portable captures: #[]];
let materializeTime := [[Time monotonicMicros] - materializeStart];
[[[block codeObject] == portable] ifTrue: [| nil] ifFalse: [| [OS exit: 60]]];
let input := [[shape = 2] ifTrue: [| 11.5] ifFalse: [| [11 clone]]];
let other := 3;
let last := [nil asCell];
let effects := [0 asCell];
let expected := [{expected} asCell];
let check := [:answer |
  [[answer == input] ifTrue: [| [OS exit: 61]] ifFalse: [| nil]];
  [[answer == other] ifTrue: [| [OS exit: 62]] ifFalse: [| nil]];
  let value := [[shape = 3] ifTrue: [| [answer value]] ifFalse: [| answer]];
  [[value = [expected value]] ifTrue: [| nil] ifFalse: [| [OS exit: 63]]]
];
let invoke := [|
  let previous := [last value];
  let answer := [block value: input value: other];
  [[answer == previous] ifTrue: [| [OS exit: 64]] ifFalse: [| nil]];
  [check value: answer];
  [last set: answer]
];
let coldStart := [Time monotonicMicros];
[128 timesRepeat: invoke];
let coldTime := [[Time monotonicMicros] - coldStart];
[[shape = 1] ifTrue: [|
  [Reflect defineMethod: #+ on: input as: [:self :right | [effects inc]; [11 - right]]];
  [expected set: {changed}]
] ifFalse: [| nil]];
[128 timesRepeat: invoke];
[System gc];
[check value: [last value]];
[effects set: 0];
[System println: ["indices-materialize-us " , [materializeTime printString]]];
[System println: ["indices-cold-us " , [coldTime printString]]];
[batches timesRepeat: [|
  let start := [Time monotonicMicros];
  [128 timesRepeat: invoke];
  let elapsed := [[Time monotonicMicros] - start];
  let gcStart := [Time monotonicMicros];
  [System gc];
  let gcElapsed := [[Time monotonicMicros] - gcStart];
  [check value: [last value]];
  [System println: ["indices-sample-us " , [elapsed printString]]];
  [System println: ["indices-gc-us " , [gcElapsed printString]]]
]];
let expectedEffects := [[shape = 1] ifTrue: [| [batches * 128]] ifFalse: [| 0]];
[[[effects value] = expectedEffects] ifTrue: [| nil] ifFalse: [| [OS exit: 65]]];
[[shape = 1] ifTrue: [|
  [Reflect defineMethod: #+ on: input as: [:self :right | [11 + right]]];
  [expected set: {expected}];
  [128 timesRepeat: invoke];
  [[[effects value] = expectedEffects] ifTrue: [| nil] ifFalse: [| [OS exit: 66]]]
] ifFalse: [| nil]];
[Codegen evictMaterializations];
[System gc];
[check value: [last value]];
[invoke value];
[System println: "indices-production-ok"];
'''


def parsed(output, batches):
    result = {}
    for label, count in (('materialize', 1), ('cold', 1), ('sample', batches), ('gc', batches)):
        values = [int(v) for v in re.findall(r'^indices-' + label + r'-us (\d+)$', output, re.M)]
        if len(values) != count:
            raise RuntimeError('missing or extra ' + label + ' measurements')
        result[label + '_us'] = values
    if output.splitlines().count('indices-production-ok') != 1 or any(
            marker in output for marker in ('EvaluationRejected', 'error(')):
        raise RuntimeError('Id numeric driver did not finish correctly')
    result.update(execution_us=sum(result['sample_us']),
                  including_gc_us=sum(result['sample_us']) + sum(result['gc_us']),
                  cold_execution_us=result['cold_us'][0], materialization_us=result['materialize_us'][0])
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', required=True, type=Path)
    parser.add_argument('--receipt', required=True, type=Path)
    parser.add_argument('--counted-receipt', required=True, type=Path,
                        help='passing 60-case counted numeric release source matrix')
    parser.add_argument('--verify', action='store_true')
    parser.add_argument('--floor', type=Path, help='test floor for driver verification only')
    parser.add_argument('--verified', type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--batches', type=int, default=100)
    parser.add_argument('--pairs', type=int, default=4)
    args = parser.parse_args()
    if args.output.exists():
        parser.error('preserve existing evidence; choose a new output')
    if not 1 <= args.batches <= 500 or not 1 <= args.pairs <= 8:
        parser.error('invalid measurement bounds')
    if args.verify and not args.floor or not args.verify and (not args.verified or args.floor):
        parser.error('verification needs --floor; timing needs --verified and no test floor')
    proof = json.loads(args.receipt.read_text())
    if proof.get('status') != 'passed' or proof.get('production_verification') != 'passed':
        parser.error('full production correctness must pass first')
    sources = dict(proof['sources'])
    sources['scripts/counted-indices-production-timing.py'] = sha(__file__)
    sources['scripts/unboxed_numeric_timing_common.py'] = sha(ROOT / 'scripts/unboxed_numeric_timing_common.py')
    sources['scripts/bench-host-check.sh'] = sha(ROOT / 'scripts/bench-host-check.sh')
    if any(sha(ROOT / n) != h for n, h in proof['sources'].items()):
        parser.error('production sources changed')
    counted = json.loads(args.counted_receipt.read_text())
    coordinates = set(itertools.product((3, 16, 32), range(5), (0, 1), (0, 1)))
    if (counted.get('status') != 'passed' or counted.get('profile') != 'release'
            or len(counted.get('rows', [])) != 60
            or {(r['operations'], r['shape'], r['counted'], r['unboxed']) for r in counted['rows']} != coordinates):
        parser.error('counted numeric source matrix is incomplete')
    if any(sha(ROOT / n) != h for n, h in counted['sources'].items()):
        parser.error('counted numeric source matrix is stale')
    prefix = args.prefix.resolve()
    manifest_path = prefix / 'share/birnam/rust-install.json'
    manifest = json.loads(manifest_path.read_text())
    if manifest['profile'] != 'release' or manifest['publication']['state'] != 'published':
        parser.error('published normal release installation required')
    installed = dict(manifest['staged_manifest'])
    installed['share/birnam/rust-install.json'] = sha(manifest_path)
    shipped = {n.removeprefix('share/birnam/src/'): h for n, h in installed.items()
               if n.startswith('share/birnam/src/')}
    if not shipped or any(sha(ROOT / n) != h for n, h in shipped.items()):
        parser.error('installed source differs from verified source')
    sources.update(shipped)
    floor_sha = proof['unboxed_source_matrices']['unboxed-measure-release.json']['floor_sha256']
    if args.verify and sha(args.floor) != floor_sha:
        parser.error('test floor differs from full production proof')
    fixtures = {f'{n}:{s}:{gc}': hashlib.sha256(source(n, s, gc).encode()).hexdigest()
                for n, s, gc in itertools.product((3, 16, 32), range(5), (0, 1))}
    bindings = dict(sources=sources, installed_sha256=installed, fixture_sha256=fixtures,
                    correctness_sha256=sha(args.receipt), counted_correctness_sha256=sha(args.counted_receipt),
                    test_floor_sha256=floor_sha)
    if not args.verify:
        verified = json.loads(args.verified.read_text())
        expected = {(n, s, gc, u, c, test) for n, s, gc, u in CASES
                    for c, test in itertools.product((0, 1), (False, True))}
        if (verified.get('status') != 'passed' or verified.get('verification') is not True
                or any(verified.get(k) != v for k, v in bindings.items())
                or len(verified.get('rows', [])) != len(expected)
                or {(r['operations'], r['shape'], r['precise'], r['unboxed'], r['counted'], r['test_floor'])
                    for r in verified['rows']} != expected):
            parser.error('complete matching test/normal-floor driver verification required')
        for row in verified['rows']:
            if row['exit_code'] != 0:
                parser.error('driver verification contains failed process')
            parsed(row['stdout'], 2)
    result = dict(**bindings, status='running', qualified=False, verification=args.verify,
                  prefix=str(prefix), batches=2 if args.verify else args.batches,
                  pairs=1 if args.verify else args.pairs, rows=[], host_observations=[],
                  scope='Ordinary Id calls and Time/System gc on the normal release floor. Five unchanged numeric bodies: final value, receiver-specific effectful operator replacement followed by a second Id replacement restoring the arithmetic result, Float input, escaping Cell, earlier result. The second Id replacement does not restore the canonical vtable or native method identity; exact native replacement/restoration remains in the prerequisite numeric counter matrix. Exact values and fresh identities checked on each call and after GC, plus replacement effect counts and retained code after eviction. Both GC modes and unboxing settings; counted indices independently off/on. Verification compares test and normal floors; numeric allocation savings come from the separately bound source matrix. Timings include Id checks and driver overhead, have no test probes, skip quiet-host admission under the user waiver, and remain exploratory. This does not measure default-path overhead against pre-change binaries or establish application-wide profitability.')
    if not args.verify:
        result['verification_sha256'] = sha(args.verified)
    env = {k: v for k, v in os.environ.items() if not k.startswith(('BIRNAM_', 'DYLD_', 'BENCH_HOST_'))}

    def save():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        temporary = args.output.with_suffix('.json.tmp')
        temporary.write_text(json.dumps(result, indent=2) + '\n')
        temporary.replace(args.output)

    def current():
        if (any(sha(ROOT / n) != h for n, h in sources.items())
                or any(sha(prefix / n) != h for n, h in installed.items())
                or sha(args.receipt) != bindings['correctness_sha256']
                or sha(args.counted_receipt) != bindings['counted_correctness_sha256']):
            raise RuntimeError('source, installed artifact or prerequisite changed')
        if args.verify and sha(args.floor) != floor_sha:
            raise RuntimeError('test floor changed')
        if not args.verify and sha(args.verified) != result['verification_sha256']:
            raise RuntimeError('driver verification changed')

    try:
        current()
        save()
        with tempfile.TemporaryDirectory(prefix='birnam-counted-production-') as work:
            fixture = Path(work) / 'numeric.birnam'
            for n, s, gc, u in CASES:
                fixture.write_text(source(n, s, gc))
                for pair in range(result['pairs']):
                    for c in ((0, 1) if pair % 2 == 0 else (1, 0)):
                        for test in ((True, False) if args.verify else (False,)):
                            current()
                            extra = dict(BIRNAM_NUMERIC_SHAPE=str(s), BIRNAM_NUMERIC_BATCHES=str(result['batches']),
                                         BIRNAM_DEV_UNBOXED_NUMERIC=str(u), BIRNAM_DEV_COUNTED_FRAME_INDICES=str(c))
                            if test:
                                extra.update(BIRNAM_TEST_UNBOXED_NUMERIC='1', BIRNAM_ID_DYLIB=str(args.floor.resolve()))
                            argv = ['/usr/bin/time', '-l', str(prefix / 'bin/birnam'), str(fixture)]
                            start = time.monotonic_ns()
                            p = subprocess.run(argv, cwd=ROOT, env=dict(env, **extra), capture_output=True,
                                               text=True, timeout=1800)
                            row = dict(operations=n, shape=s, precise=gc, unboxed=u, counted=c,
                                       test_floor=test, pair=pair, exit_code=p.returncode,
                                       process_wall_ns=time.monotonic_ns() - start, stdout=p.stdout, stderr=p.stderr)
                            result['rows'].append(row)
                            save()
                            if p.returncode:
                                raise RuntimeError('normal Id numeric process failed')
                            row.update(parsed(p.stdout, result['batches']), peak_rss_bytes=peak_rss(p.stderr))
                            save()
                            print(n, s, gc, u, pair, c, test, 'passed', flush=True)
                    if not args.verify:
                        p = subprocess.run(['bash', 'scripts/bench-host-check.sh'], cwd=ROOT,
                                           env=env, text=True, capture_output=True)
                        result['host_observations'].append(dict(operations=n, shape=s, precise=gc,
                            unboxed=u, pair=pair, exit_code=p.returncode, output=p.stdout + p.stderr))
                        save()
        current()
        result['comparisons'] = []
        for n, s, gc, u in CASES:
            for metric in ('execution_us', 'including_gc_us', 'cold_execution_us', 'materialization_us',
                           'process_wall_ns', 'peak_rss_bytes'):
                changes = []
                for pair in range(result['pairs']):
                    rows = {r['counted']: r for r in result['rows'] if not r['test_floor'] and
                            (r['operations'], r['shape'], r['precise'], r['unboxed'], r['pair']) == (n, s, gc, u, pair)}
                    if rows[0][metric] > 0:
                        changes.append(100 * (1 - rows[1][metric] / rows[0][metric]))
                if changes:
                    result['comparisons'].append(dict(operations=n, shape=s, precise=gc, unboxed=u,
                        metric=metric, paired_reduction_percent=changes,
                        median_reduction_percent=statistics.median(changes)))
        result['status'] = 'passed'
    except BaseException as error:
        result.update(status='failed', error=str(error))
        raise
    finally:
        save()


if __name__ == '__main__':
    main()
