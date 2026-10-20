#!/usr/bin/env python3
"""Late real Store/Relational activation preserves an already-used core world."""
from pathlib import Path
import json
import sys
import tempfile
from validation_support import ROOT, clean_environment, command, save_json, sha
p=Path(sys.argv[1]).resolve();w=Path(tempfile.mkdtemp(prefix='birnam-optional-store-'));runs=[]
source=w/'late.birnam';source.write_text('''[[|
let facade := Globals; let cell := [17 asCell]; let alias := cell;
let probe := [Object clone]; [Reflect defineMethod: #answer on: probe as: [:self | 41]];
let lookup := [| Store]; let absent := [[lookup value] isNil];
let missing := [[Namespace includesKey: #Store] not];
[Module load: #store];
let store := Store; let fresh := [[lookup value] == store];
let basis := [store basis]; let facts := [store retainedFactCount];
[Reflect defineMethod: #answer on: probe as: [:self | 42]];
[cell set: 19]; [Globals at: #afterStoreProbe put: probe]; [Globals remove: #afterStoreProbe];
[System gc];
let independent := [[[store basis] = basis] & [[store retainedFactCount] = facts]];
let schema := [[store currentValue: #cardinality attr: #cardinality] == #one];
[store assert: #optValue attr: #history value: true];
let entity := [Object clone]; [store assert: entity attr: #optValue value: 17];
let earlier := [store basis]; [store assert: entity attr: #optValue value: 18];
let history := [[store valueOf: entity attr: #optValue asOf: earlier] = 17];
let handle := [store cellFor: entity attr: #optValue];
let increment := [[handle inc] = 19]; let decrement := [[handle dec] = 18];
[Module load: #relational];
let query := [[store findAll: ?e where: #[#[?e #optValue 18]]] = #[entity]];
let original := [System methodFor: #imageStore];
[Reflect defineMethod: #imageStore on: System as: [:self | nil]];
[Module load: #store]; [Module load: #relational];
let repeat := [[System imageStore] isNil];
[[Reflect vtableOf: System] addMethod: original as: #imageStore];
let stable := [[Globals == facade] & [[[cell == alias] & [[alias value] = 19]] & [[probe answer] = 42]]];
[System println: #[absent missing fresh independent schema history query repeat stable [Store == store] increment decrement]]
] value]
''')
for label,flags in [('compiled',{}),('source',{'BIRNAM_FORCE_SOURCE':'1'})]:
 env=clean_environment();env.update(flags);r=command([p/'bin/birnam',source],cwd=w,env=env,timeout=180);(w/(label+'.log')).write_text(r['output']);runs.append(dict(name=label,**r));save_json(w/'results.json',dict(passed=False,runs=runs));assert r['exit_code']==0 and not r['stderr'] and r['stdout']=='#[true true true true true true true true true true true true]\n',(label,w,r)
result=dict(passed=True,prefix=str(p),runs=runs,source=source.read_text(),source_sha256={str(f.relative_to(ROOT)):sha(f) for f in [Path(__file__).resolve(),ROOT/'modules/store.birnam',ROOT/'modules/relational.birnam']});save_json(w/'results.json',result);print('optional-store: passed; '+str(w/'results.json'))
