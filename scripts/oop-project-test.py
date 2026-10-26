#!/usr/bin/env python3
"""Run the retained project observation tests with declared Oop callbacks."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parents[1]
build, base = map(lambda p: Path(p).resolve(), sys.argv[1:])
logs = build / 'project-qualification'
logs.mkdir(exist_ok=True)
work = Path(tempfile.mkdtemp(prefix='native-', dir=logs))
stage = work / 'lib'
shutil.copytree(base, stage)
for name in ['host_entry', 'host_interaction', 'host_project', 'project_build']:
    shutil.copy2(build / 'release' / f'libbirnam_{name}.dylib', stage)
env = {k: v for k, v in os.environ.items() if not k.startswith('BIRNAM_')}
env.update(CARGO_TARGET_DIR=str(build), BIRNAM_TEST_MODULE_DIRECTORY=str(stage),
           BIRNAM_TEST_SOURCE_BOOTSTRAP='1', BIRNAM_MODULES_DIR='/nonexistent')
receipt = {'passed': False, 'runs': []}
for package, test in [
    ('birnam-project-build', 'project::operations::tests::project_policy_values_survive_collecting_messages'),
    ('birnam-host-project', 'project::source::tests::project_source_observation_uses_live_messages'),
    ('birnam-host-project', 'project_query_contract::project_queries_are_rooted_and_do_not_install_a_global_bridge'),
]:
    result = subprocess.run(['mise', 'exec', '--', 'cargo', 'test', '--offline', '--locked',
        '-p', package, '--lib', test, '--', '--exact', '--ignored', '--nocapture'],
        cwd=root, env=env, capture_output=True, text=True, timeout=300)
    receipt['runs'].append(dict(package=package, test=test, exit_code=result.returncode,
                               stdout=result.stdout, stderr=result.stderr))
    (work / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    assert result.returncode == 0 and '1 passed' in result.stdout, receipt['runs'][-1]
receipt['passed'] = True
(work / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
print('OOP_PROJECT_OBSERVATION_OK ' + str(work / 'receipt.json'))
