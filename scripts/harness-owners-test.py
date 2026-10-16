#!/usr/bin/env python3
"""Test/Bench owned state, deterministic timing contract, retention and image persistence."""
import argparse
import json
from pathlib import Path
import tempfile
from validation_support import ROOT, clean_environment, command, save_json, sha


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix',type=Path);parser.add_argument('--skip-images',action='store_true')
    args=parser.parse_args();prefix=args.prefix.resolve()
    work=Path(tempfile.mkdtemp(prefix='birnam-harness-owners-'))
    env=clean_environment();env['BIRNAM_GC_PRECISE']='1';binary=prefix/'bin/birnam'
    sources=[ROOT/f for f in ['modules/test.birnam','modules/bench.birnam','tests/fixtures/harness-owners.birnam','rust/birnam-siphon/tests/harness_retention.rs','scripts/harness-owners-test.py']]
    receipt=dict(passed=False,prefix=str(prefix),runs=[],source_sha256={str(f.relative_to(ROOT)):sha(f) for f in sources})
    def run(name,argv,*,cwd=ROOT,native=False,extra=None,timeout=300):
        selected=dict(env)
        if extra:selected.update(extra)
        if native:selected.update(BIRNAM_TEST_MODULE_DIRECTORY=str(prefix/'lib'),CARGO_TARGET_DIR=str(ROOT/'.cargo-target/native-message-tests'),CARGO_PROFILE_DEV_PANIC='unwind')
        result=command(argv,cwd=cwd,env=selected,timeout=timeout)
        receipt['runs'].append(dict(name=name,**result));save_json(work/'results.json',receipt)
        (work/(name+'.log')).write_text(result['output'])
        assert result['exit_code']==0,(name,work,result)
        if not native:assert not result['stderr'],(name,work,result)
        return result['stdout']
    out=run('semantics',[binary,ROOT/'tests/fixtures/harness-owners.birnam'])
    assert out.endswith('harness-owners: passed\n'),(work,out)
    result=json.loads(next(line for line in out.splitlines() if line.startswith('{"suite"')))
    assert result=={'suite':'owned-bench','cases':[
        dict(name='subtract',ns_per_op=0,ns_per_op_raw=5000,net_us=0,ops=8,iters=4,reps=3,median_us=40,min_us=40),
        dict(name='raw',ns_per_op=10000,ns_per_op_raw=10000,net_us=40,ops=4,iters=4,reps=3,median_us=40,min_us=40),
        dict(name='fixed',ns_per_op=0,ns_per_op_raw=10000,net_us=0,ops=3,iters=3,reps=3,median_us=30,min_us=30),
        dict(name='external',ns_per_op=3500,ns_per_op_raw=3500,net_us=7,ops=2,iters=1,reps=1,median_us=7,min_us=7)]},result
    run('retention',['mise','exec','--','cargo','test','--offline','--locked','-p','birnam-siphon','--test','harness_retention','--','--include-ignored','--nocapture'],native=True)
    # Exercise the real clock, GC windows, writer and output-path override too.
    source=work/'real.birnam';output=work/'result.json'
    source.write_text('[System load: '+json.dumps(str(prefix/'share/birnam/src/modules/bench.birnam'))+']; let s := [BenchSuite named: "real"]; [s fixedCase: "loop" iters: 2 do: [:n | [1 to: n do: [:i | nil]]]]; [Bench runSuite: s]; [Bench writeResults: "ignored.json"]')
    run('real-clock-and-file',[binary,source],extra={'BENCH_QUICK':'1','BENCH_OUT':str(output)})
    data=json.loads(output.read_text());case=data['cases'][0]
    assert data['suite']=='real' and case['name']=='loop' and case['iters']==2 and case['reps']==2
    assert case['ops']==2 and case['ns_per_op']>=0 and case['ns_per_op_raw']>=0 and case['median_us']>=case['min_us']>=0
    if not args.skip_images:
        source=work/'save.birnam';image=work/'harness.image'
        source.write_text('''[Module load: #os]; [System load: TEST]; [System load: BENCH];
[[|
let check := [:ok | [ok ifFalse: [| [System printErrorText: "harness image failure\\n"]; [OS exit: 1]]]];
let count := [0 asCell]; let alias := count;
let suite := [TestSuite named: "saved" module: #savedHarness];
[suite test: "captured" do: [:t | [count inc]; [t assert: true describes: "captured"]]];
let bench := [Bench clone]; [Reflect slot: #results in: bench put: #[]];
[Reflect slot: #suiteName in: bench put: "saved-bench"]; [bench record: "saved-case" us: 12 ops: 3];
let descriptions := db; [Test describeSuitesIn: descriptions];
[Globals at: #main put: [|
  [System gc]; [check value: [count == alias]];
  [check value: [[[Test suitesForModule: #savedHarness] at: 0] == suite]];
  [Test runTestsForModule: #savedHarness withRunner: ConsoleRunner];
  [check value: [[count value] = 1]];
  [check value: [[ConsoleRunner passedCount] = 1]];
  [Reflect slot: #cblock in: [[suite cases] at: 0] put: [:t | [count set: [[count value] + 2]]; t]];
  [Test runSuite: suite withRunner: ConsoleRunner]; [check value: [[count value] = 3]];
  [check value: [[[[bench results] at: 0] nsOp] = 4000]];
  [check value: [[bench suiteName] = "saved-bench"]];
  [Test unregister: suite]; [check value: [[[Test suitesForModule: #savedHarness] size] = 0]];
  [check value: [[descriptions currentValue: suite attr: #isTestSuite] == #true]];
  let before := [db retainedFactCount]; let fresh := [TestSuite named: "fresh" module: #freshHarness];
  [Test unregister: fresh]; [check value: [[db retainedFactCount] = before]];
  [System println: "harness-image: passed"]
]];
[System saveImage: IMAGE]; [System println: [[System imageSaveReport] status]]
] value]
'''.replace('TEST',json.dumps(str(prefix/'share/birnam/src/modules/test.birnam'))).replace('BENCH',json.dumps(str(prefix/'share/birnam/src/modules/bench.birnam'))).replace('IMAGE',json.dumps(str(image))))
        saved=run('save',[binary,source])
        assert saved.endswith('#saved\n') and image.exists(),(work,saved)
        receipt['image_sha256']=sha(image);source.unlink()
        for mode,flags in [('mapped',[]),('rebuilt',['--rebuild'])]:
            assert run(mode,[binary,*flags,image],cwd=work).endswith('harness-image: passed\n')
    receipt['passed']=True;save_json(work/'results.json',receipt)
    print('harness-owners: passed; '+str(work/'results.json'))

if __name__=='__main__':main()
