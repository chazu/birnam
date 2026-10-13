"""Build shared Host fixtures once; execute immutable copies during parallel checks."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
from validation_support import ROOT, save_json, sha

# Each Cargo invocation has one explicit target selection; unrelated harnesses
# cannot silently replace a requested executable or turn it into zero tests.
TARGETS = {
    'host-lib': ('birnam-host', '--lib', None),
    'native-owners': ('birnam-host', '--test', 'native_owners'),
    'native-backend': ('birnam-host', '--test', 'native_backend'),
    'native-compilers': ('birnam-host', '--test', 'native_compilers'),
    'project-build': ('birnam-project-build', '--lib', None),
    'host-project': ('birnam-host-project', '--lib', None),
    'task-control': ('birnam-siphon', '--test', 'task_control'),
}


def prepare(directory):
    directory = directory.resolve()
    directory.mkdir(parents=True, exist_ok=True)
    entries = {}
    for key, (package, kind, target) in TARGETS.items():
        argv = ['cargo', 'test', '--offline', '--locked', '--no-run', '--message-format=json', '-p', package, kind]
        if target: argv.append(target)
        result = subprocess.run(argv, cwd=ROOT, stdout=subprocess.PIPE, text=True, check=True)
        wanted = target or package.replace('-', '_')
        executables = [row['executable'] for line in result.stdout.splitlines()
            if (row := json.loads(line)).get('reason') == 'compiler-artifact'
            and row.get('executable') and row.get('profile', {}).get('test')
            and row['target']['name'] == wanted]
        if len(executables) != 1:
            raise ValueError('expected exactly one executable for ' + key)
        # Copies, never hard links: a subsequent Cargo replacement cannot touch
        # an executable that a current_exe() child will enter during acceptance.
        staged = directory / (key + '.tmp')
        shutil.copy2(executables[0], staged)
        staged.replace(directory / key)
        entries[key] = dict(file=key, sha256=sha(directory/key))
    save_json(directory/'manifest.json', dict(schema=1, executables=entries))
    print('prepared immutable Host test executables:', directory, flush=True)


def executable(manifest, key):
    manifest = Path(manifest).resolve()
    data = json.loads(manifest.read_text())
    if data.get('schema') != 1 or set(data['executables']) != set(TARGETS):
        raise ValueError('incomplete Host test executable manifest')
    row = data['executables'][key]
    if row['file'] != key:
        raise ValueError('unexpected Host test executable path')
    path = manifest.parent / row['file']
    if path.is_symlink() or not os.access(path, os.X_OK) or sha(path) != row['sha256']:
        raise ValueError('Host test executable changed: ' + key)
    return path


def run(manifest, key, args):
    path = executable(manifest, key)
    listed = subprocess.run([path, *args, '--list'], text=True, capture_output=True, check=True, timeout=15)
    if not any(line.endswith(': test') for line in listed.stdout.splitlines()):
        raise ValueError('test selection matched no tests: ' + repr(args))
    return subprocess.run([path, *args]).returncode


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='mode', required=True)
    build = sub.add_parser('prepare'); build.add_argument('directory', type=Path)
    invoke = sub.add_parser('run')
    invoke.add_argument('manifest', type=Path); invoke.add_argument('key', choices=TARGETS)
    invoke.add_argument('args', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.mode == 'prepare': prepare(args.directory)
    else: sys.exit(run(args.manifest, args.key, args.args[1:] if args.args[:1] == ['--'] else args.args))


if __name__ == '__main__': main()
