#!/usr/bin/env python3
"""Verify one immutable archive through the current installed Id-message contract."""
import argparse
import json
import fcntl
from pathlib import Path
import subprocess
import sys
import tempfile
from install_contract import RECEIPT, verify
from validate import HOST_GROUPS, plan
from release_archive import extract_candidate
from validation_support import ROOT, PlanRunner, audit_plan, save_json, sha, environment_binding


def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT, text=True).strip()


def check_qualification(path, install, runner, binding_path=None):
    """Re-audit the full cold-candidate plan, not merely its reported status."""
    data = json.loads(path.read_text())
    if data.get('status') != 'passed' or data.get('candidate') is not True or data.get('suite') != 'release':
        raise ValueError('qualification must be a passed complete cold-candidate plan')
    prefix = Path(data['prefix'])
    if not prefix.is_absolute():
        raise ValueError('qualification prefix must be absolute')
    expected = plan('release', prefix, path.parent, candidate=True)
    if (data.get('selected_steps') != [s['name'] for s in expected] or
            set(data.get('steps', {})) != {s['name'] for s in expected}):
        raise ValueError('qualification coverage differs from the complete release plan')
    if data.get('source') != install['source_identity'] or verify(prefix) != install:
        raise ValueError('qualification and archive refer to different candidates')
    if data.get('toolchain') != runner.toolchain:
        raise ValueError('qualification toolchain differs from current validation context')
    for row in data['steps'].values():
        binding = row['binding']
        if binding.get('toolchain') != runner.toolchain or binding.get('environment') != environment_binding(runner.env):
            raise ValueError('qualification gate configuration differs from current validation context')
    audit_plan(data['steps'], expected)
    if binding_path is not None:
        # Per-phase receipts exclude the volatile 'reused' display flag. Bind
        # smoke reuse to those proofs, not bookkeeping in the aggregate report.
        save_json(binding_path, dict(source=data['source'], toolchain=data['toolchain'],
            install_receipt_sha256=sha(prefix / RECEIPT),
            required=[dict(name=s['name'], sha256=data['steps'][s['name']]['evidence_sha256'])
                      for s in expected]))
    return sha(path)


def accept(archive, output, *, allow_dirty=False, deadline=43200, qualification=None):
    archive = archive.resolve()
    output = output.resolve()
    commit = git('rev-parse', 'HEAD')
    clean = not git('status', '--porcelain=v1', '--untracked-files=all')
    if not clean and not allow_dirty:
        raise ValueError('release acceptance requires a clean tree; --allow-dirty records a non-promotable local check')
    output.mkdir(parents=True, exist_ok=True)
    receipt = dict(schema=3, scope='birnam-id-message-archive-acceptance', result='failed',
                   promotable=False, source=dict(git_commit=commit, git_clean=clean))
    try:
        before = sha(archive)
        with tempfile.TemporaryDirectory(prefix='birnam-release-acceptance-') as temporary:
            prefix = extract_candidate(archive, Path(temporary))
            install = verify(prefix)
            if install['source_identity']['git_commit'] != commit:
                raise ValueError('archive and acceptance checkout have different source commits')
            if install['profile'] != 'release' or install['build_mode'] != 'cold':
                raise ValueError('release archive must come from a cold release installation')
            runner = PlanRunner(output / 'gates', deadline=deadline, resume=True)
            if qualification is not None:
                qualification = qualification.resolve()
                binding_path = output / 'qualified-inputs.json'
                qualified_hash = check_qualification(qualification, install, runner, binding_path)
                steps = [dict(name='archive-smoke',
                    argv=[sys.executable, 'scripts/release_archive.py', str(archive)],
                    inputs=(str(archive), str(binding_path), 'scripts/release_archive.py',
                            'scripts/release-acceptance.py', 'scripts/install_contract.py',
                            'scripts/host-cli-test.py', 'rust/birnam-host/**/*'), timeout=2100)]
                receipt['qualification'] = dict(path=str(qualification), sha256=qualified_hash)
            else:
                # Standalone acceptance without prior qualification retains its
                # complete installed matrix. The release command supplies proof.
                steps = [dict(name='install-layout',
                    argv=[sys.executable, 'scripts/install_contract.py', str(prefix)],
                    inputs=(str(archive), 'scripts/install_contract.py'))]
                for step in plan('release', prefix, output):
                    if step['name'] in {'host-' + group for group in HOST_GROUPS}:
                        steps.append(dict(step, inputs=step['inputs'] + (str(archive),), depends=('install-layout',)))
            runner.receipt.update(scope=receipt['scope'], selected_steps=[s['name'] for s in steps])
            try:
                for step in steps:
                    runner.run(**step)
                audit_plan(runner.rows, steps)
                runner.receipt['status'] = 'passed'
            except BaseException as error:
                runner.receipt.update(status='failed', error=str(error))
                raise
            finally:
                runner.save()
            if qualification is not None and check_qualification(qualification, install, runner) != qualified_hash:
                raise ValueError('qualification changed during archive acceptance')
            if verify(prefix) != install or sha(archive) != before:
                raise ValueError('candidate archive or installation changed during acceptance')
            still_clean = not git('status', '--porcelain=v1', '--untracked-files=all')
            if git('rev-parse', 'HEAD') != commit or still_clean != clean:
                raise ValueError('checkout identity changed during acceptance')
            receipt.update(
                result='passed', promotable=clean and install['source_identity']['git_clean'],
                archive=dict(path=archive.name, sha256=before, bytes=archive.stat().st_size),
                install_receipt_sha256=sha(prefix / RECEIPT),
                source_manifest_sha256=install['source_manifest_sha256'],
                staged_manifest_sha256=install['staged_manifest_sha256'],
                admission_contract=install['admission_contract'],
                toolchain=install['toolchain'],
                gates_receipt='gates/run.json', gates_receipt_sha256=sha(output / 'gates/run.json'))
    except BaseException as error:
        receipt['error'] = str(error)
        raise
    finally:
        save_json(output / 'receipt.json', receipt)
    print('release acceptance: immutable Id-message archive passed; ' + str(output / 'receipt.json'))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('archive', type=Path)
    parser.add_argument('evidence', type=Path)
    parser.add_argument('--qualification', type=Path, help='complete validate --candidate run.json for these exact artifacts')
    parser.add_argument('--allow-dirty', action='store_true')
    parser.add_argument('--deadline', type=int, default=43200)
    args = parser.parse_args()
    if args.deadline <= 0:
        parser.error('deadline must be positive')
    # Shared Cargo products cannot be rebuilt by a simultaneous validation plan.
    (ROOT / '.cargo-target').mkdir(exist_ok=True)
    with (ROOT / '.cargo-target/validation.lock').open('w') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            parser.error('another validation or archive acceptance is active')
        accept(args.archive, args.evidence, allow_dirty=args.allow_dirty,
               deadline=args.deadline, qualification=args.qualification)


if __name__ == '__main__':
    main()
