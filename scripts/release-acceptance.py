#!/usr/bin/env python3
"""Verify one immutable archive through the current installed Id-message contract."""
import argparse
from pathlib import Path, PurePosixPath
import subprocess
import sys
import tarfile
import tempfile
from install_contract import RECEIPT, verify
from validate import HOST_GROUPS, plan
from validation_support import ROOT, PlanRunner, audit_plan, save_json, sha


def extract_candidate(archive, destination):
    """A release contains one ordinary prefix tree, with no links or overlays."""
    with tarfile.open(archive, 'r:gz') as package:
        names = set()
        roots = set()
        for member in package.getmembers():
            path = PurePosixPath(member.name)
            if (path.is_absolute() or '..' in path.parts or not path.parts or
                    path.as_posix() != member.name.rstrip('/') or
                    not (member.isfile() or member.isdir())):
                raise ValueError('invalid archive member: ' + member.name)
            name = path.as_posix()
            if name in names:
                raise ValueError('duplicate archive member: ' + name)
            names.add(name)
            roots.add(path.parts[0])
        if len(roots) != 1:
            raise ValueError('candidate must contain exactly one prefix directory')
        package.extractall(destination, filter='data')
    prefix = destination / roots.pop()
    if not prefix.is_dir():
        raise ValueError('candidate prefix is not a directory')
    return prefix


def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT, text=True).strip()


def accept(archive, output, *, allow_dirty=False, deadline=14400):
    archive = archive.resolve()
    output = output.resolve()
    commit = git('rev-parse', 'HEAD')
    clean = not git('status', '--porcelain=v1', '--untracked-files=all')
    if not clean and not allow_dirty:
        raise ValueError('release acceptance requires a clean tree; --allow-dirty records a non-promotable local check')
    output.mkdir(parents=True, exist_ok=False)
    receipt = dict(schema=2, scope='birnam-id-message-archive-acceptance', result='failed',
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
            # Use the same complete installed groups as local/CI validation;
            # do not rebuild or replace any artifact in the candidate archive.
            steps = [dict(name='install-layout',
                          argv=[sys.executable, 'scripts/install_contract.py', str(prefix)],
                          inputs=(str(archive), 'scripts/install_contract.py'))]
            installed = plan('release', prefix, output)
            for step in installed:
                if step['name'] in {'host-' + group for group in HOST_GROUPS}:
                    steps.append(dict(step, inputs=step['inputs'] + (str(archive),), depends=('install-layout',)))
            runner = PlanRunner(output / 'gates', deadline=deadline, resume=False)
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
    parser.add_argument('--allow-dirty', action='store_true')
    parser.add_argument('--deadline', type=int, default=14400)
    args = parser.parse_args()
    if args.deadline <= 0:
        parser.error('deadline must be positive')
    accept(args.archive, args.evidence, allow_dirty=args.allow_dirty, deadline=args.deadline)


if __name__ == '__main__':
    main()
