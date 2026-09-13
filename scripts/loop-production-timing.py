#!/usr/bin/env python3
"""Verify Id-driven loops, then time them on the normal production floor."""
import argparse
import hashlib
from itertools import product
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
import tempfile
import time

from loop_timing_common import source as counter_source, parse_output, check_pair
from unboxed_numeric_timing_common import peak_rss

ROOT = Path(__file__).resolve().parent.parent


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def source(operations, shape, counters=False):
    # Keep the already verified numeric, loop and driver factories byte-identical.
    setup = counter_source(operations, shape).split('let batches :=', 1)[0]
    expected = [11 * operations + 3, 11 * operations + 127,
                (operations + 1) * 127, 11 * operations + 3][shape]
    before = 'let reusedBefore := [UnboxedNumericProbe loopReused];' if counters else ''
    after = ('''[System println: ["production-reused " , [[[UnboxedNumericProbe loopReused] - reusedBefore] printString]]];
''' + f'[UnboxedNumericProbe measureLoop: run numeric: numeric operations: {operations} shape: {shape} batches: 1];\n') if counters else ''
    return setup + f'''[System load: "modules/time.birnam"];
let batches := [[System getEnv: "BIRNAM_LOOP_BATCHES"] asInteger];
let coldStart := [Time monotonicMicros];
let coldAnswer := [run value: numeric];
let coldTime := [[Time monotonicMicros] - coldStart];
[[coldAnswer = {expected}] ifTrue: [| nil] ifFalse: [| [OS exit: 61]]];
[System gc];
[3 timesRepeat: [| [run value: numeric]; [System gc]]];
[System println: ["production-cold-us " , [coldTime printString]]];
{before}
[batches timesRepeat: [|
  let start := [Time monotonicMicros];
  let answer := [run value: numeric];
  let elapsed := [[Time monotonicMicros] - start];
  [[answer = {expected}] ifTrue: [| nil] ifFalse: [| [OS exit: 62]]];
  let gcStart := [Time monotonicMicros];
  [System gc];
  let gcElapsed := [[Time monotonicMicros] - gcStart];
  [[answer = {expected}] ifTrue: [| nil] ifFalse: [| [OS exit: 63]]];
  [System println: ["production-sample-us " , [elapsed printString]]];
  [System println: ["production-gc-us " , [gcElapsed printString]]]
]];
{after}
[System println: "production-loop-ok"];
'''


