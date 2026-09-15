#!/usr/bin/env python3
"""Verify and measure repeated closures through ordinary Id on a normal release floor."""
from validation_support import save_json
from validation_proof import load_proof
from measurement_archive import load_json, read_bytes
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
from closure_timing_common import source as counter_source
from unboxed_numeric_timing_common import peak_rss

ROOT = Path(__file__).resolve().parent.parent
USES = (1, 3, 4, 8, 16, 32)
SHAPES = (0, 1, 3)


def sha(path):
    return hashlib.sha256(read_bytes(Path(path))).hexdigest()


def source(uses, *, numeric=False):
    setup = counter_source(uses, numeric=numeric).split('let block :=', 1)[0]
    setup = setup.replace('[System preciseGc: true];\n', '')
    return setup + '''[System load: "modules/time.birnam"];
let portable := [[source codeObject] withoutSourceProvenance];
let materializeStart := [Time monotonicMicros];
let block := [Codegen materializeResumable: portable captures: #[]];
let materializeTime := [[Time monotonicMicros] - materializeStart];
[[[block codeObject] == portable] ifTrue: [| nil] ifFalse: [| [OS exit: 60]]];
let input := [73 asCell];
let answer := [nil asCell];
let invoke := [|
  let previous := [answer value];
  let returned := [block value: input];
  [[shape = 0] ifTrue: [|
    [[returned == input] ifTrue: [| nil] ifFalse: [| [OS exit: 61]]]
  ] ifFalse: [|
    [[shape = 1] ifTrue: [|
      [[returned == previous] ifTrue: [| [OS exit: 62]] ifFalse: [| nil]];
      [[[returned value] == input] ifTrue: [| nil] ifFalse: [| [OS exit: 63]]]
    ] ifFalse: [|
      [[returned = 73] ifTrue: [| nil] ifFalse: [| [OS exit: 64]]]
    ]]
  ]];
  [answer set: returned]
];
let coldStart := [Time monotonicMicros];
[128 timesRepeat: invoke];
let coldTime := [[Time monotonicMicros] - coldStart];
[System gc];
[128 timesRepeat: invoke];
[System gc];
[System println: ["production-materialize-us " , [materializeTime printString]]];
[System println: ["production-cold-us " , [coldTime printString]]];
[System println: "production-closure-begin"];
[batches timesRepeat: [|
  let start := [Time monotonicMicros];
  [128 timesRepeat: invoke];
  let elapsed := [[Time monotonicMicros] - start];
  let gcStart := [Time monotonicMicros];
  [System gc];
  let gcElapsed := [[Time monotonicMicros] - gcStart];
  [[shape = 0] ifTrue: [|
    [[[answer value] == input] ifTrue: [| nil] ifFalse: [| [OS exit: 65]]]
  ] ifFalse: [|
    [[shape = 1] ifTrue: [|
      [[[[answer value] value] == input] ifTrue: [| nil] ifFalse: [| [OS exit: 66]]]
    ] ifFalse: [|
      [[[answer value] = 73] ifTrue: [| nil] ifFalse: [| [OS exit: 67]]]
    ]]
  ]];
  [System println: ["production-sample-us " , [elapsed printString]]];
  [System println: ["production-gc-us " , [gcElapsed printString]]]
]];
[System println: "production-closure-end"];
[System println: "production-closure-ok"];
'''


