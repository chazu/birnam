#!/usr/bin/env python3
"""Compare default and combined experimental recovery consumers in real applications."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
import tempfile
import time

from unboxed_numeric_timing_common import peak_rss

ROOT = Path(__file__).resolve().parent.parent
SCENARIOS = ('eval', 'collections', 'portable-collections', 'materialize-cold')
SWITCHES = ('BIRNAM_DEV_UNBOXED_NUMERIC', 'BIRNAM_DEV_LOOP_INVARIANTS',
            'BIRNAM_DEV_CLOSURE_ELISION', 'BIRNAM_DEV_LITERAL_CALLEES')
SWITCH_NAMES = dict(zip(('unboxed', 'loop', 'closure', 'literal'), SWITCHES))
DEFAULT_SWITCH_NAMES = tuple(SWITCH_NAMES)
SWITCH_NAMES.update({'discarded-clones': 'BIRNAM_DEV_DISCARDED_CLONES',
                     'virtual-fields': 'BIRNAM_DEV_VIRTUAL_FIELDS'})


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def parse_output(output, scenario):
    samples = {}
    for label, count in (('materialize-us', 1), ('warmup-us', 1), ('sample-us', 5)):
        values = [int(v) for v in re.findall(r'^' + label + r' (\d+)$', output, re.MULTILINE)]
        if len(values) != count:
            raise RuntimeError(f'{scenario}: expected {count} {label} values')
        samples[label] = values
    if output.splitlines().count('focus-ok ' + scenario) != 1:
        raise RuntimeError(f'{scenario}: missing final answer/collection check')
    if 'EvaluationRejected' in output or 'error(' in output:
        raise RuntimeError(f'{scenario}: evaluation reported an error')
    return samples


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path, required=True)
    parser.add_argument('--receipt', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--pairs', type=int, default=4)
    parser.add_argument('--wait-seconds', type=int, default=120)
    parser.add_argument('--check-only', action='store_true', help='one correctness pair per scenario, without timing qualification')
    parser.add_argument('--exploratory', action='store_true', help='skip quiet-host admission and retain unqualified timings')
    parser.add_argument('--switches', nargs='+', choices=tuple(SWITCH_NAMES), default=list(DEFAULT_SWITCH_NAMES),
                        help='selected consumers disabled versus enabled; unrelated consumers keep defaults')
    parser.add_argument('--scenarios', nargs='+', choices=SCENARIOS, default=list(SCENARIOS))
    args = parser.parse_args()
    if not 1 <= args.pairs <= 8 or args.wait_seconds < 0:
        parser.error('invalid pairs or wait interval')
    prefix = args.prefix.resolve()
    scenarios = tuple(dict.fromkeys(args.scenarios))
    switches = tuple(SWITCH_NAMES[name] for name in dict.fromkeys(args.switches))
    proof = json.loads(args.receipt.read_text())
    if proof.get('status') != 'passed' or proof.get('production_verification') != 'passed':
        parser.error('full production correctness must pass first')
    if not proof.get('commands') or any(c['exit_code'] for c in proof['commands']):
        parser.error('production receipt contains unsuccessful commands')
    if not proof.get('sources') or any(sha(ROOT / name) != digest for name, digest in proof['sources'].items()):
        parser.error('production sources differ from the correctness receipt')
    manifest_path = prefix / 'share/birnam/rust-install.json'
    installed = json.loads(manifest_path.read_text())
    if installed['profile'] != 'release' or installed['publication']['state'] != 'published':
        parser.error('a published release installation is required')
    fixture = ROOT / 'bench/native-tier-focus.birnam'
    inputs = dict(proof['sources'])
    for name in ('scripts/recovery-applications.py', 'scripts/unboxed_numeric_timing_common.py',
                 'scripts/bench-host-check.sh', 'bench/native-tier-focus.birnam'):
        inputs[name] = sha(ROOT / name)
    installed_hashes = dict(installed['staged_manifest'])
    installed_hashes['share/birnam/rust-install.json'] = sha(manifest_path)
    # Bind the normal installed floor and all other artifacts to the installer
    # manifest, and its shipped source tree to this verified production source.
    shipped = {name.removeprefix('share/birnam/src/'): digest
               for name, digest in installed_hashes.items() if name.startswith('share/birnam/src/')}
    if not shipped or any(not (ROOT / name).is_file() or sha(ROOT / name) != digest
                          for name, digest in shipped.items()):
        parser.error('installed source differs from this production checkpoint')
    inputs.update(shipped)
    result = dict(status='running', qualified=False, check_only=args.check_only,
                  exploratory_requested=args.exploratory,
                  pairs=1 if args.check_only else args.pairs, sources=inputs,
                  correctness_receipt=str(args.receipt.resolve()), correctness_sha256=sha(args.receipt),
                  prefix=str(prefix), installed_sha256=installed_hashes,
                  switches=list(switches), scenarios=list(scenarios), rows=[], gates=[],
                  scope='Normal release installation, without a test floor or probe. Selected consumers disabled versus enabled. Default-off switches remain absent in the baseline; selected default-on aggregate switches are explicitly zero. Unselected consumers retain their defaults; the default selection remains the four experimental consumers, with aggregate consumers enabled on both sides. Selected fixed application workloads, each with five batches and a final post-GC answer check. Alternating configuration order and rotating scenario order. Execution samples, startup-inclusive process wall time and Darwin peak RSS are retained. Cold materialization includes eviction, compilation and retirement. This is combined regression and process-resource evidence; it neither attributes gains to individual consumers nor proves their activation or arbitrary application coverage.')
    env = {k: v for k, v in os.environ.items()
           if not k.startswith(('BIRNAM_', 'DYLD_', 'BENCH_HOST_', 'NATIVE_TIER_'))}
    env.update(BENCH_HOST_WAIT_SECONDS=str(args.wait_seconds), BENCH_HOST_RETRY_SECONDS='10')

    def save():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + '\n')

    def check_inputs():
        if any(sha(ROOT / name) != digest for name, digest in inputs.items()):
            raise RuntimeError('source changed during application comparison')
        if any(sha(prefix / name) != digest for name, digest in installed_hashes.items()):
            raise RuntimeError('installed artifact changed during application comparison')
        if sha(args.receipt) != result['correctness_sha256']:
            raise RuntimeError('correctness receipt changed')

    def gate(arguments):
        p = subprocess.run(['bash', 'scripts/bench-host-check.sh', *map(str, arguments)],
                           cwd=ROOT, env=env, capture_output=True, text=True)
        result['gates'].append(dict(arguments=list(map(str, arguments)), exit_code=p.returncode,
                                    output=p.stdout + p.stderr))
        save()
        return p.returncode == 0

    try:
        check_inputs()
        save()
        with tempfile.TemporaryDirectory(prefix='birnam-recovery-applications-') as work:
            for pair in range(result['pairs']):
                cases = scenarios[pair % len(scenarios):] + scenarios[:pair % len(scenarios)]
                for scenario in cases:
                    check_inputs()
                    context = f'recovery-applications-{pair}-{scenario}'
                    if not args.check_only and not args.exploratory and not gate(['--wait', context]):
                        result['status'] = 'not_qualified'
                        return 2
                    log_path = Path(work) / 'interval.log'
                    monitor = None
                    with log_path.open('w') as log:
                        if not args.check_only:
                            monitor = subprocess.Popen(['top', '-l', '0', '-s', '1', '-n', '0', '-stats', 'pid'],
                                                       stdout=log, stderr=subprocess.DEVNULL)
                        try:
                            pair_rows = []
                            for enabled in ((False, True) if pair % 2 == 0 else (True, False)):
                                print(f'recovery-applications: pair={pair} scenario={scenario} enabled={enabled}', flush=True)
                                # Preserve the original absent-variable baseline
                                # for default-off experiments. Only default-on
                                # aggregate passes need an explicit zero.
                                extra = ({key: '1' for key in switches} if enabled else
                                         {key: '0' for key in switches if key not in SWITCHES})
                                start = time.monotonic_ns()
                                p = subprocess.run(['/usr/bin/time', '-l', str(prefix / 'bin/birnam'), str(fixture)],
                                                   cwd=ROOT, env=dict(env, NATIVE_TIER_FOCUS=scenario, **extra),
                                                   capture_output=True, text=True, timeout=1800)
                                row = dict(pair=pair, scenario=scenario, enabled=enabled, switch_values=extra, exit_code=p.returncode,
                                           process_wall_ns=time.monotonic_ns() - start, stdout=p.stdout, stderr=p.stderr)
                                result['rows'].append(row)
                                save()
                                if p.returncode:
                                    raise RuntimeError('application process failed')
                                row['samples'] = parse_output(p.stdout, scenario)
                                row['peak_rss_bytes'] = peak_rss(p.stderr)
                                row['execution_us'] = statistics.median(row['samples']['sample-us'])
                                row['materialization_us'] = row['samples']['materialize-us'][0]
                                row['warmup_us'] = row['samples']['warmup-us'][0]
                                pair_rows.append(row)
                                save()
                            # All stdout apart from measured durations must agree.
                            normalized = [re.sub(r'^(?:sample|materialize|warmup)-us \d+$', '', r['stdout'],
                                                 flags=re.MULTILINE)
                                          for r in pair_rows]
                            if normalized[0] != normalized[1]:
                                raise RuntimeError('application output differs between configurations')
                        finally:
                            if monitor is not None:
                                monitor.terminate()
                                monitor.wait(timeout=10)
                    if not args.check_only:
                        accepted = gate(['--samples', log_path, context])
                        result['gates'][-1]['samples'] = log_path.read_text()
                        if not accepted and not args.exploratory:
                            result['status'] = 'not_qualified'
                            return 2
        check_inputs()
        result['comparisons'] = []
        for scenario in scenarios:
            for metric in ('execution_us', 'materialization_us', 'warmup_us', 'process_wall_ns', 'peak_rss_bytes'):
                changes = []
                for pair in range(result['pairs']):
                    rows = {r['enabled']: r for r in result['rows'] if r['scenario'] == scenario and r['pair'] == pair}
                    if rows[False][metric] > 0:
                        changes.append(100 * (1 - rows[True][metric] / rows[False][metric]))
                if changes:
                    result['comparisons'].append(dict(scenario=scenario, metric=metric, paired_reduction_percent=changes,
                                                     median_reduction_percent=statistics.median(changes)))
        result['qualified'] = not args.check_only and not args.exploratory
        result['status'] = 'passed'
        return 0
    except BaseException as error:
        result.update(status='failed', error=str(error))
        raise
    finally:
        save()


if __name__ == '__main__':
    raise SystemExit(main())
