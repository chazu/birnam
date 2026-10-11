"""Check authored Trace through source-free mapped and rebuilt images."""
import os,sys,tempfile,subprocess
from pathlib import Path
prefix=Path(sys.argv[1]).resolve()
env={k:v for k,v in os.environ.items() if not k.startswith('BIRNAM_')}
env['BIRNAM_NO_STDLIB']='1'
env['BIRNAM_GC_PRECISE']='1'
with tempfile.TemporaryDirectory(prefix='birnam-trace-image-') as scratch:
 p=Path(scratch); image=p/'trace.image'
 source=r'''
let proto := [ObjectModel newSlots: #[nil nil nil nil nil] parent: ObjectVt];
[Reflect defineMethod: #value on: proto as: [Execution block: [:self | self] withNativeSignature: "m"]];
[Reflect defineMethod: #nativeReceiver on: proto as: [:self | [ObjectModel slot: 0 in: self]]];
[Reflect defineMethod: #nativeSelector on: proto as: [:self | [ObjectModel slot: 1 in: self]]];
[Reflect defineMethod: #nativeArguments on: proto as: [:self | [ObjectModel slot: 2 in: self]]];
[Reflect defineMethod: #nativeSignature on: proto as: [:self | [ObjectModel slot: 3 in: self]]];
[Reflect defineMethod: #nativeCleanup on: proto as: [:self | [ObjectModel slot: 4 in: self]]];
let request := [:target :selector :args :signature |
  [ObjectModel newSlots: #[target selector args signature nil] parent: [Reflect vtableOf: proto]]
];
let zero := [Execution wordResultOf: 0 selector: #word];
let one := [Execution wordResultOf: 1 selector: #word];
let two := [Execution wordResultOf: 2 selector: #word];
let answer := [Execution wordResultOf: 73 selector: #word];
let x := [Object clone]; let y := [Object clone];
let visitor := [ObjectModel newSlots: #[[Vector new]] parent: ObjectVt];
[Reflect defineMethod: #reached:identity: on: visitor as: [Execution block: [:self :value :identity |
  [ObjectModel slot: 0 in: self put: [[ObjectModel slot: 0 in: self] add: true]]; nil
] withNativeSignature: "xOw"]];
[Reflect defineMethod: #notReached:identity: on: visitor as: [Execution block: [:self :value :identity |
  [ObjectModel slot: 0 in: self put: [[ObjectModel slot: 0 in: self] add: false]]; nil
] withNativeSignature: "xOw"]];
[Reflect defineMethod: #complete: on: visitor as: [Execution block: [:self :gaps | [System gc]; answer] withNativeSignature: "ww"]];
[Reflect defineMethod: #observe: on: Storage as: [Execution block: [:self :sink |
  [[request value: sink value: #allocation:bytes:identity: value: #[y zero two] value: "xOww"] value];
  [[request value: sink value: #allocation:bytes:identity: value: #[x zero one] value: "xOww"] value];
  [[request value: sink value: #root: value: #[x] value: "xO"] value];
  [request value: sink value: #complete: value: #[one] value: "vw"]
] withNativeSignature: "vO"]];
let originalTrace := Trace;
[Reflect defineMethod: #portProbe on: Trace as: [:self | #retained]];
let main := [|
  [Module load: #trace];
  [System println: [originalTrace == Trace]];
  [System println: [Trace portProbe]];
  let report := [request value: Trace value: #report:acceptingGaps: value: #[visitor one] value: "vOw"];
  [System println: [[Execution wordResultOf: report selector: #value] asInteger]];
  [System println: [ObjectModel slot: 0 in: visitor]];
];
[System println: [System saveImage: "IMAGE_PATH"]];
[System println: [[System imageSaveReport] detail]];
'''.replace('IMAGE_PATH',str(image))
 path=p/'save.birnam';path.write_text(source)
 def run(args):
  r=subprocess.run([str(prefix/'bin/birnam'),*args],cwd=p,env=env,capture_output=True,timeout=180)
  assert r.returncode==0 and not r.stderr,(r.returncode,r.stdout,r.stderr)
  return r.stdout
 check=p/'globals.birnam'
 check.write_text('[System println: [[Globals at: #message] isNil]]; [System println: [[Globals at: #request] isNil]]; [System println: [[Globals at: #row] isNil]];')
 assert run([str(check)])==b'true\ntrue\ntrue\n'
 assert run([str(path)])==b'true\n\n'
 assert image.is_file()
 path.unlink()
 for args in [[str(image)],['--rebuild',str(image)]]:
  assert run(args)==b'true\n#retained\n73\n#[true false]\n'
 print('Trace: source-free mapped and rebuilt image, native carriers and collecting completion passed')
