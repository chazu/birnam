#!/usr/bin/env python3
"""Explicit validation plans. --plan lists work without building or executing it."""
import argparse
import fcntl
import hashlib
import json
from pathlib import Path
import tempfile
from validation_support import ROOT, PlanRunner, audit_steps
from closure_plan import closure_plan

PRODUCTION = ('Cargo.toml', 'Cargo.lock', 'rust/**/*', 'src/id/**/*', 'modules/**/*',
              'vendor/libtcc-src/**/*', 'wrapped/**/*.dylib')
NATIVE = PRODUCTION + ('scripts/*lifecycle*', 'scripts/portable-image-v8-smoke.sh',
                      'scripts/stackless-control-smoke.sh', 'scripts/task-control-smoke.*',
                      'scripts/multicore-smoke.*',
                      'scripts/worker-service-smoke.*',
                      'scripts/closure-elision-smoke.sh',
                      'scripts/rust-driver-native-smoke.sh', 'scripts/fixtures/*', 'abi/**/*')


def plan(suite, prefix, output):
    def step(name, argv, inputs, **kwargs):
        return dict(name=name, argv=[str(a) for a in argv], inputs=inputs, **kwargs)
    result = [step('tool-tests', ['python3', 'scripts/validation-support-test.py'],
                   ('scripts/validation*.py', 'scripts/validate.py', 'scripts/closure_plan.py', 'scripts/rust-install.sh', 'scripts/*timing*.py')),
              step('workspace', ['cargo', 'test', '--workspace', '--locked', '--offline'],
                   PRODUCTION + ('abi/idapi-v15.json',), timeout=300)]
    for fixture in ('stackless-s2', 'portable-code-v1'):
        result.append(step(fixture, ['python3', f'abi/fixtures/{fixture}/verify.py'], (f'abi/fixtures/{fixture}/*',)))
    for script in ('release-changelog-test.py', 'v014-performance-acceptance-test.py', 'v015-performance-acceptance-test.py'):
        result.append(step(script[:-3], ['python3', 'scripts/' + script], ('scripts/' + script, 'scripts/' + script.replace('-test', ''))))
    result.append(step('archive-test', ['python3', 'scripts/measurement-archive-test.py'], ('scripts/measurement*.py',)))
    result.append(step('install', ['bash', 'scripts/rust-install.sh', 'install', '--root', ROOT,
                       '--prefix', prefix, '--profile', 'release', '--build-cache', ROOT / '.cargo-target/install'],
                       PRODUCTION + ('scripts/rust-install.sh', 'docs/guide/**/*'), artifacts=(str(prefix),), timeout=300))
    result.append(step('native', ['bash', 'scripts/rust-driver-native-smoke.sh', prefix], NATIVE,
                       depends=('install',), timeout=600))
    result.append(step('floor-discovery', ['python3', 'scripts/floor-discovery-test.py',
                       '--prefix', prefix, '--output', output/'floor-discovery.json'],
                       PRODUCTION + ('scripts/floor-discovery-test.py', 'scripts/fixtures/floor-lookup.c',
                                     'abi/fixtures/idapi-v15/idapi_v15.h'),
                       depends=('install',), artifacts=(str(output/'floor-discovery.json'),), timeout=300))
    result.append(step('relational-output', ['python3', 'scripts/relational-output-test.py',
                       '--prefix', prefix, '--output', output/'relational-output.json'],
                       PRODUCTION + ('scripts/relational-output-test.py',),
                       depends=('install',), artifacts=(str(output/'relational-output.json'),), timeout=300))
    result.append(step('host-bridge', ['python3', 'scripts/host-bridge-test.py',
                       '--prefix', prefix, '--output', output/'host-bridge.json'],
                       PRODUCTION + ('scripts/host-bridge-test.py',), depends=('install',),
                       artifacts=(str(output/'host-bridge.json'),), timeout=300))
    result.append(step('host-project', ['env', 'BIRNAM_BIN=' + str(prefix/'bin/birnam'),
                       'bash', 'scripts/rust-project-check-smoke.sh'],
                       PRODUCTION + ('scripts/rust-project-check-smoke.sh',),
                       depends=('install',), timeout=600))
    result.append(step('codegen-state', ['python3', 'scripts/codegen-state-test.py',
                       '--output', output/'codegen-state.json'],
                       PRODUCTION + ('scripts/codegen-state-test.py',),
                       artifacts=(str(output/'codegen-state.json'),), timeout=300))
    if suite == 'performance':
        result.append(step('performance', ['python3', 'scripts/performance-smoke.py', '--prefix', prefix,
                           '--output', output/'performance.json'], PRODUCTION +
                           ('scripts/performance-smoke.py', 'scripts/recovery-applications.py', 'scripts/benchmark_support.py',
                            'bench/native-tier-focus.birnam'), depends=('workspace', 'native'),
                           artifacts=(str(output/'performance.json'),), timeout=600))
        return result
    coverage = 'exhaustive' if suite in ('exhaustive', 'release') else 'routine'
    result.extend(closure_plan(prefix, output/'matrices', coverage))
    if suite in ('exhaustive', 'release'):
        result.append(step('recovery', ['python3', 'scripts/recovery-graphs.py', '--all-families', '--output', output / 'recovery.json'],
                           PRODUCTION + ('scripts/recovery-graphs.py', 'scripts/compatibility_authority.py',
                                         'scripts/compatibility-authority-test.py', 'scripts/fixtures/*', 'abi/**/*'),
                           artifacts=(str(output / 'recovery.json'),), timeout=600))
    if suite == 'release':
        result.append(step('dylib-audit', ['bash', 'scripts/rust-dylib-audit.sh'],
                           PRODUCTION + ('scripts/rust-dylib-audit.sh', 'abi/rust-id-dylib-v1.json'), timeout=300))
        result.append(step('cold-source', ['bash', 'scripts/rust-driver-cold-archive-smoke.sh'],
                           PRODUCTION + ('scripts/rust-install.sh', 'scripts/rust-driver-cold-archive-smoke.sh',
                                         'docs/guide/**/*', 'examples/fizzbuzz.birnam'), timeout=600))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--suite', choices=('routine', 'exhaustive', 'release', 'performance'), default='routine')
    parser.add_argument('--plan', action='store_true')
    parser.add_argument('--deadline', type=int, help='Total seconds; expiry is incomplete, never a pass')
    parser.add_argument('--no-resume', action='store_true')
    parser.add_argument('--only', nargs='+', help='Named steps plus their prerequisites')
    parser.add_argument('--output', type=Path, default=ROOT / '.cargo-target/validation')
    parser.add_argument('--prefix', type=Path)
    args = parser.parse_args()
    key = hashlib.sha256(str(ROOT).encode()).hexdigest()[:12]
    prefix = (args.prefix or Path(tempfile.gettempdir()) / ('birnam-validation-' + key) / 'prefix').resolve()
    steps = plan(args.suite, prefix, args.output.resolve())
    if args.only:
        names = {step['name'] for step in steps}
        if not set(args.only) <= names: parser.error('unknown step; inspect --plan')
        selected = set(args.only)
        for step in reversed(steps):
            if step['name'] in selected: selected.update(step.get('depends', ()))
        steps = [step for step in steps if step['name'] in selected]
    deadline = args.deadline if args.deadline is not None else (900 if args.suite == 'routine' else 1800)
    if deadline <= 0: parser.error('deadline must be positive')
    if args.plan:
        print(json.dumps(dict(suite=args.suite, deadline_seconds=deadline, steps=steps), indent=2))
        return
    args.output.mkdir(parents=True, exist_ok=True)
    (ROOT / '.cargo-target').mkdir(exist_ok=True)
    # One checkout owns the shared diagnostic targets and default published prefix.
    with (ROOT / '.cargo-target/validation.lock').open('w') as lock:
        try: fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError: parser.error('another validation plan is active in this checkout')
        runner = PlanRunner(args.output, deadline=deadline, resume=not args.no_resume)
        runner.receipt.update(suite=args.suite, selected_steps=[s['name'] for s in steps])
        try:
            for step in steps: runner.run(**step)
            audit_steps(runner.rows, [step['name'] for step in steps])
            runner.receipt['status'] = 'passed'
        except BaseException as error:
            runner.receipt.update(status='failed', error=str(error))
            raise
        finally: runner.save()
        print(f'validate: {args.suite}: {len(steps)} steps passed; {args.output / "run.json"}')


if __name__ == '__main__':
    main()
