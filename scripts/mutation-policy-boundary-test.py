#!/usr/bin/env python3
"""Check ordinary Reflect and direct editing in a running image without an adopted generation."""
import argparse,json,tempfile
from pathlib import Path
from validation_support import clean_environment,command,save_json,sha
p=argparse.ArgumentParser(description=__doc__);p.add_argument('prefix',type=Path);a=p.parse_args();prefix=a.prefix.resolve()
work=Path(tempfile.mkdtemp(prefix='birnam-mutation-policy-'));image=work/'policy.image';writer=work/'writer.birnam'
writer.write_text('''
[Globals at: #main put: [|
  [System println: [[DispatchGeneration current] isNil]];
  [System println: [[Reflect defineMethod: #policyProbe on: Integer as: [:self | 99]] == Integer]];
  [System println: [3 squared]];
  let old := [3 methodFor: #squared];
  let changed := [Reflect methodForArity: 0 implementation: [:self | 777]];
  [[Reflect vtableOf: 3] addMethod: changed as: #squared];
  [System gc];
  [System println: [3 squared]];
  [System println: [old invokeOn: 3 arguments: #[]]];
  [System println: [[Generation validateImageGeneration] == true]]
]];
[System saveImage: IMAGE]; [System println: [[System imageSaveReport] status]]
'''.replace('IMAGE',json.dumps(str(image))))
env=clean_environment();env['BIRNAM_GC_PRECISE']='1'
saved=command([prefix/'bin/birnam',writer],env=env,timeout=240)
assert saved['exit_code']==0 and not saved['stderr'] and saved['stdout']=='#saved\n',saved
writer.unlink()
run=command([prefix/'bin/birnam',image],env=env,timeout=120)
expected='true\ntrue\n9\n777\n9\ntrue\n'
receipt=dict(passed=run['exit_code']==0 and not run['stderr'] and run['stdout']==expected,prefix=str(prefix),saved=saved,run=run,expected=expected,image_sha256=sha(image),script_sha256=sha(Path(__file__).resolve()),scope='Ordinary running Reflect and existing vtable mutation both succeed; captured Method survives; no managed generation was adopted, so its explicit validator still succeeds.')
save_json(work/'results.json',receipt);print(run['output']);print(work/'results.json');assert receipt['passed'],receipt
