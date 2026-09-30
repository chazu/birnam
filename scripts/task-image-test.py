"""Installed task policy: reject handles, retain outcomes, resume in fresh Images."""
import json
import os
from pathlib import Path
import subprocess
import sys

prefix = Path(sys.argv[1]).resolve()
work = prefix.parent / 'task image checks'
work.mkdir(exist_ok=True)
binary = prefix / 'bin/birnam'
environment = {k: v for k, v in os.environ.items() if not k.startswith('BIRNAM_')}
environment['BIRNAM_GC_PRECISE'] = '1'
handle = work / 'handle.image'
outcome = work / 'outcome.image'
handle.write_bytes(b'preserve existing destination')
source = work / 'tasks.birnam'
source.write_text('''
[[|
[System println: [Module isLoaded: #stacklessControl]];
let finished := [TaskControl start: [:task |
  [task ensure: [| [task fail: #bodyFailed]] cleanup: [| [task fail: #cleanupFailed]]]]];
[Globals at: #SavedTaskOutcome put: [finished pollOutcome]];
[Globals at: #SavedTaskHandle put: #[finished]];
[System saveImage: HANDLE];
[System println: [[System imageSaveReport] status]];
let waiting := [TaskControl start: [:task | [task wait]]];
[Globals at: #SavedTaskHandle put: #[waiting]];
[System saveImage: HANDLE];
[System println: [[System imageSaveReport] status]];
[waiting cancel: #finished];
[Globals at: #SavedTaskHandle put: nil];
[Globals at: #main put: [|
  [System gc];
  [System println: SavedTaskOutcome];
  [System println: [[TaskControl start: [:task | 42]] pollOutcome]];
  [System println: [TaskExecutor current]]]];
[System saveImage: OUTCOME];
[System println: [[System imageSaveReport] status]]
] value]
'''.replace('HANDLE', json.dumps(str(handle))).replace('OUTCOME', json.dumps(str(outcome))))
result = subprocess.run([binary, source], cwd=work, env=environment,
                        capture_output=True, timeout=900)
assert result.returncode == 0 and not result.stderr, result
assert result.stdout == b'true\n#rejected\n#rejected\n#saved\n', result
assert handle.read_bytes() == b'preserve existing destination'
assert outcome.is_file()
source.unlink()
for options in ([], ['--rebuild']):
    result = subprocess.run([binary, *options, outcome], cwd=work, env=environment,
                            capture_output=True, timeout=900)
    assert result.returncode == 0 and not result.stderr, result
    assert result.stdout == b'#[#failure #[#bodyFailed #cleanupFailed] nil #[]]\n#[#success 42]\nnil\n', result
print('installed task Images: completed/waiting handles rejected; outcomes and fresh tasks survive mapped/rebuilt loading')