def parsed(output, batches, uses, shape, mode, reports):
    result = {}
    for label, count in (('materialize', 1), ('cold', 1), ('sample', batches), ('gc', batches)):
        values = [int(v) for v in re.findall(r'^production-' + label + r'-us (\d+)$', output, re.M)]
        if len(values) != count:
            raise RuntimeError('missing or extra ' + label + ' samples')
        result[label + '_us'] = values
    lines = output.splitlines()
    for marker in ('production-closure-begin', 'production-closure-end', 'production-closure-ok'):
        if lines.count(marker) != 1:
            raise RuntimeError('missing or extra closure boundary ' + marker)
    if 'EvaluationRejected' in output or 'error(' in output:
        raise RuntimeError('closure evaluation failed')
    begin, end = lines.index('production-closure-begin'), lines.index('production-closure-end')
    diagnostics = [line for line in lines[begin + 1:end] if line.startswith('closure-elision: ')]
    expected = []
    if reports and mode != 'baseline' and shape != 3:
        if mode == 'forced':
            expected = ['closure-elision: forced constructor recovery']
        else:
            expected = ['closure-elision: hit materialized=' + str(shape == 1).lower()]
            if uses > 1:
                expected.append('closure-elision: completed calls=' + str(uses))
    if diagnostics != expected * (batches * 128):
        raise RuntimeError(f'closure diagnostics differ: uses={uses} shape={shape} mode={mode} reports={reports}: {diagnostics[:8]} ({len(diagnostics)} lines)')
    if not reports and any(line.startswith('closure-elision: ') for line in lines):
        raise RuntimeError('unexpected reports during timing')
    result.update(execution_us=sum(result['sample_us']), including_gc_us=sum(result['sample_us']) + sum(result['gc_us']),
                  cold_execution_us=result['cold_us'][0], materialization_us=result['materialize_us'][0],
                  verified_region_reports=len(diagnostics))
    return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--prefix', type=Path, required=True)
    p.add_argument('--receipt', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--verify', action='store_true')
    p.add_argument('--numeric-body', action='store_true')
    p.add_argument('--verified', type=Path)
    p.add_argument('--exploratory', action='store_true')
    p.add_argument('--batches', type=int, default=100)
    p.add_argument('--pairs', type=int, default=4)
    p.add_argument('--wait-seconds', type=int, default=120)
    args = p.parse_args()
    if not 1 <= args.batches <= 500 or not 1 <= args.pairs <= 8 or args.wait_seconds < 0:
        p.error('invalid measurement bounds')
    if not args.verify and not args.verified:
        p.error('timing requires --verified')
    proof = load_proof(args.receipt, measurement=('numeric-measure-release' if args.numeric_body else 'measure-release'))
    if proof.get('status') != 'passed' or (proof.get('production_verification') != 'passed' and not proof.get("phase_verified")) or not proof.get('commands') or any(c['exit_code'] for c in proof['commands']):
        p.error('required correctness phase must pass first')
    sources = dict(proof['sources'])
    if not sources or any(sha(ROOT / n) != h for n, h in sources.items()):
        p.error('production sources differ from correctness receipt')
    measurement = proof.get('closure_source_matrices', {}).get(('numeric-' if args.numeric_body else '') + 'measure-release.json', {})
    if measurement.get('status') != 'passed':
        p.error('release allocation and invocation counter matrix required')
    for uses in USES:
        digest = hashlib.sha256(counter_source(uses, numeric=args.numeric_body).encode()).hexdigest()
        rows = [r for r in measurement['rows'] if r['uses'] == uses and r['source_sha256'] == digest]
        if {(r['shape'], r['enabled']) for r in rows} != set(product(range(4), (0, 1))):
            p.error('factory bodies lack complete counter verification')
    for name in ('scripts/closure-production-timing.py', 'scripts/closure_timing_common.py', 'scripts/unboxed_numeric_timing_common.py', 'scripts/bench-host-check.sh'):
        sources[name] = sha(ROOT / name)
    prefix = args.prefix.resolve()
    manifest_path = prefix / 'share/birnam/rust-install.json'
    install = load_json(manifest_path)
    if install['profile'] != 'release' or install['publication']['state'] != 'published':
        p.error('normal published release installation required')
    installed = dict(install['staged_manifest'])
    installed['share/birnam/rust-install.json'] = sha(manifest_path)
    shipped = {n.removeprefix('share/birnam/src/'): h for n, h in installed.items() if n.startswith('share/birnam/src/')}
    if not shipped or any(sha(ROOT / n) != h for n, h in shipped.items()):
        p.error('installed sources differ from correctness checkpoint')
    sources.update(shipped)
    fixtures = {str(n): hashlib.sha256(source(n, numeric=args.numeric_body).encode()).hexdigest() for n in USES}
    if not args.verify:
        verified = load_json(args.verified)
        coordinates = {(r['uses'], r['shape'], r['mode'], r['reports']) for r in verified.get('rows', [])}
        expected = {(n, s, m, r) for n, s, m, r in product(USES, SHAPES, ('baseline', 'enabled', 'forced'), (False, True)) if r or m != 'forced'}
        if verified.get('status') != 'passed' or not verified.get('verification') or verified.get('sources') != sources or verified.get('installed_sha256') != installed or verified.get('fixture_sha256') != fixtures or len(verified.get('rows', [])) != 90 or coordinates != expected:
            p.error('normal-floor verification is incomplete or stale')
    result = dict(status='running', qualified=False, verification=args.verify, exploratory_requested=args.exploratory, numeric_body=args.numeric_body,
                  sources=sources, installed_sha256=installed, fixture_sha256=fixtures, correctness_sha256=sha(args.receipt),
                  verification_sha256=sha(args.verified) if args.verified else None, prefix=str(prefix),
                  batches=2 if args.verify else args.batches, pairs=1 if args.verify else args.pairs, rows=[], gates=[],
                  scope='Normal production release floor with unchanged verified factory bodies, materialized through ordinary Codegen. Id drives 128 factory calls per batch, with identity/result checks and ordinary collection retaining the last answer. Shapes are captured Cell return, escaping closure, and compiler-refused observing body. Existing native replacement controls remain in the prerequisite full counter matrix. Verification requires exact reports for baseline, enabled and forced constructor recovery, plus silent baseline/enabled runs. Timings have no reports, test floor or native probe; they include Id driver/check overhead. This is a fixed production-path workload, not general application coverage.')
    env = {k: v for k, v in os.environ.items() if not k.startswith(('BIRNAM_', 'DYLD_', 'BENCH_HOST_', 'NATIVE_TIER_'))}
    env.update(BENCH_HOST_WAIT_SECONDS=str(args.wait_seconds), BENCH_HOST_RETRY_SECONDS='10')

    def save():
        save_json(args.output, result)

    def check():
        if any(sha(ROOT / n) != h for n, h in sources.items()) or any(sha(prefix / n) != h for n, h in installed.items()) or sha(args.receipt) != result['correctness_sha256']:
            raise RuntimeError('source, installation or correctness receipt changed')
        if args.verified and sha(args.verified) != result['verification_sha256']:
            raise RuntimeError('driver verification changed')

    def gate(arguments):
        run = subprocess.run(['bash', 'scripts/bench-host-check.sh', *map(str, arguments)], cwd=ROOT, env=env, capture_output=True, text=True)
        result['gates'].append(dict(arguments=list(map(str, arguments)), exit_code=run.returncode, output=run.stdout + run.stderr))
        save()
        return run.returncode == 0

    try:
        check(); save()
        with tempfile.TemporaryDirectory(prefix='birnam-production-closure-') as work:
            fixture, log_path = Path(work) / 'closure.birnam', Path(work) / 'interval.log'
            for pair in range(result['pairs']):
                for n, s in product(USES, SHAPES):
                    check(); fixture.write_text(source(n, numeric=args.numeric_body))
                    if not args.verify and not args.exploratory and not gate(['--wait', 'production-closure']):
                        result['status'] = 'not_qualified'; return 2
                    monitor = None
                    with log_path.open('w') as log:
                        if not args.verify:
                            monitor = subprocess.Popen(['top', '-l', '0', '-s', '1', '-n', '0', '-stats', 'pid'], stdout=log, stderr=subprocess.DEVNULL)
                        try:
                            modes = ('baseline', 'enabled', 'forced') if args.verify else (('baseline', 'enabled') if pair % 2 == 0 else ('enabled', 'baseline'))
                            for reports in ((True, False) if args.verify else (False,)):
                                for mode in modes:
                                    if mode == 'forced' and not reports:
                                        continue
                                    print(f'production-closure: uses={n} shape={s} pair={pair} mode={mode} reports={reports}', flush=True)
                                    extra = dict(BIRNAM_CLOSURE_SHAPE=str(s), BIRNAM_CLOSURE_BATCHES=str(result['batches']), BIRNAM_DEV_CLOSURE_ELISION=str(int(mode != 'baseline')), BIRNAM_DEV_CLOSURE_REPORT=str(int(reports)))
                                    if mode == 'forced':
                                        extra['BIRNAM_DEV_CLOSURE_FORCE_AT'] = '0'
                                    start = time.monotonic_ns()
                                    run = subprocess.run(['/usr/bin/time', '-l', str(prefix / 'bin/birnam'), str(fixture)], cwd=ROOT, env=dict(env, **extra), stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=1800)
                                    row = dict(uses=n, shape=s, pair=pair, mode=mode, reports=reports, exit_code=run.returncode, output=run.stdout, process_wall_ns=time.monotonic_ns() - start)
                                    result['rows'].append(row); save()
                                    if run.returncode:
                                        raise RuntimeError('normal closure process failed')
                                    row.update(parsed(run.stdout, result['batches'], n, s, mode, reports), peak_rss_bytes=peak_rss(run.stdout)); save()
                        finally:
                            if monitor is not None:
                                monitor.terminate(); monitor.wait(timeout=10)
                    if not args.verify:
                        accepted = gate(['--samples', log_path, 'production-closure'])
                        result['gates'][-1]['samples'] = log_path.read_text()
                        if not accepted and not args.exploratory:
                            result['status'] = 'not_qualified'; return 2
        check()
        result['comparisons'] = []
        if not args.verify:
            for n, s, metric in product(USES, SHAPES, ('execution_us', 'including_gc_us', 'cold_execution_us', 'materialization_us', 'process_wall_ns', 'peak_rss_bytes')):
                changes = []
                for pair in range(result['pairs']):
                    rows = {r['mode']: r for r in result['rows'] if (r['uses'], r['shape'], r['pair']) == (n, s, pair)}
                    if rows['baseline'][metric] > 0:
                        changes.append(100 * (1 - rows['enabled'][metric] / rows['baseline'][metric]))
                if changes:
                    result['comparisons'].append(dict(uses=n, shape=s, metric=metric, paired_reduction_percent=changes, median_reduction_percent=statistics.median(changes)))
        result.update(status='passed', qualified=not args.verify and not args.exploratory)
        return 0
    except BaseException as error:
        result.update(status='failed', error=str(error)); raise
    finally:
        save()


if __name__ == '__main__':
    raise SystemExit(main())
