#!/usr/bin/env python3
"""Qualify Oop Host callers against a fresh published Bootstrap closure."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parents[1]
build, stage = map(lambda p: Path(p).resolve(), sys.argv[1:])
logs = build / 'host-qualification'
logs.mkdir(exist_ok=True)
work = Path(tempfile.mkdtemp(prefix='host-', dir=logs))
prefix = work / 'original'
(prefix / 'bin').mkdir(parents=True)
shutil.copytree(stage, prefix / 'lib')
shutil.copy2(build / 'release/birnam', prefix / 'bin/birnam')
for name in ['host_entry', 'host_interaction', 'host_project', 'project_build']:
    library = build / 'release' / f'libbirnam_{name}.dylib'
    expected = json.loads((root / 'rust' / ('birnam-' + name.replace('_', '-')) / 'exports.json').read_text())['exported_symbols']
    actual = set(subprocess.check_output(['nm', '-gjU', str(library)], text=True).splitlines())
    assert actual == {'_' + name for name in expected}, (name, actual)
    shutil.copy2(library, prefix / 'lib')
# Startup must use executable-relative libraries from the relocated prefix.
relocated = work / 'relocated'
prefix.rename(relocated)
binary = relocated / 'bin/birnam'
env = {k: v for k, v in os.environ.items() if not k.startswith('BIRNAM_')}
env.update(BIRNAM_MODULES_DIR='/nonexistent', BIRNAM_TEST_MODULE_DIRECTORY='/nonexistent')
source_env = dict(env, BIRNAM_FORCE_SOURCE='1')
receipt = {'passed': False, 'scope': 'Oop Host source startup and CLI calls; images and workers remain separate', 'runs': []}

def run(name, args, expected=None, contains=None, error=False, environment=source_env):
    result = subprocess.run([str(binary), *map(str, args)], cwd=work, env=environment, capture_output=True, text=True, timeout=180)
    receipt['runs'].append(dict(name=name, exit_code=result.returncode, stdout=result.stdout, stderr=result.stderr))
    (work / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    assert (result.returncode != 0) == error, (name, result.returncode, result.stdout, result.stderr)
    if expected is not None:
        assert result.stdout == expected, (name, result.stdout, result.stderr)
    if contains is not None:
        assert contains in result.stdout + result.stderr, (name, result.stdout, result.stderr)
    if not error:
        assert not result.stderr, (name, result.stderr)
    return result

hello = work / 'hello space-λ.birnam'
hello.write_text('[System println: [40 + 2]]\n')
run('source startup', [hello], expected='42\n')
run('explicit minimal startup', [hello], expected='42\n', environment=dict(env, BIRNAM_NO_STDLIB='1'))
run('default requires image', [hello], error=True, contains='birnam-bootstrap.image', environment=env)
run('missing source', [work / 'absent.birnam'], error=True, contains='absent.birnam')
bad = work / 'syntax.birnam'
bad.write_text('[[1 + ]')
run('source diagnostic', [bad], error=True, contains='syntax')
methods = work / 'methods.birnam'
methods.write_text('[System println: [[|\n' + (root / 'scripts/oop-method-invocation-smoke.birnam').read_text() + '\n] value]]\n')
run('method construction through Host', [methods], expected='#[' + ' '.join(['true'] * 19) + ']\n')
primitive = work / 'primitive-replacement.birnam'
primitive.write_text('[System println: ' + (root / 'scripts/oop-primitive-replacement-smoke.birnam').read_text() + ']\n')
run('source primitive replacements', [primitive], expected='#[true true true true]\n')
module = work / 'hostModule.birnam'
module.write_text('[Module named: #hostModule requires: #[]]; [System println: 73]; [Module markLoaded: #hostModule]\n')
artifact = work / 'libbirnam_hostModule.dylib'
run('compile without executing', ['--compile-module', module, artifact], expected='')
assert artifact.is_file()
module.unlink()
run('activate artifact without source', [artifact], expected='73\n')
project = work / 'host-projections.birnam'
hook = work / 'cleanup.dylib'
subprocess.run(['clang', '-dynamiclib', '-O2', str(root / 'tests/fixtures/oop-host/cleanup.c'), '-o', str(hook)], check=True)
project.write_text('''
[System println: [[HostProject pathForUri: "file:///tmp/a%20b"] = "/tmp/a b"]];
[System println: [[HostProject pathForUri: "https://example.test"] at: "ok"]];
[[|
let original := [Roots methodFor: #release:];
let hook := [System loadModule: HOOK arguments: #[Namespace]];
[[Vtable behaviorOf: Roots] addMethod: hook as: #release:];
let value := [HostProject pathForUri: "file:///tmp/collected%20result"];
[[Vtable behaviorOf: Roots] addMethod: original as: #release:];
[GC collectPrecisely];
[System println: [value = "/tmp/collected result"]]
] value]
'''.replace('HOOK', json.dumps(str(hook))))
run('project result survives collecting cleanup', [project], expected='true\nfalse\ntrue\n')
receipt['passed'] = True
receipt['binary_sha256'] = hashlib.sha256(binary.read_bytes()).hexdigest()
(work / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
print('OOP_HOST_CLI_OK ' + str(work / 'receipt.json'))
