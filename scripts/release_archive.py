"""Bounded acceptance of packaged bytes: layout, signatures, relocation and startup."""
import argparse
from pathlib import Path, PurePosixPath
import subprocess
import sys
import tarfile
import tempfile
from install_contract import RECEIPT, verify
from validation_support import ROOT, clean_environment, command, sha


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



def verify_signatures(prefix, install):
    for relative in install['staged_manifest']:
        if not relative.startswith(('bin/', 'lib/')):
            continue
        path = prefix / relative
        with path.open('rb') as stream:
            magic = stream.read(8)
        if relative.startswith('lib/') and (magic.startswith(b'\x7fELF') or
                (path.suffix == '.image' and magic == b'BIRNAMIM')):
            continue
        subprocess.run(['codesign', '--verify', '--strict', path], check=True, timeout=30)


def smoke(archive):
    before = sha(archive)
    with tempfile.TemporaryDirectory(prefix='birnam-archive-smoke-') as temporary:
        prefix = extract_candidate(archive, Path(temporary))
        install = verify(prefix)
        receipt_hash = sha(prefix / RECEIPT)
        verify_signatures(prefix, install)
        # This bounded CLI fixture moves the extracted prefix and checks source,
        # diagnostics, GC and a saved image. It is not the five Host groups.
        env = clean_environment()
        env['CARGO_TARGET_DIR'] = str(ROOT / '.cargo-target')
        result = command([sys.executable, 'scripts/host-cli-test.py', str(prefix)], env=env, timeout=1800)
        print(result['output'], end='', flush=True)
        if result['exit_code']:
            raise RuntimeError('archive relocation/source/image check failed')
        relocated = Path(temporary) / 'relocated'
        env['BIRNAM_TEST_MODULE_DIRECTORY'] = str(relocated / 'lib')
        result = command(['cargo', 'test', '--offline', '--locked', '-p', 'birnam-host',
                          '--lib', 'startup::tests::installed_bootstrap_and_console_do_not_compile_or_parse',
                          '--', '--ignored', '--exact'], env=env, timeout=300)
        print(result['output'], end='', flush=True)
        if result['exit_code'] or '1 passed' not in result['output']:
            raise RuntimeError('archive compiled-startup check did not pass')
        if verify(relocated) != install or sha(relocated / RECEIPT) != receipt_hash or sha(archive) != before:
            raise ValueError('archive or extracted installation changed during smoke checks')
    print('archive smoke: signatures, exact layout, relocation, source/GC/image and compiled startup passed')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('archive', type=Path)
    smoke(parser.parse_args().archive.resolve())
