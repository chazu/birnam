#!/usr/bin/env python3
"""Explicit validation plans. --plan lists work without building or executing it."""
import argparse
import fcntl
import hashlib
import json
from pathlib import Path
import tempfile
import subprocess
import signal
from concurrent.futures import ThreadPoolExecutor, as_completed, CancelledError
from install_contract import verify
from validation_support import ROOT, PlanRunner, audit_plan, save_json

PRODUCTION = ('Cargo.toml', 'Cargo.lock', 'rust/**/*', 'modules/**/*',
              'vendor/libtcc-src/**/*', 'wrapped/**/*.dylib')
HOST_GROUPS = ('cli', 'image', 'interaction', 'generation', 'tasks')
# Source and embedded fixtures, excluding generated Python bytecode caches.
SCRIPT_INPUTS = ('scripts/*', 'scripts/**/*.py', 'scripts/**/*.sh',
                 'scripts/**/*.birnam', 'scripts/fixtures/**/*')


def source_identity():
    def git(*args):
        return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()
    return dict(git_commit=git("rev-parse", "HEAD"),
                git_clean=not git("status", "--porcelain=v1", "--untracked-files=all"))


def plan(suite, prefix, output, *, candidate=False):
    if candidate and suite != "release":
        raise ValueError("candidate qualification requires the complete release suite")
    def step(name, argv, inputs, **kwargs):
        return dict(name=name, argv=[str(a) for a in argv], inputs=inputs, **kwargs)
    result = [step('tool-tests', ['python3', 'scripts/validation-support-test.py'],
                   ('scripts/validation*.py', 'scripts/validate.py', 'scripts/host-cli-test.sh',
                    'scripts/rust-install.sh', 'scripts/*timing*.py', 'scripts/release*.py', 'scripts/release.sh', 'scripts/dist.sh', 'scripts/host_test_binaries.py')),
              step('workspace', ['cargo', 'test', '--workspace', '--locked', '--offline'],
                   PRODUCTION, timeout=600)]
    for fixture in ('stackless-s2', 'portable-code-v1'):
        result.append(step(fixture, ['python3', f'tests/fixtures/{fixture}/verify.py'], (f'tests/fixtures/{fixture}/*',)))
    for script in ('release-changelog-test.py', 'release-acceptance-test.py', 'release-pipeline-test.py', 'v014-performance-acceptance-test.py', 'v015-performance-acceptance-test.py'):
        inputs = ('scripts/' + script, 'scripts/' + script.replace('-test', ''))
        if script.startswith('release-'):
            inputs += ('scripts/release*.py', 'scripts/release.sh', 'scripts/dist.sh', 'scripts/install_contract.py')
        result.append(step(script[:-3], ['python3', 'scripts/' + script], inputs))
    result.append(step('install-contract-test', ['python3', 'scripts/install-contract-test.py'],
                       ('scripts/install-contract-test.py', 'scripts/install_contract.py')))
    result.append(step('archive-test', ['python3', 'scripts/measurement-archive-test.py'], ('scripts/measurement*.py',)))
    result.append(step('native', ['bash', 'scripts/native-message-test.sh'],
                       # Collecting loader hooks and full image/resave lifetimes
                       # exceed 20 minutes before build and owner-image checks.
                       PRODUCTION + ('scripts/native-message-test.sh', 'tests/fixtures/**/*'), timeout=2700))
    result.append(step('install', ['bash', 'scripts/rust-install.sh', 'install', '--root', ROOT,
                       '--prefix', prefix, '--profile', 'release'] + ([] if candidate else
                       ['--build-cache', ROOT / '.cargo-target/install']),
                       PRODUCTION + ('VERSION', 'scripts/rust-install.sh', 'docs/guide/**/*') +
                       ((str(output / 'candidate-source.json'),) if candidate else ()),
                       artifacts=(str(prefix),), timeout=900))
    test_directory = output / 'artifacts/host-tests'
    result.append(step('host-test-binaries', ['python3', 'scripts/host_test_binaries.py',
                       'prepare', test_directory],
                       PRODUCTION + ('VERSION', 'tests/**/*') + SCRIPT_INPUTS,
                       artifacts=(str(test_directory),), timeout=600))
    # Immutable shared test executables and per-group runtime copies permit two
    # bounded lanes. Fixture-specific oracle builds use their own Cargo targets.
    for group in HOST_GROUPS:
        result.append(step('host-' + group,
                           ['bash', 'scripts/host-cli-test.sh', '--prefix', prefix, '--group', group,
                            '--prepared-binaries', test_directory / 'manifest.json',
                            '--timings', output / 'timings' / (group + '.json')],
                           PRODUCTION + ('tests/**/*', 'examples/**/*') + SCRIPT_INPUTS,
                           depends=('install', 'host-test-binaries'),
                           # Optional-provider and owner-image coverage exceeds
                           # 20 minutes before the final reentry/Host cases.
                           artifacts=(str(output / 'timings' / (group + '.json')),), timeout={'cli': 2400, 'image': 2400, 'interaction': 3600, 'generation': 2400, 'tasks': 2400}[group]))
    if suite == 'release':
        result.append(step('dylib-audit', ['bash', 'scripts/rust-dylib-audit.sh'],
                           PRODUCTION + ('scripts/rust-dylib-audit.sh', 'rust/birnam-id/exports.json'), timeout=300))
        result.append(step('cold-source', ['bash', 'scripts/rust-driver-cold-archive-smoke.sh'],
                           PRODUCTION + ('VERSION', 'scripts/rust-install.sh', 'scripts/rust-driver-cold-archive-smoke.sh',
                                         'docs/guide/**/*', 'examples/fizzbuzz.birnam'), timeout=1200))
    return result


