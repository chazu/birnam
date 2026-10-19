#!/usr/bin/env python3
"""Borrowed stdio, repeated sessions, cleanup aliases and console-owner images."""
import json
from pathlib import Path
import sys
import tempfile
from validation_support import clean_environment, save_json, sha
prefix=Path(sys.argv[1]).resolve(); work=Path(tempfile.mkdtemp(prefix='birnam-repl-owner-'))
env=clean_environment();env['BIRNAM_GC_PRECISE']='1'
image=work/'console.image';refused=work/'stream.image';refused.write_bytes(b'keep destination')
# Exercise the real source initializer separately from the compiled startup image.
setup=work/'setup.birnam'
setup.write_text('''[System println: [Repl stdioReady]];
[System load: REPL_SOURCE];
[[|
[System println: [[Repl stdioReady] not]];
let alias := [Repl clone];
[System println: [[alias stdioState] == [Repl stdioState]]];
let alien := Alien; let calls := [0 asCell]; let failing := [Object clone];
[Reflect defineMethod: #wrap:signature:as:on: on: failing as: [:self :name :signature :selector :target |
  [calls inc];
  [[[calls value] = 3] ifTrue: [| [Error kind: #forcedWrapperFailure]] ifFalse: [| self]]
]];
[Globals at: #Alien put: failing];
let failed := [alias consoleHostInitNoConfig];
[Globals at: #Alien put: alien];
[System println: [[failed kind] == #forcedWrapperFailure]];
[System println: [[calls value] = 3]];
[System println: [[alias stdioReady] not]];
[System println: [[alias consoleInput] isNil]];
// A later first session retries setup; success is visible through the shared owner.
[System println: [[[alias consoleHostInitNoConfig] isError] not]];
[System println: [Repl stdioReady]];
[alias closeConsoleResources];
let clears := [0 asCell];
[Reflect defineMethod: #consoleClearError on: ForeignAddress as: [:self | [clears inc]; nil]];
[alias consoleHostInitNoConfig]; [alias closeConsoleResources];
[System println: [[clears value] = 2]];
// Explicit installation remains available, even after readiness was true.
[System println: [[[alias wrapStdio] isError] not]];
[alias consoleHostInitNoConfig]; [alias closeConsoleResources];
[System println: [[clears value] = 2]]
] value]
'''.replace('REPL_SOURCE', json.dumps(str(prefix/'share/birnam/src/modules/repl.birnam'))))
source=work/'owner.birnam'
source.write_text('''[Module load: #repl];
[Alien wrap: "fileno" signature: "i p" as: #ownerStreamDescriptor on: ForeignAddress];
[[|
let repl := [Repl clone];
[System println: [[[repl consoleHostInitNoConfig] isError] not]];
let input := [repl consoleInput]; let error := [repl consoleError]; let buffer := [repl consoleBuffer];
let alias := [repl clone];
[System println: [[repl consoleOwner] == [alias consoleOwner]]];
[System println: [[input ownerStreamDescriptor] = 0]];
[System println: [[error ownerStreamDescriptor] = 2]];
[System println: [[repl readLineInto: ""] = "first\\n"]];
[repl closeConsoleResources]; [alias closeConsoleResources];
[System println: [[alias consoleInput] isNil]];
[System println: [buffer isNull]];
[System println: [[input ownerStreamDescriptor] = 0]];
[System println: [[error ownerStreamDescriptor] = 2]];
[repl consoleHostInitNoConfig];
// libc retains any unread bytes across session boundaries.
[System println: [[repl readLineInto: ""] = "second\\n"]];
[System println: [[repl readLineInto: ""] isNil]];
[repl closeConsoleResources]; [repl consoleHostInitNoConfig];
[System println: [[repl readLineInto: ""] isNil]];
[repl closeConsoleResources];
// Repeated initialization retires its old buffer.
[repl consoleHostInitNoConfig]; let before := [repl consoleBuffer];
[repl consoleHostInitNoConfig]; [System println: [before isNull]];
let actualBuffer := [repl consoleBuffer]; let count := [0 asCell];
let cleanup := [Object clone];
[Reflect defineMethod: #free on: cleanup as: [:self |
  [count inc]; [System gc]; [repl closeConsoleResources]; [actualBuffer free]
]];
[[repl consoleOwner] set: #[input error cleanup]];
[repl closeConsoleResources]; [System println: [[count value] = 1]];
// A cleanup callback can install a new session; it is not cleared afterward.
let replacement := [Alien alloc: 4096]; let replacing := [Object clone];
[Reflect defineMethod: #free on: replacing as: [:self |
  [repl installConsoleState: #[input error replacement]]; [System gc]; nil
]];
[repl installConsoleState: #[input error replacing]]; [repl closeConsoleResources];
[System println: [[repl consoleBuffer] == replacement]];
[repl closeConsoleResources]; [System println: [replacement isNull]];
// Failure retires old state and publishes no half-open replacement.
let broken := [Repl clone]; [broken consoleHostInitNoConfig]; let oldBuffer := [broken consoleBuffer];
[Reflect defineMethod: #borrowConsoleStreams on: broken as: [:self | [Error kind: #forcedStdioFailure]]];
[System println: [[[broken consoleHostInitNoConfig] kind] == #forcedStdioFailure]];
[System println: [oldBuffer isNull]];
[System println: [[broken consoleInput] isNil]];
[broken closeConsoleResources];
let library := [ForeignAddress consoleOpenProcess: 1];
[System println: [[[repl consoleSymbol: "__birnam_missing_stdio__" or: "__birnam_missing_stdio_fallback__" in: library] kind] == #consoleStdioUnavailable]];
[library consoleCloseLibrary];
// A current wrapper replacement and readiness both survive image recovery.
let clears := [0 asCell]; [Globals at: #SavedConsoleClears put: clears];
[Reflect defineMethod: #consoleClearError on: ForeignAddress as: [:self | [clears inc]; nil]];
// Save active session bookkeeping, but reject an independently retained stream.
[repl consoleHostInitNoConfig];
[Globals at: #SavedConsole put: repl]; [Globals at: #SavedConsoleAlias put: [repl clone]];
[Globals at: #SavedConsoleOwner put: [repl consoleOwner]];
[Globals at: #main put: [|
  [System gc];
  [System println: [[SavedConsole consoleInput] isNil]];
  [System println: [[SavedConsole consoleBuffer] isNil]];
  [System println: [[SavedConsole consoleOwner] == SavedConsoleOwner]];
  [System println: [[SavedConsoleAlias consoleOwner] == SavedConsoleOwner]];
  [System println: [[[SavedConsoleOwner value] at: 1] isNil]];
  [System println: [SavedConsole stdioReady]];
  let before := [SavedConsoleClears value];
  [System println: [[[SavedConsole consoleHostInitNoConfig] isError] not]];
  [System println: [[SavedConsoleClears value] = [before + 2]]];
  [SavedConsole closeConsoleResources]
]]
] value];
[System gc]; [System saveImage: IMAGE];
[System println: [[System imageSaveReport] status]];
[System println: [[SavedConsole consoleInput] notNil]];
[Globals at: #RetainedStream put: [SavedConsole consoleInput]];
[System saveImage: REFUSED]; [System println: [[System imageSaveReport] status]];
[SavedConsole closeConsoleResources]
'''.replace('IMAGE',json.dumps(str(image))).replace('REFUSED',json.dumps(str(refused))))
receipt={'passed':False,'prefix':str(prefix),'source':source.read_text(),'setup_source':setup.read_text(),'runner_sha256':sha(Path(__file__).resolve()),'runs':[]}
# subprocess input is needed for the two real fgets reads in separate sessions.
import subprocess,time
for name,args,expected in [('setup',[setup],'true\n'*12),('source',[source],'true\n'*20+'#saved\ntrue\n#rejected\n'),('mapped',[image],'true\n'*8),('rebuilt',['--rebuild',image],'true\n'*8)]:
 start=time.monotonic();r=subprocess.run([prefix/'bin/birnam',*args],cwd=work,env=env,input='first\nsecond\n' if name=='source' else '',capture_output=True,text=True,timeout=600)
 row=dict(name=name,exit_code=r.returncode,stdout=r.stdout,stderr=r.stderr,wall_seconds=time.monotonic()-start)
 receipt['runs'].append(row);save_json(work/'results.json',receipt);(work/(name+'.log')).write_text(r.stdout+r.stderr)
 assert r.returncode==0 and not r.stderr and r.stdout==expected,(work,row)
 if name=='source':source.unlink()
assert refused.read_bytes()==b'keep destination'
receipt['passed']=True;receipt['image_sha256']=sha(image);image.unlink();save_json(work/'results.json',receipt)
print('repl owner: passed; '+str(work/'results.json'))
