#!/usr/bin/env python3
"""Exercise the production Host with Store/Relational providers and entries absent."""
import argparse
import json
import subprocess
import sys
import time
from pathlib import Path
import shutil
import tempfile
from validation_support import ROOT, clean_environment, command, save_json, sha

parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('prefix',type=Path);parser.add_argument('--services',action='store_true',help='also exercise provider-absent project, worker and socket paths');args=parser.parse_args()
prefix=args.prefix.resolve();work=Path(tempfile.mkdtemp(prefix='birnam-core-without-store-'));floor=work/'prefix'
excluded={'libbirnam_store.dylib','libbirnam_relational.dylib'}
shutil.copytree(prefix,floor,ignore=lambda _directory,names:excluded.intersection(names))
assert not any(p.name in excluded for p in floor.rglob('*.dylib'))
receipt=dict(passed=False,prefix=str(prefix),floor=str(floor),excluded=sorted(excluded),runner_sha256=sha(Path(__file__).resolve()),runs=[],scope='Actual installed early/source/compiled startup, source-free mapped/rebuilt/resaved images, console and LSP with optional providers and entries physically excluded. Cold core preparation is independently checked by the installer.')
def run(name,argv,expected=None,flags=None,input=None,timeout=300):
 env=clean_environment();env.update(flags or {})
 if input is None:
  r=command(argv,cwd=work,env=env,timeout=timeout)
 else:
  started=time.monotonic_ns();result=subprocess.run(argv,cwd=work,env=env,input=input,text=True,capture_output=True,timeout=timeout)
  r=dict(exit_code=result.returncode,stdout=result.stdout,stderr=result.stderr,output=result.stdout+result.stderr,wall_ns=time.monotonic_ns()-started)
 (work/(name+'.log')).write_text(r['output']);passed=r['exit_code']==0 and (name in {'console','core-project'} or not r['stderr']) and (expected is None or r['stdout']==expected)
 receipt['runs'].append(dict(name=name,passed=passed,**r));save_json(work/'results.json',receipt)
 assert passed,(name,work,r['output'][-2000:]);return r
source=work/'probe.birnam';source.write_text('''let coreAbsenceProbe := [Object clone];
[Reflect defineMethod: #answer on: coreAbsenceProbe as: [:self | 42]];
[System gc]; [System println: [coreAbsenceProbe answer]];
[System println: [Namespace includesKey: #Store]];
[System println: [Namespace includesKey: #db]];
[System println: [Namespace includesKey: #Relational]]
''')
for name,flags in [('early',{'BIRNAM_NO_STDLIB':'1'}),('source',{'BIRNAM_FORCE_SOURCE':'1'}),('compiled',{})]:
 run(name,[floor/'bin/birnam',source],'42\nfalse\nfalse\nfalse\n',flags)
image=work/'core.image';resaved=work/'resaved.image';save=work/'save.birnam'
save.write_text('''[[|
let cell := [7 asCell]; let alias := cell;
[Globals at: #main put: [|
 [System gc]; [System println: [[cell value] + 1]];
 [System println: [cell == alias]];
 [System println: [Namespace includesKey: #Store]];
 [System println: [Namespace includesKey: #Relational]];
 [System saveImage: RESAVED]
]];
[System saveImage: IMAGE]; [System println: [[System imageSaveReport] status]]
] value]
'''.replace('RESAVED',json.dumps(str(resaved))).replace('IMAGE',json.dumps(str(image))))
run('save',[floor/'bin/birnam',save],'#saved\n');save.unlink()
for name,flags,path in [('mapped',[],image),('rebuilt',['--rebuild'],image),('resaved',[],resaved)]:
 run(name,[floor/'bin/birnam',*flags,path],'8\ntrue\nfalse\nfalse\n')
# An EOF console and a complete headless LSP exchange exercise ordinary services.
run('console',[floor/'bin/birnam','--repl'],input='')
messages=[dict(jsonrpc='2.0',id=1,method='initialize',params={}),dict(jsonrpc='2.0',id=2,method='shutdown',params={}),dict(jsonrpc='2.0',method='exit',params={})]
wire=''
for m in messages:
 body=json.dumps(m);wire+=f'Content-Length: {len(body.encode())}\r\n\r\n'+body
# command() uses text subprocess I/O; normalize framing after universal-newline decoding.
r=run('lsp',[floor/'bin/birnam','--lsp'],input=wire);text=r['stdout'].replace('\r\n','\n');responses=[]
while text:
 header,text=text.split('\n\n',1);length=int(header.split(':',1)[1]);responses.append(json.loads(text[:length]));text=text[length:]
assert [r['id'] for r in responses]==[1,2] and responses[1]['result'] is None,responses
if args.services:
 run('core-interaction',[sys.executable,ROOT/'scripts/interaction-cli-test.py',floor,'--core-only'],timeout=1200)
 run('core-workers',[sys.executable,ROOT/'scripts/worker-test.py',floor],timeout=1200)
 run('core-project',['bash',ROOT/'scripts/rust-project-check-smoke.sh'],flags={'BIRNAM_BIN':str(floor/'bin/birnam')},timeout=600)
receipt['services_checked']=args.services
receipt.update(passed=True,image_sha256=sha(image),resaved_sha256=sha(resaved));save_json(work/'results.json',receipt)
print('core-without-store: passed; '+str(work/'results.json'))
