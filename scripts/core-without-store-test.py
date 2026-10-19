#!/usr/bin/env python3
"""Exercise actual bootstrap/startup with optional native libraries unavailable.

This gate is deliberately red until the core composition is independent. It
copies the installed prefix, excludes Store and Relational, clears ambient module
paths and runs the real executable. No mock owners or exported method substitutes.
Cold builds, saved-image restoration and complete application acceptance remain
separate gates in the completion plan.
"""
import argparse
from pathlib import Path
import shutil
import tempfile
from validation_support import clean_environment, command, save_json, sha

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('prefix', type=Path)
args = parser.parse_args()
prefix = args.prefix.resolve()
work = Path(tempfile.mkdtemp(prefix='birnam-core-without-store-'))
floor = work / 'prefix'
excluded = {'libbirnam_store.dylib', 'libbirnam_relational.dylib'}
shutil.copytree(prefix, floor, ignore=lambda _directory, names: excluded.intersection(names))
assert all(not (floor / 'lib' / name).exists() for name in excluded)
source = work / 'probe.birnam'
source.write_text('''let coreAbsenceProbe := [Object clone];
[Reflect defineMethod: #answer on: coreAbsenceProbe as: [:self | 42]];
[System gc];
[System println: [coreAbsenceProbe answer]];
[System println: [Namespace includesKey: #Store]];
[System println: [Namespace includesKey: #db]];
[System println: [Namespace includesKey: #Relational]]
''')
receipt = {'passed': False, 'scope': 'Actual installed early bootstrap and full source startup with Store/Relational dylibs excluded. Does not prove cold builds or source-free image restoration.',
           'prefix': str(prefix), 'floor': str(floor), 'excluded': sorted(excluded),
           'runner_sha256': sha(Path(__file__).resolve()), 'runs': []}
for name, flags in [('early-bootstrap', {'BIRNAM_NO_STDLIB': '1'}),
                    ('source-startup', {'BIRNAM_FORCE_SOURCE': '1'})]:
    env = clean_environment()
    env.update(flags)
    run = command([floor / 'bin/birnam', source], cwd=work, env=env, timeout=300)
    passed = run['exit_code'] == 0 and run['stdout'] == '42\nfalse\nfalse\nfalse\n' and not run['stderr']
    receipt['runs'].append(dict(name=name, passed=passed, **run))
    (work / (name + '.log')).write_text(run['output'])
receipt['passed'] = all(run['passed'] for run in receipt['runs'])
save_json(work / 'results.json', receipt)
print('core-without-store: ' + ('passed' if receipt['passed'] else 'remaining dependency') + '; ' + str(work / 'results.json'))
raise SystemExit(0 if receipt['passed'] else 1)
