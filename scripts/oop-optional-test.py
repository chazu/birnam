#!/usr/bin/env python3
"""Explicit compiled Store/Relational activation and provider image recovery."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time

root = Path(__file__).resolve().parents[1]
build, base = map(lambda p: Path(p).resolve(), sys.argv[1:])
logs = build / 'optional-qualification'
logs.mkdir(exist_ok=True)
work = Path(tempfile.mkdtemp(prefix='optional-', dir=logs))
stage = work / 'lib'
shutil.copytree(base, stage)
(stage / 'native').mkdir()
for name in ['host_entry', 'host_interaction', 'host_project', 'project_build']:
    shutil.copy2(build / 'release' / f'libbirnam_{name}.dylib', stage)
for name in ['store', 'relational']:
    library = build / 'release' / f'libbirnam_{name}.dylib'
    expected = json.loads((root / 'rust' / f'birnam-{name}' / 'exports.json').read_text())['exported_symbols']
    actual = set(subprocess.check_output(['nm', '-gjU', str(library)], text=True).splitlines())
    assert actual == {'_' + name for name in expected}, (name, actual)
    shutil.copy2(library, stage / 'native')
for fixture, output in [('oop-store/probe.c', 'store-probe.dylib'), ('oop-host/no-compiler.c', 'no-compiler.dylib')]:
    subprocess.run(['clang', '-dynamiclib', '-O2', str(root / 'tests/fixtures' / fixture), '-o', str(stage / output)], check=True)
env = {k: v for k, v in os.environ.items() if not k.startswith('BIRNAM_')}
env.update(BIRNAM_LIB_DIR=str(stage), BIRNAM_MODULES_DIR='/nonexistent', BIRNAM_TEST_MODULE_DIRECTORY='/nonexistent')
source_env = dict(env, BIRNAM_FORCE_SOURCE='1')
receipt = {'passed': False, 'runs': []}

def run(name, args, expected=None, environment=source_env, cwd=work, contains=None):
    started = time.monotonic()
    result = subprocess.run(list(map(str, args)), env=environment, cwd=cwd, capture_output=True, text=True, timeout=900)
    receipt['runs'].append(dict(name=name, exit_code=result.returncode, stdout=result.stdout, stderr=result.stderr, elapsed_seconds=time.monotonic() - started))
    (work / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    assert result.returncode == 0 and not result.stderr, receipt['runs'][-1]
    if expected is not None:
        assert result.stdout == expected, receipt['runs'][-1]
    if contains is not None:
        assert contains in result.stdout, receipt['runs'][-1]

for name in ['store', 'relational']:
    run('compile ' + name, [build / 'release/birnam', '--compile-module', root / 'modules' / f'{name}.birnam', stage / f'libbirnam_{name}.dylib'], '')
for name in ['store', 'relational']:
    body = (root / 'scripts' / f'oop-{name}-smoke.birnam').read_text()
    checks = re.findall(r'\[check value: #(\w+) value:', body)
    source = work / f'{name}.birnam'
    source.write_text('[System println: [Namespace includesKey: #Store]];\n[Module load: #' + name + '];\n' +
        '[Globals at: #StoreTestProbe put: [System loadModule: ' + json.dumps(str(stage / 'store-probe.dylib')) + ' arguments: #[]]];\n[System println: ' + body + ']\n')
    expected = 'false\n#[' + ' '.join('#[#' + check + ' true]' for check in checks) + ']\n'
    run(name + ' behavior', [build / 'release/birnam', source], expected)
    receipt[name + '_checks'] = len(checks)
source = work / 'limit.birnam'
source.write_text('[Module load: #store]; [System println: ' + (root / 'scripts/oop-store-limit.birnam').read_text() + ']\n')
run('reserved rollback under resource exhaustion', [build / 'release/birnam', source], '#[true true true true true true true]\n', dict(source_env, BIRNAM_STORE_FACT_BYTES_LIMIT='4096'))
source = work / 'authored.birnam'
source.write_text('[System load: ' + json.dumps(str(root / 'modules/store-test.birnam')) + ']; [System println: [[Test runAllSuitesWithRunner: ConsoleRunner] allPassed]]\n')
run('retained authored Store suite', [build / 'release/birnam', source], cwd=root, contains='TOTAL: 10 passed, 0 failed\ntrue\n')
image = work / 'optional.image'
source = work / 'save.birnam'
source.write_text('[Globals at: #OptionalImagePath put: ' + json.dumps(str(image)) + '];\n' + (root / 'scripts/oop-optional-image.birnam').read_text())
run('save declared providers', [build / 'release/birnam', source], 'true\n#saved\n')
source.unlink()
for mode, extra in [('MAPPED', []), ('REBUILT', ['rebuild'])]:
    run(mode.lower() + ' declared providers', [build / 'release/examples/oop_image', stage, image, *extra],
        '#[true true true true]\nOOP_IMAGE_' + mode + '_OK\n', env)
receipt.update(passed=True, image_bytes=image.stat().st_size, image_sha256=hashlib.sha256(image.read_bytes()).hexdigest())
(work / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
print('OOP_OPTIONAL_OK ' + str(work / 'receipt.json'))
