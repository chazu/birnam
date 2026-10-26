#!/usr/bin/env python3
"""Exercise current Generation owners through the real source-mode CLI."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parents[1]
build, stage = map(lambda p: Path(p).resolve(), sys.argv[1:])
logs = build / 'generation-qualification'
logs.mkdir(exist_ok=True)
work = Path(tempfile.mkdtemp(prefix='lifecycle-', dir=logs))
for name in ['host_entry', 'host_interaction', 'host_project', 'project_build']:
    shutil.copy2(build / 'release' / f'libbirnam_{name}.dylib', stage)
source = work / 'lifecycle.birnam'
source.write_text('[Globals at: #GenerationTestPath put: '+json.dumps(str(work/'candidate.o'))+'];\n[System println: [[|\n'+(root/'scripts/oop-generation-smoke.birnam').read_text()+'\n] value]]\n')
env = {k:v for k,v in os.environ.items() if not k.startswith('BIRNAM_')}
env.update(BIRNAM_LIB_DIR=str(stage), BIRNAM_ID_DYLIB=str(stage/'libbirnam_id.dylib'), BIRNAM_FORCE_SOURCE='1', BIRNAM_MODULES_DIR='/nonexistent')
result = subprocess.run([str(build/'release/birnam'), str(source)], cwd=work, env=env, capture_output=True, text=True, timeout=240)
receipt = dict(exit_code=result.returncode, stdout=result.stdout, stderr=result.stderr, passed=False)
(work/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
assert result.returncode == 0 and not result.stderr, receipt
labels = ['vtableApi','vtablePrototype','implementation','capturedImplementation','portableBlock','cloneCode','canonicalCode','nativeBoundary','continuingWait','continuingReply','adopt','certificate','drift','restoredCertificate','compile','compileInert','inspect','map','mapInert','activate','newBody','capturedOldBody','rollback','restoredBody','repeatedRollback','removeFiles','discard','discardedCannotMap']
expected = '#['+' '.join('#[#'+label+' true]' for label in labels)+']\n'
assert result.stdout == expected, receipt
assert not (work/'candidate.o').exists() and not (work/'candidate.o.candidate').exists()
receipt['passed'] = True
(work/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
print('OOP_GENERATION_OK checks='+str(len(labels))+' '+str(work/'receipt.json'))
