#!/usr/bin/env python3
"""Owned records: behavior, external facts, exact reclamation and source-free images."""
import argparse
import json
from pathlib import Path
import tempfile
from validation_support import ROOT, clean_environment, command, save_json, sha


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix',type=Path)
    parser.add_argument('--skip-images',action='store_true')
    args=parser.parse_args();prefix=args.prefix.resolve()
    work=Path(tempfile.mkdtemp(prefix='birnam-ordinary-records-'))
    env=clean_environment();env['BIRNAM_GC_PRECISE']='1'
    binary=prefix/'bin/birnam'
    files=[Path(__file__).resolve(),ROOT/'tests/fixtures/ordinary-records.birnam',ROOT/'rust/birnam-siphon/tests/ordinary_record_retention.rs']+[ROOT/'modules'/f'{m}.birnam' for m in ['log','config','http','process','app-config']]
    receipt=dict(passed=False,prefix=str(prefix),runs=[],source_sha256={str(f.relative_to(ROOT)):sha(f) for f in files})
    def run(name,argv,expected=None,*,cwd=work,native=False,timeout=300):
        selected=dict(env)
        if native:selected.update(BIRNAM_TEST_MODULE_DIRECTORY=str(prefix/'lib'),CARGO_TARGET_DIR=str(ROOT/'.cargo-target/native-message-tests'),CARGO_PROFILE_DEV_PANIC='unwind')
        result=command(argv,cwd=cwd,env=selected,timeout=timeout)
        receipt['runs'].append(dict(name=name,**result));save_json(work/'results.json',receipt)
        (work/(name+'.log')).write_text(result['output'])
        assert result['exit_code']==0,(name,work,result)
        if not native: assert not result['stderr'],(name,work,result)
        if expected is not None:assert result['stdout']==expected,(name,work,result)
        return result
    run('semantics',[binary,ROOT/'tests/fixtures/ordinary-records.birnam'],'ordinary-records: passed\n',cwd=ROOT)
    io=work/'io.birnam'
    io.write_text('''[Module load: #log]; [Module load: #process];
[Reflect defineMethod: #nowMillis on: Time as: [:self | 123]];
let logger := [Log named: "io"]; [logger level: #warn]; [logger toFile: "record.log"];
[logger info: "filtered"]; [logger warn: "emitted"];
[System println: [[System readFile: "record.log"] = "123 WARN io: emitted\\n"]];
[logger toStdout]; [logger error: "stdout"];
let result := [Process run: "printf 'record-output'; exit 7"];
[System println: [[result stdout] = "record-output"]];
[System println: [[result exitCode] = 7]];
[System println: [[result ok] = false]];
[System println: [[[Reflect factsOf: result] size] = 0]]
''')
    run('io',[binary,io],'true\n123 ERROR io: stdout\ntrue\ntrue\ntrue\ntrue\n')
    project=work/'project';child=project/'child';child.mkdir(parents=True)
    probe=work/'config.birnam'
    probe.write_text('''[Module load: #config]; [Config refresh];
[System println: [[Config path] = "../birnam.toml"]];
[System println: [[Config main] = "../app.birnam"]];
[System println: [[[Config loads] at: 0] = "demo"]]
''')
    config=project/'birnam.toml';config.write_text('main = "app.birnam"\nload = "demo"\n')
    run('config-parent',[binary,probe],'true\ntrue\ntrue\n',cwd=child)
    config.write_text('main = "unterminated\n')
    probe.write_text('''[Module load: #config]; [Config refresh];
[System println: [[Config path] = "../birnam.toml"]];
[System println: [[Config map] isNil]]; [System println: [[Config main] isNil]];
[System println: [[[Config loads] size] = 0]]
''')
    run('config-invalid',[binary,probe],'true\ntrue\ntrue\ntrue\n',cwd=child)
    config.unlink()
    probe.write_text('[Module load: #config]; [Config refresh]; [System println: [[Config path] isNil]]; [System println: [[Config map] isNil]]')
    run('config-absent',[binary,probe],'true\ntrue\n',cwd=child)
    run('retention',['mise','exec','--','cargo','test','--offline','--locked','-p','birnam-siphon','--test','ordinary_record_retention','--','--include-ignored','--nocapture'],cwd=ROOT,native=True)
    if not args.skip_images:
        source=work/'save.birnam';image=work/'records.image'
        source.write_text('''[Module load: #log]; [Module load: #config]; [Module load: #http]; [Module load: #process]; [System load: APP_CONFIG];
[[|
let logger := [Log named: "saved"]; let alias := logger; let clone := [logger clone]; [logger level: #warn];
let config := [Config clone]; [Reflect slot: #configPath in: config put: "../../birnam.toml"];
let request := [Http requestFrom: "PATCH /saved?q=1 HTTP/1.1\\r\\n\\r\\n"];
let result := [Process run: "printf 'captured'; exit 7"];
let app := [AppConfig resolve: "saved" defaults: {"port" -> 1} configPath: "/absent/saved.toml" environment: [Map new] args: #["--port=9"]];
[db assert: logger attr: #name value: #explicit];
[Globals at: #main put: [|
  [System gc];
  [System println: [logger == alias]];
  [System println: [[clone name] == [logger name]]];
  [Reflect slot: #name in: alias put: "edited"]; [System gc];
  [System println: [[logger name] = "edited"]];
  [System println: [[clone name] = "saved"]];
  [System println: [[logger thresholdRank] = 2]];
  [System println: [[config path] = "../../birnam.toml"]];
  [System println: [[request method] = "PATCH"]];
  [System println: [[request path] = "/saved"]];
  [System println: [[result stdout] = "captured"]];
  [System println: [[result exitCode] = 7]];
  [System println: [[app at: "port"] = 9]];
  [System println: [[app originAt: "port"] = "cli"]];
  [System println: [[db currentValue: logger attr: #name] == #explicit]];
  let before := [db retainedFactCount]; let fresh := [Log named: "fresh"];
  [System println: [[[Reflect slotsOf: fresh] size] = 3]];
  [System println: [[db retainedFactCount] = before]]
]];
[System saveImage: IMAGE]; [System println: [[System imageSaveReport] status]]
] value]
'''.replace('IMAGE',json.dumps(str(image))).replace('APP_CONFIG',json.dumps(str(prefix/'share/birnam/src/modules/app-config.birnam'))))
        run('save',[binary,source],'#saved\n');receipt['image_sha256']=sha(image);source.unlink()
        for mode,flags in [('mapped',[]),('rebuilt',['--rebuild'])]:run(mode,[binary,*flags,image],'true\n'*15)
    receipt['passed']=True;save_json(work/'results.json',receipt)
    print('ordinary-records: passed; '+str(work/'results.json'))

if __name__=='__main__':main()