# Static lanes are intentionally small and explicit. Balance uses recorded group
# costs; this is not an inferred dependency scheduler. Serial mode remains useful
# for comparison or machines with constrained resources.
HOST_LANES = (('host-interaction', 'host-cli'),
              ('host-tasks', 'host-image', 'host-generation'))


def run_plan(runner, steps, jobs=2):
    hosts = {s['name']: s for s in steps if s['name'].startswith('host-') and s['name'] != 'host-test-binaries'}
    if set(hosts) - {name for lane in HOST_LANES for name in lane}:
        raise ValueError('Host group is missing a lane assignment')
    launched = False
    for step in steps:
        if step['name'] not in hosts:
            runner.run(**step)
            continue
        if launched: continue
        launched = True
        if jobs == 1:
            for host in hosts.values(): runner.run(**host)
            continue
        # All prerequisites must already have completed; no compiler/build work
        # is scheduled by this executor while the Host groups are running.
        for host in hosts.values():
            if not set(host.get('depends', ())) <= runner.rows.keys():
                raise ValueError('Host lane prerequisite has not completed')
        def lane(names):
            try:
                for name in names:
                    if name in hosts: runner.run(**hosts[name])
            except BaseException:
                runner.cancel_event.set()
                raise
        with ThreadPoolExecutor(max_workers=2) as pool:
            futures = [pool.submit(lane, names) for names in HOST_LANES]
            try:
                for future in as_completed(futures):
                    try: future.result()
                    except CancelledError: continue
                if runner.cancel_event.is_set():
                    raise CancelledError('Host lanes cancelled')
            except BaseException:
                runner.cancel_event.set()
                raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--suite', choices=('routine', 'exhaustive', 'release'), default='routine')
    parser.add_argument('--jobs', type=int, choices=(1, 2), default=2, help='installed Host lanes (default: 2)')
    parser.add_argument('--plan', action='store_true')
    parser.add_argument('--deadline', type=int, help='Total seconds; expiry is incomplete, never a pass')
    parser.add_argument('--candidate', action='store_true', help='qualify one cold release candidate, without a second production build')
    parser.add_argument('--no-resume', action='store_true')
    parser.add_argument('--only', nargs='+', help='Named steps plus their prerequisites')
    parser.add_argument('--output', type=Path, default=ROOT / '.cargo-target/validation')
    parser.add_argument('--prefix', type=Path)
    args = parser.parse_args()
    if args.candidate and (args.suite != 'release' or args.only):
        parser.error('--candidate requires --suite release and forbids partial --only plans')
    identity = source_identity() if args.candidate else None
    key = hashlib.sha256(str(ROOT).encode()).hexdigest()[:12]
    default_prefix = Path(tempfile.gettempdir()) / ('birnam-validation-' + key)
    if args.candidate:
        default_prefix = default_prefix / identity['git_commit']
    prefix = (args.prefix or default_prefix / 'prefix').resolve()
    steps = plan(args.suite, prefix, args.output.resolve(), candidate=args.candidate)
    if args.only:
        names = {step['name'] for step in steps}
        if not set(args.only) <= names: parser.error('unknown step; inspect --plan')
        selected = set(args.only)
        for step in reversed(steps):
            if step['name'] in selected: selected.update(step.get('depends', ()))
        steps = [step for step in steps if step['name'] in selected]
    deadline = args.deadline if args.deadline is not None else 14400
    if deadline <= 0: parser.error('deadline must be positive')
    if args.plan:
        print(json.dumps(dict(suite=args.suite, deadline_seconds=deadline, jobs=args.jobs, host_lanes=HOST_LANES, steps=steps), indent=2))
        return
    args.output.mkdir(parents=True, exist_ok=True)
    (ROOT / '.cargo-target').mkdir(exist_ok=True)
    # One checkout owns the shared Cargo targets and default published prefix.
    with (ROOT / '.cargo-target/validation.lock').open('w') as lock:
        try: fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError: parser.error('another validation plan is active in this checkout')
        if args.candidate:
            save_json(args.output / 'candidate-source.json', identity)
        runner = PlanRunner(args.output, deadline=deadline, resume=not args.no_resume)
        runner.receipt.update(suite=args.suite, selected_steps=[s['name'] for s in steps], jobs=args.jobs, host_lanes=HOST_LANES)
        if args.candidate:
            runner.receipt.update(candidate=True, prefix=str(prefix), source=identity)
        def terminated(signum, frame):
            raise SystemExit(128 + signum)
        old_term = signal.signal(signal.SIGTERM, terminated)
        try:
            run_plan(runner, steps, args.jobs)
            audit_plan(runner.rows, steps)
            if args.candidate:
                install = verify(prefix)
                if install['profile'] != 'release' or install['build_mode'] != 'cold':
                    raise ValueError('candidate must be a cold release installation')
                if source_identity() != identity or install['source_identity'] != identity:
                    raise ValueError('candidate source identity changed during qualification')
            runner.receipt['status'] = 'passed'
        except BaseException as error:
            runner.receipt.update(status='failed', error=str(error))
            raise
        finally:
            signal.signal(signal.SIGTERM, old_term)
            runner.save()
        print(f'validate: {args.suite}: {len(steps)} steps passed; {args.output / "run.json"}')


if __name__ == '__main__':
    main()
