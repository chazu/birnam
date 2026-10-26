#!/usr/bin/env python3
"""Mapped and explicit-rebuild acceptance, without source/compiler fallback."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import struct
import sys
import tempfile
import time

root=Path(__file__).resolve().parents[1]
build,stage=map(lambda p:Path(p).resolve(),sys.argv[1:])
logs=build/'image-qualification';logs.mkdir(exist_ok=True)
work=Path(tempfile.mkdtemp(prefix='roundtrip-',dir=logs))
shutil.copytree(stage,work/'lib');stage=work/'lib'
for name in ['host_entry','host_interaction','host_project','project_build']:
    shutil.copy2(build/'release'/f'libbirnam_{name}.dylib',stage)
subprocess.run(['clang','-dynamiclib','-O2',str(root/'tests/fixtures/oop-host/no-compiler.c'),'-o',str(stage/'no-compiler.dylib')],check=True)
source=work/'writer.birnam';snapshot=work/'program.image'
source.write_text('[Globals at: #ImageTestPath put: '+json.dumps(str(snapshot))+'];\n'+(root/'scripts/oop-image-smoke.birnam').read_text())
env={k:v for k,v in os.environ.items() if not k.startswith('BIRNAM_')};env.update(BIRNAM_LIB_DIR=str(stage),BIRNAM_MODULES_DIR='/nonexistent',BIRNAM_TEST_MODULE_DIRECTORY='/nonexistent')
receipt={'passed':False,'runs':[]}
def run(name,argv,expected,environment=env):
    start=time.monotonic()
    result=subprocess.run(list(map(str,argv)),env=environment,cwd=work,capture_output=True,text=True,timeout=900)
    receipt['runs'].append(dict(name=name,exit_code=result.returncode,stdout=result.stdout,stderr=result.stderr,elapsed_seconds=time.monotonic()-start))
    (work/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    assert result.returncode==0 and not result.stderr and result.stdout==expected,receipt['runs'][-1]
boundaries=work/'boundaries.birnam'
boundaries.write_text('[Globals at: #ImageTestNativeMethods put: [System loadModule: '+json.dumps(str(stage/'no-compiler.dylib'))+' arguments: #[]]];\n[System println: '+(root/'scripts/oop-image-boundaries.birnam').read_text()+']\n')
run('dispatch and Block restoration',[build/'release/birnam',boundaries],'#['+' '.join(['true']*18)+']\n',dict(env,BIRNAM_FORCE_SOURCE='1'))
portable=work/'portable.birnam'
portable.write_text('[System load: '+json.dumps(str(root/'modules/portable-code-test.birnam'))+']; [System println: [[Test runAllSuitesWithRunner: ConsoleRunner] allPassed]]\n')
# This existing suite includes materialization, reflection and live method removal.
result=subprocess.run([str(build/'release/birnam'),str(portable)],env=dict(env,BIRNAM_FORCE_SOURCE='1',BIRNAM_MODULES_DIR=str(root/'modules')),cwd=root,capture_output=True,text=True,timeout=300)
(work/'portable.log').write_text(result.stdout+result.stderr)
assert result.returncode==0 and not result.stderr and 'TOTAL: 107 passed, 0 failed' in result.stdout and result.stdout.endswith('true\n'),result
# Exercise the authored validator separately from native field decoding.
empty=(root/'tests/fixtures/portable-code-v1/image-empty-stripped.bin').read_bytes()
def record(kind,payload):
    return struct.pack('<IQ',kind,len(payload))+payload
def immediate(value):
    return b'\x13'+struct.pack('<IQ',8,value)
def heap(locator,fields=b'',count=0):
    return record(3,struct.pack('<I',1)+locator+struct.pack('<I',count)+fields)
valid=[empty+heap(immediate(n)) for n in [0,137,2**63-1]]
invalid=[empty+heap(immediate(2**63)),
         empty+heap(b'\x13'+struct.pack('<I',7)+bytes(7)),
         empty+heap(immediate(1),b'\x00',1),
         empty+heap(b'\x09'+struct.pack('<I',8)+b'ObjectVt',immediate(1),1),
         empty+record(11,struct.pack('<III',1,0,0)+b'\x00'+struct.pack('<I',8)+b'ObjectVt')+heap(immediate(1))]
forms=[]
for i,data in enumerate(valid+invalid):
    path=work/f'format-{i}.image';path.write_bytes(data)
    forms.append('let bytes := [[System readFile: '+json.dumps(str(path))+'] asBytes];')
    forms.append('[System println: [[[ImageFormat inspect: bytes architecture: 1 abi: [bytes readU64At: 20]] isError] = '+('false' if i<len(valid) else 'true')+']];')
format_probe=work/'format.birnam';format_probe.write_text('\n'.join(forms))
run('immediate locator admission',[build/'release/birnam',format_probe],'true\n'*len(forms[::2]),dict(env,BIRNAM_FORCE_SOURCE='1'))
run('save',[build/'release/birnam',source],'true\n#saved\n',dict(env,BIRNAM_FORCE_SOURCE='1'))
assert snapshot.is_file()
source.unlink()
output='#['+' '.join(['true']*9)+']\n'
run('mapped without parser or compiler',[build/'release/examples/oop_image',stage,snapshot],output+'OOP_IMAGE_MAPPED_OK\n')
run('explicit rebuild without parser',[build/'release/examples/oop_image',stage,snapshot,'rebuild'],output+'OOP_IMAGE_REBUILT_OK\n')
# The default CLI must also use a prepared bootstrap image, with no forced source.
shutil.copy2(snapshot,stage/'birnam-bootstrap.image')
hello=work/'hello.birnam';hello.write_text('[System println: [40 + 2]]\n')
run('default image-backed CLI',[build/'release/birnam',hello],'42\n')
receipt.update(passed=True,image_bytes=snapshot.stat().st_size,image_sha256=hashlib.sha256(snapshot.read_bytes()).hexdigest())
(work/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
print('OOP_IMAGE_OK '+str(work/'receipt.json'))
