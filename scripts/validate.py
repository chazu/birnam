#!/usr/bin/env python3
"""Explicit validation plans. --plan lists work without building or executing it."""
import argparse
import fcntl
import hashlib
import json
from pathlib import Path
import tempfile
from validation_support import ROOT, PlanRunner, audit_plan

PRODUCTION = ('Cargo.toml', 'Cargo.lock', 'rust/**/*', 'src/id/**/*', 'modules/**/*',
              'vendor/libtcc-src/**/*', 'wrapped/**/*.dylib')
HOST_GROUPS = ('cli', 'image', 'interaction', 'generation', 'tasks')


def plan(suite, prefix, output):
    def step(name, argv, inputs, **kwargs):
        return dict(name=name, argv=[str(a) for a in argv], inputs=inputs, **kwargs)
    result = [step('tool-tests', ['python3', 'scripts/validation-support-test.py'],
                   ('scripts/validation*.py', 'scripts/validate.py', 'scripts/host-cli-test.sh',
                    'scripts/rust-install.sh', 'scripts/*timing*.py')),
              step('workspace', ['cargo', 'test', '--workspace', '--locked', '--offline'],
                   PRODUCTION, timeout=600)]
    for fixture in ('stackless-s2', 'portable-code-v1'):
        result.append(step(fixture, ['python3', f'abi/fixtures/{fixture}/verify.py'], (f'abi/fixtures/{fixture}/*',)))
    for script in ('release-changelog-test.py', 'v014-performance-acceptance-test.py', 'v015-performance-acceptance-test.py'):
        result.append(step(script[:-3], ['python3', 'scripts/' + script], ('scripts/' + script, 'scripts/' + script.replace('-test', ''))))
    result.append(step('archive-test', ['python3', 'scripts/measurement-archive-test.py'], ('scripts/measurement*.py',)))
    result.append(step('native', ['bash', 'scripts/native-message-test.sh'],
                       PRODUCTION + ('scripts/native-message-test.sh', 'abi/**/*'), timeout=900))
    result.append(step('install', ['bash', 'scripts/rust-install.sh', 'install', '--root', ROOT,
                       '--prefix', prefix, '--profile', 'release', '--build-cache', ROOT / '.cargo-target/install'],
                       PRODUCTION + ('VERSION', 'scripts/rust-install.sh', 'docs/guide/**/*'),
                       artifacts=(str(prefix),), timeout=900))
    # Keep all installed checks in every suite. CI shards the groups, while local
    # execution is serial so self-spawning Host tests never race a binary rebuild.
    for group in HOST_GROUPS:
        result.append(step('host-' + group,
                           ['bash', 'scripts/host-cli-test.sh', '--prefix', prefix, '--group', group],
                           PRODUCTION + ('scripts/*', 'abi/**/*', 'tests/**/*', 'examples/**/*'),
                           depends=('install',), timeout={'interaction': 3600, 'generation': 2400, 'tasks': 2400}.get(group, 1200)))
    if suite == 'release':
        result.append(step('dylib-audit', ['bash', 'scripts/rust-dylib-audit.sh'],
                           PRODUCTION + ('scripts/rust-dylib-audit.sh', 'abi/rust-id-dylib-v1.json'), timeout=300))
        result.append(step('cold-source', ['bash', 'scripts/rust-driver-cold-archive-smoke.sh'],
                           PRODUCTION + ('VERSION', 'scripts/rust-install.sh', 'scripts/rust-driver-cold-archive-smoke.sh',
                                         'docs/guide/**/*', 'examples/fizzbuzz.birnam'), timeout=1200))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--suite', choices=('routine', 'exhaustive', 'release'), default='routine')
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
    deadline = args.deadline if args.deadline is not None else 7200
    if deadline <= 0: parser.error('deadline must be positive')
    if args.plan:
        print(json.dumps(dict(suite=args.suite, deadline_seconds=deadline, steps=steps), indent=2))
        return
    args.output.mkdir(parents=True, exist_ok=True)
    (ROOT / '.cargo-target').mkdir(exist_ok=True)
    # One checkout owns the shared Cargo targets and default published prefix.
    with (ROOT / '.cargo-target/validation.lock').open('w') as lock:
        try: fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError: parser.error('another validation plan is active in this checkout')
        runner = PlanRunner(args.output, deadline=deadline, resume=not args.no_resume)
        runner.receipt.update(suite=args.suite, selected_steps=[s['name'] for s in steps])
        try:
            for step in steps: runner.run(**step)
            audit_plan(runner.rows, steps)
            runner.receipt['status'] = 'passed'
        except BaseException as error:
            runner.receipt.update(status='failed', error=str(error))
            raise
        finally: runner.save()
        print(f'validate: {args.suite}: {len(steps)} steps passed; {args.output / "run.json"}')


if __name__ == '__main__':
    main()