def parsed(output, batches):
    result = {}
    for label, count in (('cold', 1), ('sample', batches), ('gc', batches)):
        values = [int(v) for v in re.findall(r'^production-' + label + r'-us (\d+)$', output, re.MULTILINE)]
        if len(values) != count:
            raise RuntimeError(f'missing or extra {label} samples')
        result[label + '_us'] = values
    if output.splitlines().count('production-loop-ok') != 1 or 'EvaluationRejected' in output or 'error(' in output:
        raise RuntimeError('source loop did not finish correctly')
    result['execution_us'] = sum(result['sample_us'])
    result['cold_execution_us'] = result['cold_us'][0]
    result['including_gc_us'] = result['execution_us'] + sum(result['gc_us'])
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path, required=True)
    parser.add_argument('--receipt', type=Path, required=True, help='full production correctness receipt')
    parser.add_argument('--floor', type=Path, help='verified release test floor, required for --verify')
    parser.add_argument('--verify', action='store_true')
    parser.add_argument('--verified', type=Path, help='passing Id-driven counter/normal-floor matrix')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--batches', type=int, default=500)
    parser.add_argument('--pairs', type=int, default=4)
    parser.add_argument('--wait-seconds', type=int, default=120)
    args = parser.parse_args()
    if not 1 <= args.batches <= 500 or not 1 <= args.pairs <= 8 or args.wait_seconds < 0:
        parser.error('invalid measurement bounds')
    if args.verify and not args.floor or not args.verify and not args.verified:
        parser.error('verification needs --floor; timing needs --verified')
    prefix = args.prefix.resolve()
    proof = json.loads(args.receipt.read_text())
    if proof.get('status') != 'passed' or proof.get('production_verification') != 'passed':
        parser.error('full production verification must pass first')
    if not proof.get('sources') or any(sha(ROOT / n) != h for n, h in proof['sources'].items()):
        parser.error('production source differs from correctness receipt')
    inputs = dict(proof['sources'])
    inputs['scripts/loop-production-timing.py'] = sha(__file__)
    inputs['scripts/bench-host-check.sh'] = sha(ROOT / 'scripts/bench-host-check.sh')
    install_path = prefix / 'share/birnam/rust-install.json'
    install = json.loads(install_path.read_text())
    if install['profile'] != 'release' or install['publication']['state'] != 'published':
        parser.error('normal published release installation required')
    binaries = dict(install['staged_manifest'])
    binaries['share/birnam/rust-install.json'] = sha(install_path)
    shipped = {n.removeprefix('share/birnam/src/'): h for n, h in binaries.items()
               if n.startswith('share/birnam/src/')}
    if not shipped or any(sha(ROOT / n) != h for n, h in shipped.items()):
        parser.error('installed source differs from this checkpoint')
    inputs.update(shipped)
    floor = args.floor.resolve() if args.floor else None
    if args.verify and sha(floor) != proof['unboxed_source_matrices']['loop-measure-release.json']['floor_sha256']:
        parser.error('test floor differs from verified release floor')
    cases = set(product((3, 16, 32), range(4), (0, 1), (0, 1)))
    fixture_hashes = {f'{n}:{s}:{int(c)}': hashlib.sha256(source(n, s, c).encode()).hexdigest()
                      for n, s, c in product((3, 16, 32), range(4), (False, True))}
    if not args.verify:
        verified = json.loads(args.verified.read_text())
        if (verified.get('status') != 'passed' or not verified.get('verification')
                or verified['sources'] != inputs or verified['installed_sha256'] != binaries
                or verified['fixture_sha256'] != fixture_hashes or len(verified['rows']) != 96
                or verified['test_floor_sha256'] != proof['unboxed_source_matrices']['loop-measure-release.json']['floor_sha256']):
            parser.error('Id-driven correctness matrix is incomplete or stale')
        for counters in (False, True):
            if {(r['operations'], r['shape'], r['unboxed'], r['enabled']) for r in verified['rows']
                    if r['counters'] == counters} != cases:
                parser.error('Id-driven correctness coordinates are incomplete')
    result = dict(status='running', qualified=False, verification=args.verify, sources=inputs,
                  correctness_sha256=sha(args.receipt), installed_sha256=binaries, prefix=str(prefix),
                  fixture_sha256=fixture_hashes, batches=2 if args.verify else args.batches,
                  pairs=1 if args.verify else args.pairs, rows=[], gates=[],
                  scope='Identical verified numeric/loop factories, called by ordinary Id with Time and System gc. Verification compares test and normal release floors and checks exact reuse counts around the Id-driven phase; a subsequent native probe independently checks arithmetic-consumer admission. Production timing uses no test floor, test counter or probe. Timing includes Id driver/clock overhead; collection is ordinary System gc and retains the result through its post-GC check. These costs differ from the native probe and must not be compared as absolute durations. Both representations and all four input/control shapes are retained.')
    if args.verify:
        result['test_floor_sha256'] = sha(floor)
    else:
        result['verification_sha256'] = sha(args.verified)
    env = {k: v for k, v in os.environ.items() if not k.startswith(('BIRNAM_', 'DYLD_', 'BENCH_HOST_'))}
    env.update(BENCH_HOST_WAIT_SECONDS=str(args.wait_seconds), BENCH_HOST_RETRY_SECONDS='10')

    def save():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + '\n')

    def check():
        if any(sha(ROOT / n) != h for n, h in inputs.items()) or any(sha(prefix / n) != h for n, h in binaries.items()):
            raise RuntimeError('source or installation changed')
        if args.verify and sha(floor) != result['test_floor_sha256']:
            raise RuntimeError('test floor changed')
        if sha(args.receipt) != result['correctness_sha256']:
            raise RuntimeError('production correctness receipt changed')
        if not args.verify and sha(args.verified) != result['verification_sha256']:
            raise RuntimeError('Id-driven correctness receipt changed')

    def gate(arguments):
        p = subprocess.run(['bash', 'scripts/bench-host-check.sh', *map(str, arguments)], cwd=ROOT,
                           env=env, capture_output=True, text=True)
        result['gates'].append(dict(arguments=list(map(str, arguments)), exit_code=p.returncode, output=p.stdout + p.stderr))
        save()
        return p.returncode == 0

    try:
        check()
        with tempfile.TemporaryDirectory(prefix='birnam-production-loop-') as work:
            fixture, monitor_path = Path(work) / 'loop.birnam', Path(work) / 'interval.log'
            for n, s, u in product((3, 16, 32), range(4), (0, 1)):
                for pair in range(result['pairs']):
                    if not args.verify and not gate(['--wait', f'production-loop-{n}-{s}-{u}-{pair}']):
                        result['status'] = 'not_qualified'; return 2
                    monitor = None
                    with monitor_path.open('w') as log:
                        if not args.verify:
                            monitor = subprocess.Popen(['top', '-l', '0', '-s', '1', '-n', '0', '-stats', 'pid'], stdout=log, stderr=subprocess.DEVNULL)
                        try:
                            counter_rows = {}
                            for enabled in ((0, 1) if pair % 2 == 0 else (1, 0)):
                                for counters in ((True, False) if args.verify else (False,)):
                                    fixture.write_text(source(n, s, counters))
                                    extra = dict(BIRNAM_DEV_LOOP_INVARIANTS=str(enabled), BIRNAM_DEV_UNBOXED_NUMERIC=str(u), BIRNAM_LOOP_BATCHES=str(result['batches']))
                                    if counters:
                                        extra.update(BIRNAM_ID_DYLIB=str(floor), BIRNAM_TEST_UNBOXED_NUMERIC='1')
                                    print(n, s, u, pair, enabled, counters, flush=True)
                                    start = time.monotonic_ns()
                                    p = subprocess.run(['/usr/bin/time', '-l', str(prefix / 'bin/birnam'), str(fixture)], cwd=ROOT,
                                                       env=dict(env, **extra), capture_output=True, text=True, timeout=1800)
                                    row = dict(operations=n, shape=s, unboxed=u, pair=pair, enabled=enabled, counters=counters,
                                               exit_code=p.returncode, process_wall_ns=time.monotonic_ns() - start, stdout=p.stdout, stderr=p.stderr)
                                    result['rows'].append(row); save()
                                    if p.returncode:
                                        raise RuntimeError('Id-driven loop process failed')
                                    row.update(parsed(p.stdout, result['batches']), peak_rss_bytes=peak_rss(p.stderr))
                                    if counters:
                                        reused = re.findall(r'^production-reused (\d+)$', p.stdout, re.MULTILINE)
                                        if reused != [str([127 * n, 127, 0, 0][s] * result['batches'] * enabled)]:
                                            raise RuntimeError('Id-driven loop reuse count differs')
                                        counter_rows[enabled] = parse_output(p.stdout)
                                    save()
                            if args.verify:
                                check_pair(counter_rows[0], counter_rows[1], 1, u)
                        finally:
                            if monitor is not None:
                                monitor.terminate(); monitor.wait(timeout=10)
                    if not args.verify:
                        accepted = gate(['--samples', monitor_path, 'production-loop'])
                        result['gates'][-1]['samples'] = monitor_path.read_text()
                        if not accepted:
                            result['status'] = 'not_qualified'; return 2
        check()
        result['comparisons'] = []
        for n, s, u in product((3, 16, 32), range(4), (0, 1)):
            for metric in ('execution_us', 'including_gc_us', 'cold_execution_us', 'process_wall_ns', 'peak_rss_bytes'):
                changes = []
                for pair in range(result['pairs']):
                    rows = {r['enabled']: r for r in result['rows'] if (r['operations'], r['shape'], r['unboxed'], r['pair'], r['counters']) == (n, s, u, pair, False)}
                    if rows[0][metric] > 0:
                        changes.append(100 * (1 - rows[1][metric] / rows[0][metric]))
                if changes:
                    result['comparisons'].append(dict(operations=n, shape=s, unboxed=u, metric=metric,
                                                     paired_reduction_percent=changes, median_reduction_percent=statistics.median(changes)))
        result.update(status='passed', qualified=not args.verify)
        return 0
    except BaseException as error:
        result.update(status='failed', error=str(error)); raise
    finally:
        save()


if __name__ == '__main__':
    raise SystemExit(main())
